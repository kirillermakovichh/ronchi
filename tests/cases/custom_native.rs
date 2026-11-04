//! Native Custom Case histories use explicit local funding and the pinned owned backend.
use super::*;
use ed25519_dalek::SigningKey;
use serde_json::json;
use std::time::{Duration, Instant};

fn artifact(primitive: &str, origin: &str) -> Value {
    json!({"schema":"ronchi.case/v1","id":"custom.native-lookup","revision":"1",
        "requires":{"primitives":[primitive,"observe.rpc@1"]},"modes":["surfpool"],
        "filter":{"methods":["getSignatureStatuses","getTransaction"],"transaction_origin":origin},
        "state":{"key":"transaction_signature","scope":"activation","max_keys":8,"on_exhausted":"error"},
        "sequence":[{"id":"fault","using":primitive,"until":{"matches":1},"next":"observe"},
            {"id":"observe","using":"observe.rpc@1","until":{"matches":1},"next":"done"}]})
}
fn activate(p: &Project, artifact: &Value) -> Value {
    let path = p.path().join("custom-case.yaml");
    std::fs::write(&path, artifact.to_string()).unwrap();
    p.ok(&["case", "activate", path.to_str().unwrap()])
}
fn submit(address: &str, recipient: u8) -> String {
    let hash = rpc(address, "getLatestBlockhash", json!([]))["result"]["value"]["blockhash"]
        .as_str()
        .unwrap()
        .to_owned();
    let (signature, wire) = transfer(&hash, 144, [recipient; 32]);
    assert_eq!(
        rpc(
            address,
            "sendTransaction",
            json!([wire,{"encoding":"base64"}])
        )["result"],
        signature
    );
    let account = bs58::encode([recipient; 32]).into_string();
    let deadline = Instant::now() + Duration::from_secs(8);
    while rpc(address, "getBalance", json!([account]))["result"]["value"] != 1_000_000u64 {
        assert!(
            Instant::now() < deadline,
            "native transfer was not observed"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    signature
}
fn native() -> Option<(Project, Value)> {
    let archive = archive()?;
    let p = Project::new();
    let payer = bs58::encode(
        SigningKey::from_bytes(&[144; 32])
            .verifying_key()
            .to_bytes(),
    )
    .into_string();
    funded_snapshot(&p, &payer);
    install(&p, &archive);
    let ready = p.ok(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    Some((p, ready))
}
fn events(p: &Project, ready: &Value) -> Vec<ronchi::journal::Event> {
    let reader = ronchi::journal::JournalReader::open(
        &p.path()
            .join("ronchi/labs")
            .join(ready["lab_id"].as_str().unwrap())
            .join("journal.sqlite"),
    )
    .unwrap();
    (1..=reader.snapshot(false).unwrap().snapshot_seq)
        .map(|seq| reader.event(seq).unwrap().unwrap())
        .collect()
}

#[test]
fn real_custom_null_observe_origin_and_mode_rejection_preserve_native_authority() {
    let Some((p, ready)) = native() else {
        return;
    };
    let address = ready["result"]["rpc_address"].as_str().unwrap();
    let old = submit(address, 145);
    let case = artifact("rpc.lookup-null@1", "accepted_under_this_activation");
    let activated = activate(&p, &case);
    assert_eq!(
        activated["result"]["active_case"]["custom"]["origin"],
        "custom"
    );
    let previous = rpc(address, "getSignatureStatuses", json!([[old]]));
    assert!(previous["result"]["value"][0].is_object(), "{previous}");
    let current = submit(address, 146);
    let unknown = bs58::encode([147; 64]).into_string();
    let hidden = rpc(
        address,
        "getSignatureStatuses",
        json!([[current, old, unknown, current]]),
    );
    assert_eq!(hidden["result"]["value"].as_array().unwrap().len(), 4);
    assert_eq!(hidden["result"]["value"][0], Value::Null);
    assert_eq!(hidden["result"]["value"][2], Value::Null);
    assert_eq!(hidden["result"]["value"][3], Value::Null);
    assert!(hidden["result"]["value"][1].is_object(), "{hidden}");
    let observed = rpc(address, "getSignatureStatuses", json!([[current]]));
    assert!(observed["result"]["value"][0].is_object(), "{observed}");
    let known = activate(&p, &artifact("rpc.lookup-null@1", "known_local"));
    let known_hidden = rpc(
        address,
        "getTransaction",
        json!([old,{"commitment":"confirmed","encoding":"base64"}]),
    );
    assert_eq!(
        known_hidden.get("result"),
        Some(&Value::Null),
        "{known_hidden}"
    );
    let visible = rpc(
        address,
        "getTransaction",
        json!([old,{"commitment":"confirmed","encoding":"base64"}]),
    );
    assert!(visible["result"].is_object(), "{visible}");
    let mut unsupported = artifact("sim.confirmation-delay@1", "known_local");
    unsupported["filter"]["methods"] = json!(["sendTransaction"]);
    let path = p.path().join("unsupported.yaml");
    std::fs::write(&path, unsupported.to_string()).unwrap();
    let rejected = p.run(&["case", "activate", path.to_str().unwrap()]);
    assert!(!rejected.status.success(), "{rejected:?}");
    let error: Value = serde_json::from_slice(&rejected.stdout).unwrap();
    assert_eq!(error["error"]["code"], "unsupported_mode", "{error}");
    let status = p.ok(&["status"]);
    assert_eq!(
        status["result"]["active_case"]["revision"],
        known["result"]["active_case"]["revision"]
    );
    p.ok(&["down"]);
    let events = events(&p, &ready);
    let accepted: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "transaction.accepted")
        .collect();
    assert_eq!(accepted.len(), 2);
    for event in accepted {
        assert_eq!(event.source, "surfpool");
        assert_eq!(event.data["authority"], "execution_backend");
        assert!(event.data["next_due_ns"].is_null());
        assert!(
            event.data["source_identity"]
                .as_str()
                .unwrap()
                .starts_with("surfpool:")
        );
    }
    assert!(
        events
            .iter()
            .all(|e| e.execution_mode == ronchi::config::Mode::Surfpool)
    );
    assert!(events.iter().all(|e| e.kind != "transaction.transition"));
    assert!(events.iter().any(|e| e.kind == "rpc.custom_lookup_source"
        && e.source == "surfpool"
        && e.data["source_available"] == true));
    assert!(
        events
            .iter()
            .any(|e| e.kind == "fault.custom_lookup_changed"
                && e.data["selection"]["primitive"] == "rpc.lookup-null@1")
    );
    assert!(events.iter().any(
        |e| e.kind == "custom.selection" && e.data["selection"]["primitive"] == "observe.rpc@1"
    ));
}

#[test]
fn real_custom_snapshot_reuses_only_observed_native_views_then_observes() {
    let Some((p, ready)) = native() else {
        return;
    };
    let address = ready["result"]["rpc_address"].as_str().unwrap();
    let signature = submit(address, 148);
    let mut case = artifact("rpc.lookup-snapshot@1", "known_local");
    case["sequence"][0]["until"] = json!({"matches":3});
    activate(&p, &case);
    let params = json!([signature,{"encoding":"base64","commitment":"confirmed"}]);
    let first = rpc(address, "getTransaction", params.clone());
    assert!(first["result"].is_object(), "{first}");
    let status = rpc(address, "getSignatureStatuses", json!([[signature]]));
    let view = status["result"]["value"][0].clone();
    assert!(view.is_object(), "{status}");
    assert!(matches!(
        view["confirmationStatus"].as_str(),
        Some("confirmed" | "finalized")
    ));
    assert!(view.get("err").is_some());
    assert!(view.get("confirmations").is_some());
    assert_eq!(
        rpc(address, "getTransaction", params.clone())["result"],
        first["result"]
    );
    let observed = rpc(address, "getTransaction", params);
    assert!(observed["result"].is_object(), "{observed}");
    p.ok(&["down"]);
    let events = events(&p, &ready);
    let lifecycle: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "rpc.custom_lifecycle_source")
        .collect();
    assert!(!lifecycle.is_empty());
    assert_eq!(lifecycle[0].data["status_view"], view);
    for event in lifecycle {
        assert_eq!(event.source, "surfpool");
        assert!(
            event.data["source_identity"]
                .as_str()
                .unwrap()
                .starts_with("surfpool:")
        );
    }
    assert!(events.iter().any(
        |e| e.kind == "custom.selection" && e.data["selection"]["primitive"] == "observe.rpc@1"
    ));
    assert!(events.iter().any(|e| e.kind == "custom.lookup_observed"
        && e.data["selection"]["primitive"] == "rpc.lookup-snapshot@1"));
    assert!(!events.iter().any(|e| e.kind == "transaction.transition"));
}
