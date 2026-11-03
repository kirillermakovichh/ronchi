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
