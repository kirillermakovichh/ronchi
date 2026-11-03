use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn confirmation_schedules_progress_without_polling_and_survive_replacement() {
    for (case_id, confirmed_at) in [("tx-confirmation-stalls", 34), ("finality-delayed", 4)] {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        server.rpc.wait_initialized().await.unwrap();
        server
            .rpc
            .arm_case(ronchi::cases::Arm {
                case_id: case_id.into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let (signature, wire) = signed(&server.hash().await, 91, false);
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        let assigned = server.record(&signature).await.unwrap();
        assert_eq!(assigned.process_at_ns, 2_000_000_000);
        assert_eq!(assigned.confirmed_at_ns, confirmed_at * 1_000_000_000);
        assert_eq!(assigned.finalized_at_ns, 36_000_000_000);
        server.time.advance(Duration::from_secs(2)).unwrap();
        server.phase(&signature, Phase::Processed).await;
        server.rpc.disarm_case().await.unwrap();
        // A duplicate cannot replace the accepted schedule.
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        assert_eq!(
            server.record(&signature).await.unwrap().confirmed_at_ns,
            assigned.confirmed_at_ns
        );
        server
            .time
            .advance(Duration::from_secs(confirmed_at - 3))
            .unwrap();
        assert!(
            server
                .call(
                    "getTransaction",
                    json!([signature,{"commitment":"confirmed"}])
                )
                .await["result"]
                .is_null()
        );
        server.time.advance(Duration::from_secs(1)).unwrap();
        server.phase(&signature, Phase::Confirmed).await;
        assert!(
            server
                .call(
                    "getTransaction",
                    json!([signature,{"commitment":"confirmed"}])
                )
                .await["result"]
                .is_object()
        );
        assert!(
            server
                .call(
                    "getTransaction",
                    json!([signature,{"commitment":"finalized"}])
                )
                .await["result"]
                .is_null()
        );
        server
            .time
            .advance(Duration::from_secs(36 - confirmed_at))
            .unwrap();
        server.phase(&signature, Phase::Finalized).await;
        assert!(server.call("getTransaction", json!([signature])).await["result"].is_object());
        let query = ronchi::query::Request {
            contract_version: 1,
            request_id: "confirmation-finding".into(),
            lab_id: "L-transactions".into(),
            operation: ronchi::query::Operation::Findings,
            arguments: Default::default(),
        };
        let engine = ronchi::query::Engine::default();
        let path = server.directory.path().join("journal.sqlite");
        let result = engine
            .execute(
                path.clone(),
                false,
                "explicit",
                query.clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.result["items"].as_array().unwrap().len(), 1);
        let finding = &result.result["items"][0];
        assert_eq!(finding["rule_id"], "sim.confirmation-delay.observed");
        assert!(finding["statement"].as_str().unwrap().contains("32000 ms"));
        let mut detail = query.clone();
        detail.operation = ronchi::query::Operation::Evidence;
        detail.arguments.snapshot_seq = Some(result.view.snapshot_seq);
        detail.arguments.scope.finding_id = Some(finding["finding_id"].as_str().unwrap().into());
        let detail = engine
            .execute(
                path.clone(),
                false,
                "explicit",
                detail,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(detail.result["items"].as_array().unwrap().len() >= 4);
        let mut narrowed = query;
        narrowed.arguments.scope.kind = vec![
            "transaction.transition".into(),
            "transaction.accepted".into(),
        ];
        let result = engine
            .execute(path, false, "explicit", narrowed, CancellationToken::new())
            .await
            .unwrap();
        assert!(result.result["items"].as_array().unwrap().is_empty());
        assert!(server.stop(false).await);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn already_processed_transaction_does_not_expire_during_confirmation_delay() {
    let mut server = Server::new(
        ClockProfile {
            validity_window_blocks: 6,
            ..Default::default()
        },
        DiskLimits::default(),
    )
    .await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "tx-confirmation-stalls".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let (signature, wire) = signed(&server.hash().await, 92, false);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    server.time.advance(Duration::from_secs(2)).unwrap();
    server.phase(&signature, Phase::Processed).await;
    server.time.advance(Duration::from_secs(34)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    assert_eq!(
        server
            .call("getSignatureStatuses", statuses(&signature))
            .await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn confirmation_settings_are_bounded_and_nonmatching_submissions_keep_normal_schedule() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let delay = ronchi::cases::confirmation::Delay {
        extra_delay_ms: 120_001,
        ..Default::default()
    };
    let invalid = ronchi::cases::Arm {
        case_id: "tx-confirmation-stalls".into(),
        confirmation: Some(delay),
        ..Default::default()
    };
    assert_eq!(server.rpc.arm_case(invalid).await.unwrap_err().code, -32602);
    assert!(server.rpc.case_status().policy.is_none());
    let hash = server.hash().await;
    let (eligible, wire) = signed(&hash, 93, true);
    let (other, other_wire) = signed(&hash, 94, false);
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "tx-confirmation-stalls".into(),
            signatures: vec![eligible.clone()],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        server.call("sendTransaction", send(&other_wire)).await["result"],
        other
    );
    assert_eq!(
        server.record(&other).await.unwrap().finalized_at_ns,
        6_000_000_000
    );
    let (status, _) = server
        .http(json!({"jsonrpc":"2.0","method":"sendTransaction","params":send(&wire)}))
        .await
        .unwrap();
    assert_eq!(status, 204);
    assert_eq!(
        server.record(&eligible).await.unwrap().finalized_at_ns,
        6_000_000_000
    );
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&other, Phase::Finalized).await;
    server.phase(&eligible, Phase::Finalized).await;
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn batch_registration_freezes_policy_times_before_replacement_and_dispatch() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let original = server.rpc.arm_case(Default::default()).await.unwrap();
    let pause = server.rpc.pause_next_registration();
    let batch = json!([
        {"jsonrpc":"2.0","id":1,"method":"getSlot"},
        {"jsonrpc":"2.0","id":2,"method":"getBlockHeight"}
    ]);
    let (response, replacement) = tokio::join!(server.http(batch), async {
        pause.registered().await;
        server.time.advance(Duration::from_secs(7)).unwrap();
        let replacement = server
            .rpc
            .arm_case(ronchi::cases::Arm {
                case_id: "finality-delayed".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        pause.resume();
        replacement
    });
    assert_eq!(response.unwrap().0, 200);
    assert_eq!(
        replacement.policy.as_ref().unwrap().activated_simulated_ns,
        7_000_000_000
    );
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    let mut registrations = vec![];
    for seq in 1..=reader.snapshot(true).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "rpc.registered" {
            registrations.push(event.data);
        }
    }
    assert_eq!(registrations.len(), 2);
    assert_eq!(registrations[0]["registered_simulated_ns"], 0);
    assert_eq!(
        registrations[0]["registered_elapsed_ns"],
        registrations[1]["registered_elapsed_ns"]
    );
    for registration in &registrations {
        assert_eq!(
            registration["binding"]["policy"]["revision"],
            original.policy.as_ref().unwrap().revision
        );
        assert_eq!(
            registration["binding"]["policy"]["activated_simulated_ns"],
            0
        );
    }
    drop(reader);
    assert!(server.stop(false).await);
}
