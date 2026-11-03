use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_null_preserves_authority_order_and_per_identity_recovery() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let (before, wire) = signed(&server.hash().await, 117, false);
    server.call("sendTransaction", send(&wire)).await;
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "signature-status-null".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let (selected, wire) = signed(&server.hash().await, 118, false);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        selected
    );
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&selected, Phase::Finalized).await;
    let unknown = bs58::encode([119u8; 64]).into_string();
    let statuses = server
        .call(
            "getSignatureStatuses",
            json!([[selected, before, unknown, selected]]),
        )
        .await;
    assert!(statuses["result"]["value"][0].is_null());
    assert_eq!(
        statuses["result"]["value"][1]["confirmationStatus"],
        "finalized"
    );
    assert!(statuses["result"]["value"][2].is_null());
    assert!(statuses["result"]["value"][3].is_null());
    assert_eq!(
        server.record(&selected).await.unwrap().stage,
        Phase::Finalized
    );
    assert!(server.call("getTransaction", json!([selected])).await["result"].is_null());
    server.call("sendTransaction", send(&wire)).await;
    server.time.advance(Duration::from_secs(4)).unwrap();
    assert_eq!(
        server
            .call("getSignatureStatuses", json!([[selected]]))
            .await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert!(server.call("getTransaction", json!([selected])).await["result"].is_object());
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_stale_uses_first_committed_transition_and_recovers_without_stopping_authority() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "stale-transaction-read-after-submission".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let (signature, wire) = signed(&server.hash().await, 120, true);
    server.call("sendTransaction", send(&wire)).await;
    let unknown = bs58::encode([121; 64]).into_string();
    assert!(
        server
            .call("getSignatureStatuses", json!([[signature, unknown]]))
            .await["result"]["value"][0]
            .is_null()
    );
    server.time.advance(Duration::from_secs(2)).unwrap();
    server.phase(&signature, Phase::Processed).await;
    let captured = server
        .call("getSignatureStatuses", json!([[signature]]))
        .await["result"]["value"][0]
        .clone();
    assert_eq!(captured["confirmationStatus"], "processed");
    server.time.advance(Duration::from_secs(4)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    let response = server
        .call(
            "getSignatureStatuses",
            json!([[signature, unknown, signature]]),
        )
        .await;
    assert_eq!(
        response["result"]["value"],
        json!([captured, null, captured])
    );
    assert_eq!(response["result"]["context"]["slot"], 15);
    assert!(server.call("getTransaction",json!([signature,{"commitment":"confirmed","maxSupportedTransactionVersion":0,"encoding":"jsonParsed"}])).await["result"].is_null());
    assert_eq!(
        server
            .call(
                "getTransaction",
                json!([signature,{"commitment":"processed"}])
            )
            .await["error"]["code"],
        -32602
    );
    assert_eq!(
        server.call("getTransaction", json!([signature])).await["error"]["code"],
        -32015
    );
    server.time.advance(Duration::from_secs(6)).unwrap();
    assert_eq!(
        server
            .call("getSignatureStatuses", json!([[signature]]))
            .await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert!(
        server
            .call(
                "getTransaction",
                json!([signature,{"maxSupportedTransactionVersion":0,"encoding":"base64"}])
            )
            .await["result"]
            .is_object()
    );
    assert!(server.stop(false).await);
    let path = server.directory.path().join("journal.sqlite");
    let engine = ronchi::query::Engine::default();
    let request = ronchi::query::Request {
        contract_version: 1,
        request_id: "stale-finding".into(),
        lab_id: "L-transactions".into(),
        operation: ronchi::query::Operation::Findings,
        arguments: Default::default(),
    };
    let found = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            request.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        found.result["items"].as_array().unwrap().len(),
        3,
        "{}",
        found.result
    );
    assert!(
        found.result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|finding| finding["rule_id"] == "rpc.lookup-view.changed")
    );
    let mut narrowed = request;
    narrowed.arguments.scope.kind = vec![
        "transaction.accepted".into(),
        "rpc.lookup_source".into(),
        "fault.lookup_changed".into(),
        "rpc.response_prepared".into(),
        "rpc.boundary".into(),
    ];
    let found = engine
        .execute(path, true, "explicit", narrowed, CancellationToken::new())
        .await
        .unwrap();
    assert!(
        found.result["items"].as_array().unwrap().is_empty(),
        "captured transition must remain inside the selected scope"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registered_status_reads_keep_activation_and_new_activations_do_not_reselect_old_identities()
 {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "signature-status-null".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let (signature, wire) = signed(&server.hash().await, 122, false);
    server.call("sendTransaction", send(&wire)).await;
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    let pause = server.rpc.pause_next_registration();
    let address = server.address;
    let selected = tokio::spawn(async move {
        let client: Client<HttpConnector, Full<Bytes>> =
            Client::builder(TokioExecutor::new()).build_http();
        let request=hyper::Request::builder().method("POST").uri(format!("http://{address}/")).body(Full::new(Bytes::from(json!({"jsonrpc":"2.0","id":1,"method":"getSignatureStatuses","params":[[signature]]}).to_string()))).unwrap();
        let response = client.request(request).await.unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice::<Value>(&body).unwrap()
    });
    pause.registered().await;
    server.rpc.disarm_case().await.unwrap();
    pause.resume();
    assert!(selected.await.unwrap()["result"]["value"][0].is_null());
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "signature-status-null".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    // Recover the original identity from its submitted bytes, independently of Case state.
    let signature = ronchi::protocol::decode(&STANDARD.encode(&wire), "base64")
        .unwrap()
        .signature;
    server.call("sendTransaction", send(&wire)).await;
    assert_eq!(
        server
            .call("getSignatureStatuses", json!([[signature]]))
            .await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_count_and_byte_bounds_reject_before_new_acceptance_and_preserve_duplicates() {
    for settings in [
        ronchi::cases::status::Settings {
            max_keys: 1,
            max_snapshot_bytes: 4 * 1024 * 1024,
        },
        ronchi::cases::status::Settings {
            max_keys: 1024,
            max_snapshot_bytes: 1024,
        },
    ] {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        server.rpc.wait_initialized().await.unwrap();
        server
            .rpc
            .arm_case(ronchi::cases::Arm {
                case_id: "signature-status-null".into(),
                status: Some(settings),
                ..Default::default()
            })
            .await
            .unwrap();
        let hash = server.hash().await;
        let (a, wire_a) = signed(&hash, 123, false);
        let (b, wire_b) = signed(&hash, 124, false);
        assert_eq!(
            server.call("sendTransaction", send(&wire_a)).await["result"],
            a
        );
        assert_eq!(
            server.call("sendTransaction", send(&wire_a)).await["result"],
            a
        );
        let rejected = server.call("sendTransaction", send(&wire_b)).await;
        assert_eq!(rejected["error"]["data"]["code"], "case_capacity_exceeded");
        assert!(server.record(&b).await.is_none());
        server.time.advance(Duration::from_secs(10)).unwrap();
        server.phase(&a, Phase::Finalized).await;
        assert!(server.stop(false).await);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_findings_require_scoped_source_and_actual_prepared_boundary_evidence() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "signature-status-null".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let (signature, wire) = signed(&server.hash().await, 125, false);
    server.call("sendTransaction", send(&wire)).await;
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    assert!(server.call("getTransaction", json!([signature])).await["result"].is_null());
    assert!(server.stop(false).await);
    let path = server.directory.path().join("journal.sqlite");
    let engine = ronchi::query::Engine::default();
    let request = ronchi::query::Request {
        contract_version: 1,
        request_id: "lookup-finding".into(),
        lab_id: "L-transactions".into(),
        operation: ronchi::query::Operation::Findings,
        arguments: Default::default(),
    };
    let result = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            request.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let findings = result.result["items"].as_array().unwrap();
    assert_eq!(findings.len(), 1, "{}", result.result);
    let finding = &findings[0];
    assert_eq!(finding["rule_id"], "rpc.lookup-view.changed");
    assert!(finding["statement"].as_str().unwrap().contains("null"));
    let mut evidence = request.clone();
    evidence.operation = ronchi::query::Operation::Evidence;
    evidence.arguments.snapshot_seq = Some(result.view.snapshot_seq);
    evidence.arguments.scope.finding_id = Some(finding["finding_id"].as_str().unwrap().into());
    let detail = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            evidence,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(detail.result["items"].as_array().unwrap().len() >= 5);
    let mut narrowed = request;
    narrowed.arguments.scope.kind = vec![
        "fault.lookup_changed".into(),
        "transaction.accepted".into(),
        "rpc.response_prepared".into(),
        "rpc.boundary".into(),
    ];
    let result = engine
        .execute(path, true, "explicit", narrowed, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.result["items"].as_array().unwrap().len(), 0);
    assert!(!result.view.evidence_gaps.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_filters_and_notifications_do_not_invent_boundary_faults() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let hash = server.hash().await;
    let (a, wire_a) = signed(&hash, 126, false);
    let (b, wire_b) = signed(&hash, 127, false);
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "signature-status-null".into(),
            signatures: vec![a.clone()],
            status: Some(ronchi::cases::status::Settings {
                max_keys: 1,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let (status, body) = server
        .http(json!({"jsonrpc":"2.0","method":"sendTransaction","params":send(&wire_a)}))
        .await
        .unwrap();
    assert_eq!(status, 204);
    assert!(body.is_empty());
    assert_eq!(
        server.call("sendTransaction", send(&wire_b)).await["result"],
        b
    );
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&a, Phase::Finalized).await;
    server.phase(&b, Phase::Finalized).await;
    let (status, body) = server
        .http(json!({"jsonrpc":"2.0","method":"getSignatureStatuses","params":[[a,b]]}))
        .await
        .unwrap();
    assert_eq!(status, 204);
    assert!(body.is_empty());
    let result = server.call("getSignatureStatuses", json!([[a, b]])).await;
    assert!(result["result"]["value"][0].is_null());
    assert_eq!(
        result["result"]["value"][1]["confirmationStatus"],
        "finalized"
    );
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    let mut faults = 0;
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "fault.lookup_changed" {
            faults += 1;
            assert_eq!(event.data["request"]["responds"], true);
        }
    }
    assert_eq!(faults, 1);
}
