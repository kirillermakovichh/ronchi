mod custom_lifecycle_cases {
    use super::*;

    fn artifact(primitives: &[&str], methods: &[&str], key: &str, sequence: Value) -> Value {
        json!({"schema":"ronchi.case/v1","id":"custom.lifecycle","revision":"1",
            "requires":{"primitives":primitives,"finding_templates":[]},"modes":["simulator"],
            "filter":{"methods":methods,"transaction_origin":"known_local"},
            "state":{"key":key,"scope":"activation","max_keys":16,"on_exhausted":"error"},
            "sequence":sequence})
    }
    async fn arm(server: &Server, artifact: &Value) -> ronchi::cases::Binding {
        server
            .rpc
            .arm_case(ronchi::cases::Arm {
                case_id: "custom".into(),
                custom_yaml: Some(artifact.to_string()),
                ..Default::default()
            })
            .await
            .unwrap()
    }
    async fn events(server: &Server) -> Vec<ronchi::journal::Event> {
        let path = server.directory.path().join("journal.sqlite");
        tokio::task::spawn_blocking(move || {
            let reader = JournalReader::open(&path).unwrap();
            (1..=reader.snapshot(false).unwrap().snapshot_seq)
                .map(|seq| reader.event(seq).unwrap().unwrap())
                .collect()
        })
        .await
        .unwrap()
    }
    fn rpc(id: u64, method: &str, params: Value) -> Value {
        json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
    }

    async fn bulk_statuses(server: &Server, signatures: &[String]) -> Value {
        let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
            .pool_max_idle_per_host(0)
            .build_http();
        let request = hyper::Request::builder()
            .method("POST")
            .uri(format!("http://{}/", server.address))
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(
                rpc(256, "getSignatureStatuses", json!([signatures])).to_string(),
            )))
            .unwrap();
        let started = Instant::now();
        // Up to 256 per-entry source/effect facts require durable commits in this fixture.
        let response = tokio::time::timeout(Duration::from_secs(30), client.request(request))
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "bulk lookup timeout {error}; writer={:?}; actor_finished={}",
                    server.writer.as_ref().unwrap().handle().status(),
                    server.task.as_ref().unwrap().is_finished()
                )
            })
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = tokio::time::timeout(Duration::from_secs(5), response.into_body().collect())
            .await
            .unwrap()
            .unwrap()
            .to_bytes();
        assert!(bytes.len() <= ronchi::rpc::MAX_RESPONSE_BYTES);
        eprintln!("bulk status lookup completed after {:?}", started.elapsed());
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn custom_response_loss_ledger_survives_disarm_and_reactivation() {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        server.rpc.wait_initialized().await.unwrap();
        let artifact = artifact(
            &["rpc.withhold-submission-response@1"],
            &["sendTransaction"],
            "transaction_signature",
            json!([
                {"id":"lose","using":"rpc.withhold-submission-response@1","with":{"hold_real_ms":40},"until":{"matches":1},"next":"done"}
            ]),
        );
        let initial = arm(&server, &artifact).await;
        let hash = server.hash().await;
        let (a, wire_a) = signed(&hash, 181, false);
        assert!(
            server
                .http(rpc(1, "sendTransaction", send(&wire_a)))
                .await
                .is_err()
        );
        assert_eq!(
            server.record(&a).await.unwrap().wire_base64,
            STANDARD.encode(&wire_a)
        );
        server.rpc.disarm_case().await.unwrap();
        assert_eq!(
            server.call("sendTransaction", send(&wire_a)).await["result"],
            a
        );
        let replacement = arm(&server, &artifact).await;
        assert_ne!(
            initial.policy.unwrap().revision,
            replacement.policy.unwrap().revision
        );
        assert_eq!(
            server.call("sendTransaction", send(&wire_a)).await["result"],
            a
        );
        let (b, wire_b) = signed(&hash, 182, false);
        assert!(
            server
                .http(rpc(2, "sendTransaction", send(&wire_b)))
                .await
                .is_err()
        );
        assert_eq!(
            server.call("sendTransaction", send(&wire_a)).await["result"],
            a
        );
        server.time.advance(Duration::from_secs(6)).unwrap();
        server.phase(&a, Phase::Finalized).await;
        server.phase(&b, Phase::Finalized).await;
        assert!(server.stop(false).await);
        let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
        assert!(reader.response_loss(&a).unwrap().is_some());
        assert!(reader.response_loss(&b).unwrap().is_some());
        let evidence = events(&server).await;
        for signature in [&a, &b] {
            let affected: Vec<_> = evidence
                .iter()
                .filter(|event| {
                    event.kind == "fault.response_loss_selected"
                        && event.data["signature"] == *signature
                })
                .collect();
            assert_eq!(affected.len(), 1);
            assert_eq!(affected[0].data["policy"]["custom"]["origin"], "custom");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn custom_whole_batch_keeps_registered_selection_after_replacement() {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        server.rpc.wait_initialized().await.unwrap();
        let hash = server.hash().await;
        let (a, wire_a) = signed(&hash, 183, false);
        let (b, wire_b) = signed(&hash, 184, false);
        assert_eq!(
            server.call("sendTransaction", send(&wire_a)).await["result"],
            a
        );
        assert_eq!(
            server.call("sendTransaction", send(&wire_b)).await["result"],
            b
        );
        server.time.advance(Duration::from_secs(6)).unwrap();
        server.phase(&b, Phase::Finalized).await;
        let initial = arm(
            &server,
            &artifact(
                &["rpc.lookup-null@1"],
                &["getSignatureStatuses", "getTransaction"],
                "case",
                json!([
                    {"id":"hide","using":"rpc.lookup-null@1","until":{"matches":2},"next":"done"}
                ]),
            ),
        )
        .await;
        let pause = server.rpc.pause_next_registration();
        let batch = json!([
            rpc(1, "getSignatureStatuses", json!([[a]])),
            rpc(2, "getTransaction", json!([b]))
        ]);
        let (response, replacement) = tokio::join!(server.http(batch), async {
            tokio::time::timeout(Duration::from_secs(3), pause.registered())
                .await
                .unwrap();
            server.time.advance(Duration::from_secs(7)).unwrap();
            let replacement = arm(&server, &artifact(&["observe.rpc@1"],
                &["getSignatureStatuses", "getTransaction"], "case", json!([
                    {"id":"observe","using":"observe.rpc@1","until":{"matches":10},"next":"done"}
                ]))).await;
            pause.resume();
            replacement
        });
        let (status, bytes) = response.unwrap();
        assert_eq!(status, 200);
        let responses: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(responses[0]["id"], 1);
        assert!(responses[0]["result"]["value"][0].is_null());
        assert_eq!(responses[1]["id"], 2);
        assert!(responses[1]["result"].is_null());
        assert!(server.call("getTransaction", json!([b])).await["result"].is_object());
        let old_revision = initial.policy.unwrap().revision;
        assert!(replacement.policy.unwrap().revision > old_revision);
        assert!(server.stop(false).await);
        let evidence = events(&server).await;
        let registrations: Vec<_> = evidence
            .iter()
            .filter(|event| event.kind == "rpc.registered" && event.data["batch_id"].is_u64())
            .collect();
        assert_eq!(registrations.len(), 2);
        for registered in &registrations {
            assert_eq!(
                registered.data["binding"]["policy"]["revision"],
                old_revision
            );
            assert_eq!(registered.data["registered_simulated_ns"], 6_000_000_000u64);
            assert_eq!(
                registered.data["registration_seq"],
                registrations[0].data["registration_seq"]
            );
            assert_eq!(
                registered.data["registered_elapsed_ns"],
                registrations[0].data["registered_elapsed_ns"]
            );
        }
        let selected: Vec<_> = evidence
            .iter()
            .filter(|event| {
                event.kind == "custom.selection" && event.data["request"]["batch_id"].is_u64()
            })
            .collect();
        assert_eq!(selected.len(), 2);
        assert!(
            selected
                .iter()
                .all(|event| event.data["selection"]["step"] == "hide")
        );
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn custom_confirmation_deadlines_are_typed_and_survive_replacement() {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        server.rpc.wait_initialized().await.unwrap();
        let mut original_artifact = artifact(
            &["sim.confirmation-delay@1"],
            &["sendTransaction"],
            "transaction_signature",
            json!([
                {"id":"delay","using":"sim.confirmation-delay@1",
                 "with":{"processing_ms":500,"confirmation_interval_ms":700,"extra_delay_ms":1500,"hold_at":"confirmed"},
                 "until":{"matches":1},"next":"done"}
            ]),
        );
        let original = arm(&server, &original_artifact).await;
        let hash = server.hash().await;
        let (a, wire_a) = signed(&hash, 185, false);
        assert_eq!(
            server.call("sendTransaction", send(&wire_a)).await["result"],
            a
        );
        let assigned = server.record(&a).await.unwrap();
        assert_eq!(assigned.process_at_ns, 500_000_000);
        assert_eq!(assigned.confirmed_at_ns, 1_200_000_000);
        assert_eq!(assigned.finalized_at_ns, 3_400_000_000);
        server.rpc.disarm_case().await.unwrap();
        original_artifact["revision"] = json!("2");
        original_artifact["sequence"][0]["with"] = json!({
            "processing_ms":100,"confirmation_interval_ms":200,"extra_delay_ms":300,"hold_at":"processed"
        });
        let replacement = arm(&server, &original_artifact).await;
        assert_eq!(
            server.call("sendTransaction", send(&wire_a)).await["result"],
            a
        );
        let (b, wire_b) = signed(&hash, 186, false);
        assert_eq!(
            server.call("sendTransaction", send(&wire_b)).await["result"],
            b
        );
        let other = server.record(&b).await.unwrap();
        assert_eq!(
            (
                other.process_at_ns,
                other.confirmed_at_ns,
                other.finalized_at_ns
            ),
            (100_000_000, 600_000_000, 800_000_000)
        );
        let unchanged = server.record(&a).await.unwrap();
        assert_eq!(
            (
                unchanged.process_at_ns,
                unchanged.confirmed_at_ns,
                unchanged.finalized_at_ns
            ),
            (
                assigned.process_at_ns,
                assigned.confirmed_at_ns,
                assigned.finalized_at_ns
            )
        );
        server.time.advance(Duration::from_millis(499)).unwrap();
        assert!(
            server.call("getSignatureStatuses", json!([[a]])).await["result"]["value"][0].is_null()
        );
        server.time.advance(Duration::from_millis(1)).unwrap();
        server.phase(&a, Phase::Processed).await;
        server.time.advance(Duration::from_millis(699)).unwrap();
        assert!(
            server
                .call("getTransaction", json!([a,{"commitment":"confirmed"}]))
                .await["result"]
                .is_null()
        );
        server.time.advance(Duration::from_millis(1)).unwrap();
        server.phase(&a, Phase::Confirmed).await;
        assert!(
            server
                .call("getTransaction", json!([a,{"commitment":"confirmed"}]))
                .await["result"]
                .is_object()
        );
        server.time.advance(Duration::from_millis(2199)).unwrap();
        assert!(server.call("getTransaction", json!([a])).await["result"].is_null());
        server.time.advance(Duration::from_millis(1)).unwrap();
        server.phase(&a, Phase::Finalized).await;
        server.phase(&b, Phase::Finalized).await;
        assert!(server.call("getTransaction", json!([a])).await["result"].is_object());
        assert!(server.stop(false).await);
        let evidence = events(&server).await;
        let assignments: Vec<_> = evidence
            .iter()
            .filter(|event| event.kind == "fault.confirmation_schedule_assigned")
            .collect();
        assert_eq!(assignments.len(), 2);
        for (signature, revision) in [
            (&a, original.policy.unwrap().revision),
            (&b, replacement.policy.unwrap().revision),
        ] {
            let matching: Vec<_> = assignments
                .iter()
                .filter(|event| event.data["signature"] == *signature)
                .collect();
            assert_eq!(matching.len(), 1);
            assert_eq!(matching[0].data["policy"]["revision"], revision);
            assert_eq!(matching[0].data["policy"]["custom"]["origin"], "custom");
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn custom_timeout_notifications_do_not_consume_shared_batch_selection() {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        server.rpc.wait_initialized().await.unwrap();
        arm(&server, &artifact(&["rpc.timeout-before-dispatch@1", "observe.rpc@1"],
            &["getSlot", "getBlockHeight"], "case", json!([
                {"id":"outage","using":"rpc.timeout-before-dispatch@1","with":{"hold_real_ms":40},"until":{"matches":1},"next":"observe"},
                {"id":"observe","using":"observe.rpc@1","until":{"matches":1},"next":"done"}
            ]))).await;
        let (status, body) = server
            .http(json!({"jsonrpc":"2.0","method":"getSlot"}))
            .await
            .unwrap();
        assert_eq!(status, 204);
        assert!(body.is_empty());
        assert!(server.rpc.held_effects().is_empty());
        assert!(
            server
                .http(json!([
                    rpc(1, "getSlot", json!([])),
                    rpc(2, "getBlockHeight", json!([]))
                ]))
                .await
                .is_err()
        );
        assert!(server.call("getSlot", json!([])).await["result"].is_u64());
        assert!(server.stop(false).await);
        let evidence = events(&server).await;
        let selections: Vec<_> = evidence
            .iter()
            .filter(|event| event.kind == "custom.selection")
            .collect();
        assert_eq!(selections.len(), 2);
        assert_eq!(selections[0].data["selection"]["step"], "outage");
        assert_eq!(selections[1].data["selection"]["step"], "observe");
        assert!(
            selections
                .iter()
                .all(|event| event.data["request"]["responds"] == true)
        );
        assert_eq!(
            evidence
                .iter()
                .filter(|event| event.kind == "rpc.dispatch_prevented")
                .count(),
            1
        );
        let prepared = evidence
            .iter()
            .find(|event| {
                event.kind == "rpc.response_prepared"
                    && event.data["request_id"] == selections[1].data["request"]["request_id"]
            })
            .unwrap();
        assert!(prepared.data["result"].is_u64());
        let boundaries: Vec<_> = evidence
            .iter()
            .filter(|event| event.kind == "rpc.boundary" && event.data["batch_id"].is_u64())
            .collect();
        assert_eq!(boundaries.len(), 2);
        for boundary in boundaries {
            assert_eq!(boundary.data["shared_withholding"], true);
            assert_eq!(boundary.data["rpc_response"], "no_transport_response");
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn custom_timeout_repeat_advances_at_exact_simulator_boundaries() {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        server.rpc.wait_initialized().await.unwrap();
        let mut repeated = artifact(
            &["rpc.timeout-before-dispatch@1", "observe.rpc@1"],
            &["getSlot"],
            "case",
            json!([
                {"id":"outage","using":"rpc.timeout-before-dispatch@1","with":{"hold_real_ms":20},"until":{"elapsed_ms":1000,"clock":"simulator"},"next":"recover"},
                {"id":"recover","using":"observe.rpc@1","until":{"elapsed_ms":500,"clock":"simulator"},"next":"outage"}
            ]),
        );
        repeated["repeat"] = json!({"cycles":2});
        arm(&server, &repeated).await;
        assert!(server.http(rpc(1, "getSlot", json!([]))).await.is_err());
        server.time.advance(Duration::from_millis(999)).unwrap();
        assert!(server.http(rpc(2, "getSlot", json!([]))).await.is_err());
        server.time.advance(Duration::from_millis(1)).unwrap();
        assert!(server.call("getSlot", json!([])).await["result"].is_u64());
        server.time.advance(Duration::from_millis(499)).unwrap();
        assert!(server.call("getSlot", json!([])).await["result"].is_u64());
        server.time.advance(Duration::from_millis(1)).unwrap();
        assert!(server.http(rpc(3, "getSlot", json!([]))).await.is_err());
        server.time.advance(Duration::from_millis(1000)).unwrap();
        assert!(server.call("getSlot", json!([])).await["result"].is_u64());
        server.time.advance(Duration::from_millis(500)).unwrap();
        assert!(server.call("getSlot", json!([])).await["result"].is_u64());
        server.time.advance(Duration::from_secs(100)).unwrap();
        assert!(server.call("getSlot", json!([])).await["result"].is_u64());
        assert!(server.stop(false).await);
        let evidence = events(&server).await;
        let selected: Vec<_> = evidence
            .iter()
            .filter(|event| event.kind == "custom.selection")
            .map(|event| {
                (
                    event.data["request"]["registered_simulated_ns"]
                        .as_u64()
                        .unwrap(),
                    event.data["selection"]["step"].as_str().unwrap(),
                    event.data["selection"]["cycle"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            selected,
            vec![
                (0, "outage", 0),
                (999_000_000, "outage", 0),
                (1_000_000_000, "recover", 0),
                (1_499_000_000, "recover", 0),
                (1_500_000_000, "outage", 1),
                (2_500_000_000, "recover", 1)
            ]
        );
        assert_eq!(
            evidence
                .iter()
                .filter(|event| event.kind == "rpc.dispatch_prevented")
                .count(),
            3
        );
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn custom_mixed_key_timeout_wins_in_either_signature_order() {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        server.rpc.wait_initialized().await.unwrap();
        let hash = server.hash().await;
        let (a, wire_a) = signed(&hash, 187, false);
        let (b, wire_b) = signed(&hash, 188, false);
        assert_eq!(
            server.call("sendTransaction", send(&wire_a)).await["result"],
            a
        );
        assert_eq!(
            server.call("sendTransaction", send(&wire_b)).await["result"],
            b
        );
        server.time.advance(Duration::from_secs(6)).unwrap();
        server.phase(&b, Phase::Finalized).await;
        let artifact = artifact(
            &["rpc.timeout-before-dispatch@1", "observe.rpc@1"],
            &["getSignatureStatuses"],
            "transaction_signature",
            json!([
                {"id":"outage","using":"rpc.timeout-before-dispatch@1","with":{"hold_real_ms":20},"until":{"matches":1},"next":"observe"},
                {"id":"observe","using":"observe.rpc@1","until":{"matches":100},"next":"done"}
            ]),
        );
        for signatures in [[a.clone(), b.clone()], [b.clone(), a.clone()]] {
            arm(&server, &artifact).await;
            assert!(
                server
                    .http(rpc(1, "getSignatureStatuses", json!([[a]])))
                    .await
                    .is_err()
            );
            assert!(
                server
                    .http(rpc(2, "getSignatureStatuses", json!([signatures])))
                    .await
                    .is_err()
            );
            let source = server.call("getSignatureStatuses", json!([[a, b]])).await;
            assert_eq!(source["result"]["value"].as_array().unwrap().len(), 2);
            for value in source["result"]["value"].as_array().unwrap() {
                assert_eq!(value["confirmationStatus"], "finalized");
            }
        }
        assert!(server.stop(false).await);
        let evidence = events(&server).await;
        assert_eq!(
            evidence
                .iter()
                .filter(|event| event.kind == "rpc.dispatch_prevented")
                .count(),
            4
        );
        let mixed: Vec<_> = evidence
            .iter()
            .filter(|event| {
                event.kind == "rpc.dispatch_prevented"
                    && event.data["signatures"]
                        .as_array()
                        .is_some_and(|a| a.len() == 2)
            })
            .collect();
        assert_eq!(mixed.len(), 2);
        assert_eq!(mixed[0].data["signatures"], json!([a, b]));
        assert_eq!(mixed[1].data["signatures"], json!([b, a]));
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn custom_maximum_status_array_keeps_selection_evidence_bounded_and_lab_healthy() {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        server.rpc.wait_initialized().await.unwrap();
        let mut signatures = Vec::with_capacity(256);
        // Each group fills the 128 pending-lifecycle slots, then releases its credits.
        for group in 0..2 {
            let hash = server.hash().await;
            for index in 0..128 {
                let (signature, wire) = signed(&hash, (group * 128 + index) as u8, false);
                assert_eq!(
                    server.call("sendTransaction", send(&wire)).await["result"],
                    signature
                );
                signatures.push(signature);
            }
            server.time.advance(Duration::from_secs(6)).unwrap();
            // This bulk fixture commits 384 transitions, unlike the small-history helper.
            // Keep its wall-clock wait local; the simulator deadlines remain unchanged.
            let started = Instant::now();
            let mut next_progress = Duration::from_secs(1);
            loop {
                let record = server.record(signatures.last().unwrap()).await.unwrap();
                let health = server.writer.as_ref().unwrap().handle().status().health;
                assert_eq!(
                    health,
                    WriterHealth::Healthy,
                    "bulk group {group}: {record:?}"
                );
                if record.stage == Phase::Finalized {
                    eprintln!("bulk group {group} finalized after {:?}", started.elapsed());
                    break;
                }
                if started.elapsed() >= next_progress {
                    eprintln!(
                        "bulk group {group} after {:?}: stage={:?}, writer={health:?}, actor_finished={}",
                        started.elapsed(),
                        record.stage,
                        server.task.as_ref().unwrap().is_finished()
                    );
                    next_progress += Duration::from_secs(1);
                }
                assert!(
                    started.elapsed() < Duration::from_secs(30),
                    "bulk finalization stalled: {record:?}"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
        let mut artifact = artifact(
            &["rpc.lookup-null@1"],
            &["getSignatureStatuses"],
            "transaction_signature",
            json!([
                {"id":"hide","using":"rpc.lookup-null@1","until":{"matches":1},"next":"done"}
            ]),
        );
        artifact["state"]["max_keys"] = json!(256);
        arm(&server, &artifact).await;
        let masked = bulk_statuses(&server, &signatures).await;
        let values = masked["result"]["value"]
            .as_array()
            .unwrap_or_else(|| panic!("{masked}"));
        assert_eq!(values.len(), 256);
        assert!(values.iter().all(Value::is_null));
        assert_eq!(server.call("getHealth", json!([])).await["result"], "ok");
        assert_eq!(
            server.writer.as_ref().unwrap().handle().status().health,
            WriterHealth::Healthy
        );
        let restored = bulk_statuses(&server, &signatures).await;
        let values = restored["result"]["value"]
            .as_array()
            .unwrap_or_else(|| panic!("{restored}"));
        assert_eq!(values.len(), 256);
        assert!(
            values
                .iter()
                .all(|value| value["confirmationStatus"] == "finalized")
        );
        assert!(server.stop(false).await);
        let evidence = events(&server).await;
        let selected: Vec<_> = evidence
            .iter()
            .filter(|event| event.kind == "custom.selection")
            .collect();
        assert_eq!(selected.len(), 256);
        let changed: Vec<_> = evidence
            .iter()
            .filter(|event| event.kind == "fault.custom_lookup_changed")
            .collect();
        assert_eq!(changed.len(), 256);
        for event in selected.iter().chain(changed.iter()) {
            assert!(serde_json::to_vec(event).unwrap().len() <= ronchi::journal::MAX_EVENT_BYTES);
        }
    }
}
