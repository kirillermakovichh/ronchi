use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn already_expired_admission_retains_identity_without_a_success_schedule() {
    let mut server = Server::new(
        ClockProfile {
            validity_window_blocks: 1,
            ..Default::default()
        },
        DiskLimits::default(),
    )
    .await;
    let hash = server.hash().await;
    server.time.advance(Duration::from_secs(10)).unwrap();
    let (signature, wire) = signed(&hash, 83, false);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    let record = server.record(&signature).await.unwrap();
    assert_eq!(record.stage, Phase::Expired);
    assert!(record.next_due_ns.is_none());
    assert_eq!(record.process_at_ns, 0);
    assert_eq!(record.confirmed_at_ns, 0);
    assert_eq!(record.finalized_at_ns, 0);
    assert!(
        server
            .call("getSignatureStatuses", statuses(&signature))
            .await["result"]["value"][0]
            .is_null()
    );
    assert!(server.call("getTransaction", json!([signature])).await["result"].is_null());
    assert!(server.stop(false).await);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn genuine_blockhash_tuple_crosses_height_bound_and_submission_starts_recovery() {
    let mut server = Server::new(
        ClockProfile {
            validity_window_blocks: 2,
            ..Default::default()
        },
        DiskLimits::default(),
    )
    .await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "tx-blockhash-expired".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let captured = server.call("getLatestBlockhash", json!([])).await["result"].clone();
    assert_eq!(captured["value"]["lastValidBlockHeight"], 2);
    server.time.advance(Duration::from_secs(5)).unwrap();
    assert_eq!(server.call("getBlockHeight", json!([])).await["result"], 2);
    assert_eq!(
        server.call("getLatestBlockhash", json!([])).await["result"],
        captured
    );
    server.time.advance(Duration::from_millis(400)).unwrap();
    assert_eq!(server.call("getBlockHeight", json!([])).await["result"], 3);
    assert_eq!(
        server.call("getLatestBlockhash", json!([])).await["result"],
        captured
    );
    let (signature, wire) = signed(captured["value"]["blockhash"].as_str().unwrap(), 84, true);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    assert_eq!(
        server.record(&signature).await.unwrap().stage,
        Phase::Expired
    );
    assert_ne!(
        server.call("getLatestBlockhash", json!([])).await["result"],
        captured
    );
    assert!(server.stop(false).await);
    let path = server.directory.path().join("journal.sqlite");
    let query = ronchi::query::Request {
        contract_version: 1,
        request_id: "expired-history".into(),
        lab_id: "L-transactions".into(),
        operation: ronchi::query::Operation::Findings,
        arguments: Default::default(),
    };
    let findings = ronchi::query::Engine::default()
        .execute(path, false, "explicit", query, CancellationToken::new())
        .await
        .unwrap();
    assert!(
        findings.result["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["rule_id"] == "recent-blockhash.expired-submission"),
        "{}",
        findings.result
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn context_commitment_and_registered_policy_preserve_source_constraints() {
    let mut server = Server::new(
        ClockProfile {
            validity_window_blocks: 2,
            ..Default::default()
        },
        DiskLimits::default(),
    )
    .await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "tx-blockhash-expired".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let cached = server.call("getLatestBlockhash", json!([])).await["result"].clone();
    server.time.advance(Duration::from_secs(5)).unwrap();
    assert_ne!(
        server
            .call("getLatestBlockhash", json!([{"commitment":"processed"}]))
            .await["result"],
        cached
    );
    assert_ne!(
        server
            .call("getLatestBlockhash", json!([{"minContextSlot":1}]))
            .await["result"],
        cached
    );
    assert_eq!(
        server.call("getLatestBlockhash", json!([])).await["result"],
        cached
    );
    {
        let pause = server.rpc.pause_next_registration();
        let read = server.call("getLatestBlockhash", json!([]));
        tokio::pin!(read);
        tokio::select! {_=pause.registered()=>{},result=&mut read=>panic!("registration did not pause: {result}")};
        server.rpc.disarm_case().await.unwrap();
        pause.resume();
        assert_eq!(read.await["result"], cached);
    }
    assert_ne!(
        server.call("getLatestBlockhash", json!([])).await["result"],
        cached
    );
    assert!(server.stop(false).await);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn processed_duplicates_and_durable_nonce_never_release_recent_hash_cache() {
    let profile = ClockProfile {
        validity_window_blocks: 6,
        ..Default::default()
    };
    let mut server = Server::new(profile, DiskLimits::default()).await;
    let hash = server.hash().await;
    let (signature, wire) = signed(&hash, 85, false);
    server.call("sendTransaction", send(&wire)).await;
    server.time.advance(Duration::from_secs(2)).unwrap();
    server.phase(&signature, Phase::Processed).await;
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "tx-blockhash-expired".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let cached = server.call("getLatestBlockhash", json!([])).await["result"].clone();
    server.time.advance(Duration::from_millis(4800)).unwrap();
    assert_eq!(
        server.call("getLatestBlockhash", json!([])).await["result"],
        cached
    );
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    assert_eq!(
        server.record(&signature).await.unwrap().stage,
        Phase::Finalized
    );
    assert_eq!(
        server.call("getLatestBlockhash", json!([])).await["result"],
        cached
    );
    assert!(server.stop(false).await);
    for version in [-1, 0, 1] {
        let (clock, _) = Clock::controlled(profile, b"transaction-test-network").unwrap();
        let nonce = clock.blockhash(0);
        let account = bs58::encode([71u8; 32]).into_string();
        let authority = bs58::encode(SigningKey::from_bytes(&[86; 32]).verifying_key().to_bytes())
            .into_string();
        let state = ronchi::protocol::state::ProtocolState {
            nonce_accounts: vec![ronchi::protocol::nonce::NonceAccount {
                address: account.clone(),
                authority,
                nonce: nonce.clone(),
                slot: 0,
            }],
            ..Default::default()
        };
        let mut server = Server::with_state(profile, DiskLimits::default(), state).await;
        server.rpc.wait_initialized().await.unwrap();
        server
            .rpc
            .arm_case(ronchi::cases::Arm {
                case_id: "tx-blockhash-expired".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let cached = server.call("getLatestBlockhash", json!([])).await["result"].clone();
        server.time.advance(Duration::from_millis(6800)).unwrap();
        assert_eq!(
            server.call("getLatestBlockhash", json!([])).await["result"],
            cached
        );
        let (signature, wire) = signed_nonce(&nonce, 86, &account, version, 0);
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        assert_eq!(
            server.call("getLatestBlockhash", json!([])).await["result"],
            cached
        );
        server.time.advance(Duration::from_secs(2)).unwrap();
        server.phase(&signature, Phase::Processed).await;
        server.time.advance(Duration::from_secs(4)).unwrap();
        server.phase(&signature, Phase::Finalized).await;
        assert!(server.stop(false).await);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_view_without_submission_and_missing_support_cannot_create_incident() {
    let mut server = Server::new(
        ClockProfile {
            validity_window_blocks: 2,
            ..Default::default()
        },
        DiskLimits::default(),
    )
    .await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "tx-blockhash-expired".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let cached = server.call("getLatestBlockhash", json!([])).await["result"].clone();
    server.time.advance(Duration::from_millis(5400)).unwrap();
    server.call("getLatestBlockhash", json!([])).await;
    let path = server.directory.path().join("journal.sqlite");
    let request = || ronchi::query::Request {
        contract_version: 1,
        request_id: "no-expired-submission".into(),
        lab_id: "L-transactions".into(),
        operation: ronchi::query::Operation::Findings,
        arguments: Default::default(),
    };
    let engine = ronchi::query::Engine::default();
    let before = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            request(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(
        !before.result["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["rule_id"] == "recent-blockhash.expired-submission")
    );
    assert!(before.view.evidence_gaps.iter().any(|gap| {
        gap.message
            .contains("no matching recent-blockhash submission")
    }));
    let (signature, wire) = signed(cached["value"]["blockhash"].as_str().unwrap(), 87, false);
    let preflight = server
        .call(
            "sendTransaction",
            json!([STANDARD.encode(&wire),{"encoding":"base64"}]),
        )
        .await;
    assert_eq!(preflight["error"]["data"]["code"], "unsupported_capability");
    assert_eq!(
        server.call("getLatestBlockhash", json!([])).await["result"],
        cached
    );
    server.call("sendTransaction", send(&wire)).await;
    assert_eq!(
        server.record(&signature).await.unwrap().stage,
        Phase::Expired
    );
    assert!(server.stop(false).await);
    let full = engine
        .execute(
            path.clone(),
            false,
            "explicit",
            request(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let finding = full.result["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["rule_id"] == "recent-blockhash.expired-submission")
        .unwrap();
    let mut evidence = request();
    evidence.operation = ronchi::query::Operation::Evidence;
    evidence.arguments.scope.finding_id = finding["finding_id"].as_str().map(String::from);
    evidence.arguments.snapshot_seq = Some(full.view.snapshot_seq);
    let resolved = engine
        .execute(
            path.clone(),
            false,
            "explicit",
            evidence,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(resolved.result["items"].as_array().unwrap().len() >= 8);
    let reader = JournalReader::open(&path).unwrap();
    let mut kinds = std::collections::BTreeSet::new();
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind != "rpc.blockhash_height" {
            kinds.insert(event.kind);
        }
    }
    let mut narrowed = request();
    narrowed.arguments.scope.kind = kinds.into_iter().collect();
    let missing = engine
        .execute(path, false, "explicit", narrowed, CancellationToken::new())
        .await
        .unwrap();
    assert!(
        !missing.result["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["rule_id"] == "recent-blockhash.expired-submission")
    );
    assert!(
        missing
            .view
            .evidence_gaps
            .iter()
            .any(|gap| gap.message.contains("same-view source height"))
    );
}
