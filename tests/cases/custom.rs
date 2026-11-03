use super::*;
fn custom_artifact(sequence: Value, methods: Value, key: &str) -> String {
    json!({"schema":"ronchi.case/v1","id":"custom.integration","revision":"1",
        "requires":{"primitives":["rpc.lookup-null@1","observe.rpc@1"],"finding_templates":[]},
        "modes":["simulator"],"filter":{"methods":methods,"transaction_origin":"accepted_under_this_activation"},
        "state":{"key":key,"scope":"activation","max_keys":4,"on_exhausted":"error"},"sequence":sequence}).to_string()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_lookup_sequence_starts_at_first_lookup_and_recovers_per_identity() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let yaml = custom_artifact(
        json!([
            {"id":"hide","using":"rpc.lookup-null@1","until":{"elapsed_ms":10000,"clock":"simulator"},"next":"observe"},
            {"id":"observe","using":"observe.rpc@1","until":{"matches":1},"next":"done"}
        ]),
        json!(["getSignatureStatuses", "getTransaction"]),
        "transaction_signature",
    );
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "custom".into(),
            custom_yaml: Some(yaml),
            ..Default::default()
        })
        .await
        .unwrap();
    let (signature, wire) = signed(&server.hash().await, 171, false);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    server.time.advance(Duration::from_secs(16)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    assert!(
        server
            .call("getSignatureStatuses", json!([[signature]]))
            .await["result"]["value"][0]
            .is_null()
    );
    server.time.advance(Duration::from_secs(10)).unwrap();
    assert_eq!(
        server
            .call("getSignatureStatuses", json!([[signature]]))
            .await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert!(server.call("getTransaction", json!([signature])).await["result"].is_object());
    assert!(server.rpc.case_status().policy.unwrap().custom.is_some());
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_case_key_preserves_mixed_signature_filters_and_empty_method_semantics() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let (a, wa) = signed(&server.hash().await, 172, false);
    let (b, wb) = signed(&server.hash().await, 173, false);
    server.call("sendTransaction", send(&wa)).await;
    server.call("sendTransaction", send(&wb)).await;
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&b, Phase::Finalized).await;
    let mut artifact: Value = serde_json::from_str(&custom_artifact(
        json!([
            {"id":"hide","using":"rpc.lookup-null@1","until":{"matches":1},"next":"observe"},
            {"id":"observe","using":"observe.rpc@1","until":{"matches":1},"next":"done"}
        ]),
        json!([]),
        "case",
    ))
    .unwrap();
    artifact["filter"]["transaction_origin"] = json!("known_local");
    artifact["filter"]["signatures"] = json!([a]);
    let binding = server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "custom".into(),
            custom_yaml: Some(artifact.to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    let values = server
        .call("getSignatureStatuses", json!([[a, b, a]]))
        .await;
    assert_eq!(values["result"]["value"][0], Value::Null);
    assert_eq!(values["result"]["value"][2], Value::Null);
    assert_eq!(
        values["result"]["value"][1]["confirmationStatus"],
        "finalized"
    );
    assert_eq!(
        server.call("getSignatureStatuses", json!([[a]])).await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    artifact["schema"] = json!("ronchi.case/v999");
    assert!(
        server
            .rpc
            .arm_case(ronchi::cases::Arm {
                case_id: "custom".into(),
                custom_yaml: Some(artifact.to_string()),
                ..Default::default()
            })
            .await
            .is_err()
    );
    assert_eq!(
        server.rpc.case_status().policy.unwrap().revision,
        binding.policy.unwrap().revision
    );
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_capacity_rejection_preserves_all_existing_counters() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let (a, wa) = signed(&server.hash().await, 174, false);
    let (b, wb) = signed(&server.hash().await, 175, false);
    server.call("sendTransaction", send(&wa)).await;
    server.call("sendTransaction", send(&wb)).await;
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&b, Phase::Finalized).await;
    let mut artifact: Value = serde_json::from_str(&custom_artifact(
        json!([
            {"id":"hide","using":"rpc.lookup-null@1","until":{"matches":2},"next":"observe"},
            {"id":"observe","using":"observe.rpc@1","until":{"matches":1},"next":"done"}
        ]),
        json!(["getSignatureStatuses"]),
        "transaction_signature",
    ))
    .unwrap();
    artifact["filter"]["transaction_origin"] = json!("known_local");
    artifact["state"]["max_keys"] = json!(1);
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "custom".into(),
            custom_yaml: Some(artifact.to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        server.call("getSignatureStatuses", json!([[a]])).await["result"]["value"][0].is_null()
    );
    assert_eq!(
        server.call("getSignatureStatuses", json!([[a, b]])).await["error"]["data"]["code"],
        "case_capacity_exceeded"
    );
    assert!(
        server.call("getSignatureStatuses", json!([[a]])).await["result"]["value"][0].is_null()
    );
    assert_eq!(
        server.call("getSignatureStatuses", json!([[a]])).await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_unsupported_lookup_does_not_consume_a_count_step() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let (signature, wire) = signed(&server.hash().await, 176, true);
    server.call("sendTransaction", send(&wire)).await;
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    let mut artifact: Value = serde_json::from_str(&custom_artifact(
        json!([
            {"id":"hide","using":"rpc.lookup-null@1","until":{"matches":1},"next":"observe"},
            {"id":"observe","using":"observe.rpc@1","until":{"matches":1},"next":"done"}
        ]),
        json!(["getTransaction"]),
        "transaction_signature",
    ))
    .unwrap();
    artifact["filter"]["transaction_origin"] = json!("known_local");
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "custom".into(),
            custom_yaml: Some(artifact.to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        server.call("getTransaction", json!([signature])).await["error"]["code"],
        -32015
    );
    assert!(
        server
            .call(
                "getTransaction",
                json!([signature,{"maxSupportedTransactionVersion":0}])
            )
            .await["result"]
            .is_null()
    );
    assert!(
        server
            .call(
                "getTransaction",
                json!([signature,{"maxSupportedTransactionVersion":0}])
            )
            .await["result"]
            .is_object()
    );
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_transaction_first_snapshot_keeps_complete_status_shape_and_commitment_gates() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let (signature, wire) = signed(&server.hash().await, 177, false);
    server.call("sendTransaction", send(&wire)).await;
    server.time.advance(Duration::from_secs(4)).unwrap();
    server.phase(&signature, Phase::Confirmed).await;
    let mut artifact:Value=serde_json::from_str(&custom_artifact(json!([
        {"id":"snapshot","using":"rpc.lookup-snapshot@1","until":{"elapsed_ms":10000,"clock":"simulator"},"next":"observe"},
        {"id":"observe","using":"observe.rpc@1","until":{"matches":1},"next":"done"}
    ]),json!(["getTransaction","getSignatureStatuses"]),"transaction_signature")).unwrap();
    artifact["requires"]["primitives"] = json!(["rpc.lookup-snapshot@1", "observe.rpc@1"]);
    artifact["filter"]["transaction_origin"] = json!("known_local");
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "custom".into(),
            custom_yaml: Some(artifact.to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        server
            .call(
                "getTransaction",
                json!([signature,{"commitment":"confirmed","encoding":"base64"}])
            )
            .await["result"]
            .is_object()
    );
    server.time.advance(Duration::from_secs(2)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    let status = server
        .call("getSignatureStatuses", json!([[signature]]))
        .await["result"]["value"][0]
        .clone();
    assert_eq!(status["confirmationStatus"], "confirmed");
    assert!(status.get("err").is_some());
    assert!(status.get("status").is_some());
    assert!(status["confirmations"].is_number());
    assert!(server.call("getTransaction", json!([signature])).await["result"].is_null());
    server.time.advance(Duration::from_secs(8)).unwrap();
    assert_eq!(
        server
            .call("getSignatureStatuses", json!([[signature]]))
            .await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_template_joins_all_lookup_entries_and_requires_scoped_source_and_boundary() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let mut artifact:Value=serde_json::from_str(&custom_artifact(json!([
        {"id":"hide","using":"rpc.lookup-null@1","until":{"elapsed_ms":10000,"clock":"simulator"},"next":"observe"},
        {"id":"observe","using":"observe.rpc@1","until":{"matches":1},"next":"done"}
    ]),json!(["getSignatureStatuses","getTransaction"]),"transaction_signature")).unwrap();
    artifact["state"]["max_keys"] = json!(16);
    artifact["requires"]["finding_templates"] = json!(["lookup-hidden-then-queried@1"]);
    artifact["findings"] = json!([{"id":"hidden-then-queried","template":"lookup-hidden-then-queried@1","facts":{"hidden":{"kind":"lookup_hidden","step":"hide"},"later":{"kind":"lookup_received","step":"observe"}},"same":"transaction_signature","order":["hidden","later"],"within_ms":60000}]);
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "custom".into(),
            custom_yaml: Some(artifact.to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    let hash = server.hash().await;
    let mut signatures = vec![];
    for salt in 180..188 {
        let (signature, wire) = signed(&hash, salt, false);
        server.call("sendTransaction", send(&wire)).await;
        signatures.push(signature);
    }
    server.time.advance(Duration::from_secs(6)).unwrap();
    server
        .phase(signatures.last().unwrap(), Phase::Finalized)
        .await;
    let hidden = server
        .call("getSignatureStatuses", json!([signatures]))
        .await;
    assert!(
        hidden["result"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .all(Value::is_null)
    );
    server.time.advance(Duration::from_secs(10)).unwrap();
    assert!(
        server
            .call("getSignatureStatuses", json!([signatures]))
            .await["result"]["value"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["confirmationStatus"] == "finalized")
    );
    assert!(server.stop(false).await);
    let engine = ronchi::query::Engine::default();
    let path = server.directory.path().join("journal.sqlite");
    let request = ronchi::query::Request {
        contract_version: 1,
        request_id: "custom-template".into(),
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
    let findings = found.result["items"].as_array().unwrap();
    assert_eq!(findings.len(), 8, "{}", found.result);
    assert!(
        findings
            .iter()
            .all(|f| f["rule_id"] == "custom.integration.hidden-then-queried")
    );
    for finding in findings {
        assert!(
            finding["supporting_events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["role"] == "later_lookup_received")
        );
    }
    let mut incomplete = request;
    incomplete.arguments.scope.kind = vec![
        "fault.custom_lookup_changed".into(),
        "rpc.response_prepared".into(),
        "rpc.boundary".into(),
        "rpc.registered".into(),
        "custom.selection".into(),
    ];
    let missing = engine
        .execute(path, true, "explicit", incomplete, CancellationToken::new())
        .await
        .unwrap();
    assert!(missing.result["items"].as_array().unwrap().is_empty());
    assert!(
        missing
            .view
            .evidence_gaps
            .iter()
            .any(|gap| gap.message.contains("Custom template")),
        "{}",
        serde_json::to_value(missing).unwrap()
    );
}
