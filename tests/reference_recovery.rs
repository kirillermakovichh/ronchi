mod reference_support;
use reference_support::*;
use serde_json::json;
#[test]
fn foreground_intake_is_idempotent_private_and_independent_of_lab_lifecycle() {
    use std::os::unix::fs::PermissionsExt;
    let lab = Lab::new();
    let mut relayer = lab.start("resilient", Some("prepared"));
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
    lab.barrier("prepared");
    let db = rusqlite::Connection::open(lab.reference_dir().join("relayer.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM intents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM identities", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
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

fn saved_wires(lab: &Lab) -> Vec<(String, String)> {
    let connection =
        rusqlite::Connection::open(lab.reference_dir().join("relayer.sqlite")).unwrap();
    let mut stmt = connection
        .prepare("SELECT signature,wire_base64 FROM identities ORDER BY generation")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}
#[test]
fn crash_after_prepared_reopens_exact_bytes_and_queries_before_first_send() {
    let lab = Lab::new();
    let mut relayer = lab.start("resilient", Some("prepared"));
    relayer.request("POST", "/intents", input());
    lab.barrier("prepared");
    let before = relayer.intent();
    let wires = saved_wires(&lab);
    assert_eq!(wires.len(), 1);
    assert!(
        before["submission_attempts"]["value"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let submitted = lab.command(&[
        "investigate",
        "timeline",
        "--kind",
        "transaction.submission",
    ]);
    assert!(submitted["result"]["items"].as_array().unwrap().is_empty());
    relayer.crash();
    let mut recovered = lab.start("resilient", None);
    let terminal = recovered.wait(|v| v["state"] == "succeeded");
    assert_eq!(saved_wires(&lab), wires);
    assert_eq!(terminal["original_signature"], before["original_signature"]);
    assert_eq!(terminal["deadline_ms"], before["deadline_ms"]);
    let observations = terminal["observations"]["value"].as_array().unwrap();
    let recovery = observations
        .iter()
        .position(|o| o["kind"] == "recovery_prepared")
        .unwrap();
    let status = observations
        .iter()
        .position(|o| o["kind"] == "status")
        .unwrap();
    let attempt = observations
        .iter()
        .position(|o| o["kind"] == "attempt_start")
        .unwrap();
    assert!(recovery < status && status < attempt);
    recovered.stop();
}

#[test]
fn crash_after_attempt_start_does_not_invent_timeout_or_send_on_restart() {
    let lab = Lab::new();
    let mut relayer = lab.start_options("unsafe", Some("attempt_start"), Some(2), None);
    relayer.request("POST", "/intents", input());
    lab.barrier("attempt_start");
    let before = relayer.intent();
    let wires = saved_wires(&lab);
    assert_eq!(before["state"], "submitting");
    assert_eq!(
        before["submission_attempts"]["value"][0]["result"]["availability"],
        "unavailable"
    );
    relayer.crash();
    let mut recovered = lab.start("unsafe", None);
    let terminal = recovered.wait(|v| v["state"] == "unresolved");
    assert_eq!(saved_wires(&lab), wires);
    assert_eq!(terminal["deadline_ms"], before["deadline_ms"]);
    assert_eq!(
        terminal["prepared_identities"]["value"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        terminal["observations"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "recovery_unknown")
    );
    assert!(
        !terminal["observations"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "client")
    );
    let submitted = lab.command(&[
        "investigate",
        "timeline",
        "--kind",
        "transaction.submission",
    ]);
    assert!(submitted["result"]["items"].as_array().unwrap().is_empty());
    recovered.stop();
}

#[test]
fn crash_after_rpc_acceptance_reconciles_without_fabricating_a_client_result() {
    let lab = Lab::new();
    let mut relayer = lab.start("unsafe", Some("rpc_result"));
    relayer.request("POST", "/intents", input());
    lab.barrier("rpc_result");
    let before = relayer.intent();
    let wires = saved_wires(&lab);
    assert_eq!(before["state"], "submitting");
    assert_eq!(
        before["submission_attempts"]["value"][0]["result"]["availability"],
        "unavailable"
    );
    let accepted = lab.command(&["investigate", "summary"]);
    assert_eq!(accepted["result"]["counts"]["accepted_transactions"], 1);
    relayer.crash();
    let mut recovered = lab.start("unsafe", None);
    let terminal = recovered.wait(|v| v["state"] == "succeeded");
    assert_eq!(saved_wires(&lab), wires);
    assert_eq!(terminal["original_signature"], before["original_signature"]);
    assert_eq!(
        terminal["prepared_identities"]["value"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        terminal["submission_attempts"]["value"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !terminal["observations"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "client")
    );
    assert!(
        terminal["observations"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "recovery_unknown")
    );
    recovered.stop();
}

#[test]
fn crash_after_replacement_prepared_keeps_two_identities_across_repeated_restarts() {
    let lab = Lab::new();
    lab.command(&["case", "arm", "tx-response-lost", "--hold-ms", "150"]);
    let mut relayer = lab.start("unsafe", Some("replacement_prepared"));
    relayer.request("POST", "/intents", input());
    lab.barrier("replacement_prepared");
    let before = relayer.intent();
    let wires = saved_wires(&lab);
    assert_eq!(wires.len(), 2);
    assert_ne!(before["original_signature"], before["tracked_signature"]);
    assert_eq!(
        before["submission_attempts"]["value"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    relayer.crash();
    let mut recovered = lab.start("unsafe", None);
    let terminal = recovered.wait(|v| v["state"] == "succeeded");
    assert_eq!(saved_wires(&lab), wires);
    assert_eq!(terminal["original_signature"], before["original_signature"]);
    assert_eq!(terminal["tracked_signature"], before["tracked_signature"]);
    assert_eq!(
        terminal["submission_attempts"]["value"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    recovered.crash();
    for _ in 0..2 {
        let mut recovered = lab.start("unsafe", None);
        assert_eq!(recovered.intent(), terminal);
        assert_eq!(saved_wires(&lab), wires);
        recovered.crash();
    }
}

#[test]
fn crash_after_terminal_commit_preserves_terminal_snapshot_without_new_rpc() {
    let lab = Lab::new();
    let mut relayer = lab.start("resilient", Some("terminal"));
    relayer.request("POST", "/intents", input());
    lab.barrier("terminal");
    let terminal = relayer.intent();
    assert_eq!(terminal["state"], "succeeded");
    let wires = saved_wires(&lab);
    let before =
        lab.command(&["investigate", "summary"])["result"]["counts"]["rpc_registrations"].clone();
    relayer.crash();
    let mut recovered = lab.start("resilient", None);
    assert_eq!(recovered.intent(), terminal);
    assert_eq!(saved_wires(&lab), wires);
    assert_eq!(
        lab.command(&["investigate", "summary"])["result"]["counts"]["rpc_registrations"],
        before
    );
    recovered.stop();
}

fn lab_replacement_history(request_new: bool) {
    let lab = Lab::new();
    let mut relayer = lab.start("resilient", Some("prepared"));
    relayer.request("POST", "/intents", input());
    lab.barrier("prepared");
    let before = relayer.intent();
    let wires = saved_wires(&lab);
    relayer.crash();
    lab.command(&["down", "--force"]);
    let new_lab = lab.command(&[
        "up",
        "--detach",
        "--rpc-listen",
        &lab.rpc.to_string(),
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let new_id = new_lab["lab_id"].as_str().unwrap();
    assert_ne!(new_id, lab.id);
    let mut recovered = lab.start_options(
        "resilient",
        None,
        None,
        if request_new { Some(new_id) } else { None },
    );
    let unresolved = recovered.wait(|v| v["state"] == "unresolved");
    assert_eq!(unresolved["lab_id"], lab.id);
    assert_eq!(
        unresolved["original_signature"],
        before["original_signature"]
    );
    assert_eq!(saved_wires(&lab), wires);
    assert!(
        unresolved["observations"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "source_gap")
    );
    let submitted = lab.command(&[
        "investigate",
        "timeline",
        "--kind",
        "transaction.submission",
    ]);
    assert!(submitted["result"]["items"].as_array().unwrap().is_empty());
    recovered.stop();
}

#[test]
fn recovery_never_restarts_the_original_acceptance_horizon() {
    let lab = Lab::new();
    let mut relayer = lab.start_options("resilient", Some("prepared"), Some(1), None);
    relayer.request("POST", "/intents", input());
    lab.barrier("prepared");
    let before = relayer.intent();
    let deadline = before["deadline_ms"].as_i64().unwrap();
    relayer.crash();
    let bound = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000 < i128::from(deadline)
    {
        assert!(std::time::Instant::now() < bound);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let mut recovered = lab.start("resilient", None);
    let unresolved = recovered.wait(|v| v["state"] == "unresolved");
    assert_eq!(unresolved["deadline_ms"], before["deadline_ms"]);
    assert_eq!(unresolved["accepted_at_ms"], before["accepted_at_ms"]);
    assert!(
        unresolved["submission_attempts"]["value"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    recovered.stop();
    let output = base(lab.path())
        .args([
            "--lab-id",
            &lab.id,
            "reference",
            "relayer",
            "--profile",
            "resilient",
            "--horizon-seconds",
            "60",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
}

#[test]
fn another_lab_on_the_same_rpc_address_cannot_replace_the_pinned_source() {
    lab_replacement_history(true);
}
#[test]
fn original_lab_recovery_does_not_use_a_replacement_lab_on_the_same_url() {
    lab_replacement_history(false);
}

#[test]
fn a_changed_expected_genesis_is_a_source_gap_without_submission() {
    let lab = Lab::new();
    let mut relayer = lab.start("resilient", Some("prepared"));
    relayer.request("POST", "/intents", input());
    lab.barrier("prepared");
    let wires = saved_wires(&lab);
    relayer.crash();
    let connection =
        rusqlite::Connection::open(lab.reference_dir().join("relayer.sqlite")).unwrap();
    connection
        .execute(
            "UPDATE config SET binding_json=json_set(binding_json,'$.genesis_hash',?1)",
            [bs58::encode([0u8; 32]).into_string()],
        )
        .unwrap();
    drop(connection);
    let mut recovered = lab.start("resilient", None);
    let unresolved = recovered.wait(|v| v["state"] == "unresolved");
    assert_eq!(saved_wires(&lab), wires);
    assert!(
        unresolved["observations"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "source_gap")
    );
    assert!(
        lab.command(&[
            "investigate",
            "timeline",
            "--kind",
            "transaction.submission"
        ])["result"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    recovered.stop();
}

#[test]
fn stopping_an_inflight_relayer_joins_transport_and_preserves_unknown_outcome() {
    let lab = Lab::new();
    lab.command(&["case", "arm", "tx-response-lost", "--hold-ms", "10000"]);
    let mut relayer = lab.start("unsafe", None);
    relayer.request("POST", "/intents", input());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    loop {
        if lab.command(&["investigate", "summary"])["result"]["counts"]["accepted_transactions"]
            == 1
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let before = relayer.intent();
    assert_eq!(before["state"], "submitting");
    let wires = saved_wires(&lab);
    relayer.stop();
    assert_eq!(lab.command(&["status"])["result"]["state"], "ready");
    assert_eq!(
        http(
            lab.rpc,
            "POST",
            "/",
            json!({"jsonrpc":"2.0","id":1,"method":"getHealth"})
        )
        .1["result"],
        "ok"
    );
    let mut recovered = lab.start("unsafe", None);
    let terminal = recovered.wait(|v| v["state"] == "succeeded");
    assert_eq!(saved_wires(&lab), wires);
    assert_eq!(
        terminal["prepared_identities"]["value"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !terminal["observations"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "client")
    );
    recovered.stop();
}
