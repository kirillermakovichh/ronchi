use super::*;
fn timeout_arm() -> ronchi::cases::Arm {
    serde_json::from_value(json!({"case_id":"provider-timeout-burst","hold_ms":100,"timeout":{"methods":["sendTransaction"]}})).unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeout_submission_never_admits_and_preserves_previously_accepted_identity() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let hash = server.hash().await;
    let (existing, wire) = signed(&hash, 111, false);
    let (new, other) = signed(&hash, 112, true);
    server.call("sendTransaction", send(&wire)).await;
    server.rpc.arm_case(timeout_arm()).await.unwrap();
    assert!(
        server
            .http(json!({"jsonrpc":"2.0","id":1,"method":"sendTransaction","params":send(&wire)}))
            .await
            .is_err()
    );
    assert!(
        server
            .http(json!({"jsonrpc":"2.0","id":2,"method":"sendTransaction","params":send(&other)}))
            .await
            .is_err()
    );
    assert!(server.record(&new).await.is_none());
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&existing, Phase::Finalized).await;
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    let events: Vec<_> = (1..=reader.snapshot(false).unwrap().snapshot_seq)
        .map(|s| reader.event(s).unwrap().unwrap())
        .collect();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == "transaction.submission")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == "transaction.accepted")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == "rpc.dispatch_prevented")
            .count(),
        2
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeout_never_masks_missing_alt_nonce_or_preflight_capabilities() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let hash = server.hash().await;
    server.rpc.arm_case(timeout_arm()).await.unwrap();
    let table = bs58::encode([117u8; 32]).into_string();
    let nonce = bs58::encode([118u8; 32]).into_string();
    let (_, lookup) = signed_lookup(&hash, 113, &[(&table, vec![0], vec![0])]);
    let (_, nonce) = signed_nonce(&hash, 114, &nonce, 0, 1);
    for wire in [lookup, nonce] {
        let result = server.call("sendTransaction", send(&wire)).await;
        assert_eq!(
            result["error"]["data"]["code"], "state_unavailable",
            "{result}"
        );
    }
    let (_, wire) = signed(&hash, 115, false);
    let mut params = send(&wire);
    params[1]["skipPreflight"] = json!(false);
    assert_eq!(
        server.call("sendTransaction", params).await["error"]["data"]["code"],
        "unsupported_capability"
    );
    assert!(server.rpc.held_effects().is_empty());
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert!(
        !(1..=reader.snapshot(false).unwrap().snapshot_seq).any(|seq| reader
            .event(seq)
            .unwrap()
            .unwrap()
            .kind
            == "rpc.dispatch_prevented")
    );
}
