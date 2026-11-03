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
    let (status, observed) =
        relayer.request("POST", "/checks/observe", observe_request(&lab, "W1"));
    assert_eq!(status, 200);
    assert_eq!(
        observed["facts"]["intent"]["value"]["prepared_identities"]["value"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        observed["facts"]["intent"]["value"]["original_signature"],
        terminal["original_signature"]
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

fn observe_request(lab: &Lab, intent: &str) -> serde_json::Value {
    json!({"contract_version":1,"invocation_id":"C1","check_id":"intent-state","lab_id":lab.id,"snapshot_seq":42,"scope":{"intent_ids":[intent]},"facts":["intent"],"deadline":(time::OffsetDateTime::now_utc()+time::Duration::seconds(5)).format(&time::format_description::well_known::Rfc3339).unwrap()})
}
#[test]
fn observations_are_read_only_consistent_explicit_and_do_not_release_the_worker() {
    let lab = Lab::new();
    let mut relayer = lab.start("resilient", Some("prepared"));
    relayer.request("POST", "/intents", input());
    lab.barrier("prepared");
    let before = relayer.intent();
    let registrations =
        lab.command(&["investigate", "summary"])["result"]["counts"]["rpc_registrations"].clone();
    for _ in 0..5 {
        let (status, observation) =
            relayer.request("POST", "/checks/observe", observe_request(&lab, "W1"));
        assert_eq!(status, 200, "{observation}");
        assert_eq!(observation["source"], "reference-relayer");
        assert_eq!(observation["invocation_id"], "C1");
        assert_eq!(observation["snapshot_seq"], 42);
        assert_eq!(
            observation["revision"],
            before["revision"].as_i64().unwrap().to_string()
        );
        assert_eq!(observation["facts"]["intent"]["availability"], "available");
        assert_eq!(
            observation["facts"]["intent"]["value"]["original_signature"],
            before["original_signature"]
        );
        assert_eq!(observation["facts"]["intent"]["value"]["state"], "prepared");
        assert!(observation.get("verdict").is_none());
        assert!(!observation.to_string().contains("wire_base64"));
        assert!(!observation.to_string().contains("signing.seed"));
    }
    assert_eq!(relayer.intent(), before);
    assert_eq!(
        lab.command(&["investigate", "summary"])["result"]["counts"]["rpc_registrations"],
        registrations
    );
    let missing = relayer
        .request("POST", "/checks/observe", observe_request(&lab, "missing"))
        .1;
    assert_eq!(missing["facts"]["intent"]["availability"], "unavailable");
    assert!(missing.get("revision").is_none());
    assert!(missing["facts"]["intent"].get("value").is_none());
    let mut unsupported = observe_request(&lab, "W1");
    unsupported["facts"] = json!(["balance"]);
    assert_eq!(
        relayer.request("POST", "/checks/observe", unsupported).0,
        400
    );
    let mut expired = observe_request(&lab, "W1");
    expired["deadline"] = json!("2020-01-01T00:00:00Z");
    assert_eq!(relayer.request("POST", "/checks/observe", expired).0, 408);
    relayer.stop();
    assert_eq!(lab.command(&["status"])["result"]["state"], "ready");
}

#[test]
fn observation_scope_names_and_body_budget_are_validated_without_creating_work() {
    use base64::Engine;
    let lab = Lab::new();
    let mut relayer = lab.start("resilient", None);
    for variation in 0..4 {
        let mut request = observe_request(&lab, "missing");
        match variation {
            0 => request["lab_id"] = json!("another-lab"),
            1 => request["scope"]["intent_ids"] = json!([]),
            2 => request["contract_version"] = json!(2),
            _ => request["facts"] = json!(["intent", "unknown"]),
        }
        assert_eq!(relayer.request("POST", "/checks/observe", request).0, 400);
    }
    let mut oversized = observe_request(&lab, "missing");
    oversized["facts"] = json!(["X".repeat(8192)]);
    assert_eq!(relayer.request("POST", "/checks/observe", oversized).0, 400);
    let (status, response) =
        relayer.request("POST", "/checks/observe", observe_request(&lab, "missing"));
    assert_eq!(status, 200);
    assert!(response.to_string().len() <= 256 * 1024);
    let connection =
        rusqlite::Connection::open(lab.reference_dir().join("relayer.sqlite")).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM intents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let seed = std::fs::read(lab.reference_dir().join("signing.seed")).unwrap();
    let hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();
    assert!(!response.to_string().contains(&hex));
    assert!(
        !response
            .to_string()
            .contains(&base64::engine::general_purpose::STANDARD.encode(seed))
    );
    relayer.stop();
}
