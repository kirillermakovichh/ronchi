use super::*;
#[test]
fn real_native_expiry_uses_backend_height_and_preserves_safehash_admission() {
    use ed25519_dalek::SigningKey;
    use std::time::{Duration, Instant};
    let Some(archive) = archive() else {
        eprintln!(
            "native blockhash fixture unavailable; actual native expiry remains unverified in this invocation"
        );
        return;
    };
    let p = Project::new();
    let payer =
        bs58::encode(SigningKey::from_bytes(&[54; 32]).verifying_key().to_bytes()).into_string();
    funded_snapshot(&p, &payer);
    install(&p, &archive);
    let started = p.ok(&[
        "up",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let address = started["result"]["rpc_address"].as_str().unwrap();
    p.ok(&["case", "arm", "tx-blockhash-expired"]);
    let captured = rpc(address, "getLatestBlockhash", serde_json::json!([]))["result"].clone();
    let bound = captured["value"]["lastValidBlockHeight"].as_u64().unwrap();
    let hash = captured["value"]["blockhash"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut first = None;
    loop {
        let height = rpc(address, "getBlockHeight", serde_json::json!([]))["result"]
            .as_u64()
            .unwrap();
        first.get_or_insert(height);
        if height > bound {
            eprintln!(
                "native source expiry observed: initial_height={} lastValidBlockHeight={bound} observed_height={height}",
                first.unwrap()
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native source did not cross declared validity bound {bound}; last height {height}"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
    assert_eq!(
        rpc(address, "getLatestBlockhash", serde_json::json!([]))["result"],
        captured
    );
    let (signature, wire) = transfer(hash, 54, [55; 32]);
    let result = rpc(
        address,
        "sendTransaction",
        serde_json::json!([wire,{"encoding":"base64"}]),
    );
    assert!(
        result["result"] == signature || result.get("error").is_some(),
        "{result}"
    );
    let status = rpc(
        address,
        "getSignatureStatuses",
        serde_json::json!([[signature]]),
    );
    assert!(status["result"]["value"].is_array(), "{status}");
    eprintln!(
        "native expired safehash observed result={result}; status={}",
        status["result"]["value"][0]
    );
    assert_ne!(
        rpc(address, "getLatestBlockhash", serde_json::json!([]))["result"],
        captured
    );
    let findings = p.ok(&["investigate", "findings"]);
    let items = findings["result"]["items"].as_array().unwrap();
    let finding = items
        .iter()
        .find(|finding| finding["rule_id"] == "recent-blockhash.expired-submission")
        .unwrap_or_else(|| panic!("{findings}"));
    assert_eq!(finding["scope"]["rpc_method"], "sendTransaction");
    assert_eq!(finding["scope"]["signature"], signature);
    assert!(finding["scope"].get("accepted_request_id").is_none());
    p.ok(&["down"]);
}
