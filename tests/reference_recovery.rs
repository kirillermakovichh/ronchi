mod reference_support;
use reference_support::*;
use serde_json::json;
#[test]
fn foreground_intake_is_idempotent_private_and_independent_of_lab_lifecycle() {
    use std::os::unix::fs::PermissionsExt;
    let lab = Lab::new();
    let mut relayer = lab.start("resilient", None);
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let address = relayer.address;
            std::thread::spawn(move || http(address, "POST", "/intents", input()))
        })
        .collect();
    let mut digest = None;
    for handle in handles {
        let (status, intent) = handle.join().unwrap();
        assert_eq!(status, 200, "{intent}");
        assert_eq!(intent["intent_id"], "W1");
        if let Some(digest) = &digest {
            assert_eq!(&intent["payload_digest"], digest);
        }
        digest = Some(intent["payload_digest"].clone());
    }
    let mut changed = input();
    changed["amount_lamports"] = json!(8);
    assert_eq!(relayer.request("POST", "/intents", changed).0, 409);
    let seed = std::fs::read(lab.reference_dir().join("signing.seed")).unwrap();
    assert_eq!(seed.len(), 32);
    assert_eq!(
        std::fs::metadata(lab.reference_dir().join("signing.seed"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(lab.reference_dir())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let response = relayer.intent().to_string();
    assert!(!response.contains("wire_base64"));
    assert!(!response.contains("signing.seed"));
    relayer.stop();
    assert_eq!(lab.command(&["status"])["result"]["state"], "ready");
    let mut recovered = lab.start("resilient", None);
    assert_eq!(recovered.intent()["payload_digest"], digest.unwrap());
    recovered.stop();
    let output = base(lab.path())
        .args([
            "--lab-id",
            &lab.id,
            "reference",
            "relayer",
            "--profile",
            "unsafe",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(output.stderr.is_empty());
}

#[test]
fn resilient_response_loss_keeps_one_identity_and_requires_finalized_status() {
    let lab = Lab::new();
    lab.command(&["case", "arm", "tx-response-lost", "--hold-ms", "10000"]);
    let mut relayer = lab.start("resilient", None);
    assert_eq!(relayer.request("POST", "/intents", input()).0, 200);
    let reconciling = relayer.wait(|v| v["state"] == "reconciling");
    assert!(
        reconciling["observations"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "client" && o["data"]["client_outcome"]["outcome"] == "timeout")
    );
    assert_ne!(reconciling["state"], "failed");
    assert_ne!(reconciling["state"], "succeeded");
    let terminal = relayer.wait(|v| v["state"] == "succeeded" || v["state"] == "unresolved");
    assert_eq!(terminal["state"], "succeeded", "{terminal}");
    assert_eq!(
        terminal["prepared_identities"]["value"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        terminal["original_signature"],
        terminal["tracked_signature"]
    );
    assert!(
        terminal["observations"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "status"
                && o["data"]["status"]["value"]["confirmationStatus"] == "finalized")
    );
    relayer.stop();
}

#[test]
fn unsafe_prepares_exactly_one_replacement_even_with_the_same_blockhash() {
    let lab = Lab::new_with_slot(60000);
    lab.command(&["case", "arm", "tx-response-lost", "--hold-ms", "300"]);
    let mut relayer = lab.start("unsafe", None);
    assert_eq!(relayer.request("POST", "/intents", input()).0, 200);
    let terminal = relayer.wait(|v| v["state"] == "succeeded" || v["state"] == "unresolved");
    assert_eq!(terminal["state"], "succeeded", "{terminal}");
    let identities = terminal["prepared_identities"]["value"].as_array().unwrap();
    assert_eq!(identities.len(), 2);
    assert_eq!(
        identities[0]["validity_inputs"]["blockhash"],
        identities[1]["validity_inputs"]["blockhash"]
    );
    assert_ne!(identities[0]["signature"], identities[1]["signature"]);
    assert_eq!(
        terminal["original_signature"]["value"],
        identities[0]["signature"]
    );
    assert_eq!(
        terminal["tracked_signature"]["value"],
        identities[1]["signature"]
    );
    assert_eq!(
        terminal["submission_attempts"]["value"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    relayer.stop();
    let mut recovered = lab.start("unsafe", None);
    assert_eq!(recovered.intent(), terminal);
    recovered.stop();
}

#[test]
fn null_status_and_submission_acceptance_are_not_terminal_success_or_failure() {
    let lab = Lab::new();
    let mut relayer = lab.start("resilient", None);
    assert_eq!(relayer.request("POST", "/intents", input()).0, 200);
    let pending = relayer.wait(|v| {
        v["observations"]["value"].as_array().is_some_and(|obs| {
            obs.iter().any(|o| {
                o["kind"] == "status"
                    && o["data"]["status"]["availability"] == "available"
                    && o["data"]["status"]["value"].is_null()
            })
        })
    });
    assert_eq!(pending["state"], "reconciling");
    let terminal = relayer.wait(|v| v["state"] == "succeeded");
    assert_eq!(
        terminal["outcome_interpretation"],
        "simulated_lifecycle_only"
    );
    relayer.stop();
}

#[test]
fn failed_identity_commit_prevents_submission_and_stops_the_process() {
    use std::io::Read;
    let lab = Lab::new();
    let mut relayer = lab.start("resilient", Some("before_prepare_commit"));
    assert_eq!(relayer.request("POST", "/intents", input()).0, 200);
    lab.barrier("before_prepare_commit");
    let connection =
        rusqlite::Connection::open(lab.reference_dir().join("relayer.sqlite")).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_identity BEFORE INSERT ON identities BEGIN SELECT RAISE(ABORT,'injected identity write failure'); END;").unwrap();
    lab.release();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let status = loop {
        if let Some(status) = relayer.child.try_wait().unwrap() {
            break status;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert_eq!(status.code(), Some(6));
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM identities", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let submitted = lab.command(&[
        "investigate",
        "timeline",
        "--kind",
        "transaction.submission",
    ]);
    assert!(submitted["result"]["items"].as_array().unwrap().is_empty());
    let seed = std::fs::read(lab.reference_dir().join("signing.seed")).unwrap();
    let mut diagnostics = String::new();
    relayer
        .output
        .by_ref()
        .take(65536)
        .read_to_string(&mut diagnostics)
        .unwrap();
    relayer
        .child
        .stderr
        .as_mut()
        .unwrap()
        .take(65536)
        .read_to_string(&mut diagnostics)
        .unwrap();
    let hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();
    assert!(!diagnostics.contains(&hex));
    assert!(!diagnostics.contains("injected identity"));
}
