use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
async fn raw(server: &Server, value: Value) -> tokio::net::TcpStream {
    let mut stream = tokio::net::TcpStream::connect(server.address)
        .await
        .unwrap();
    let body = value.to_string();
    stream
        .write_all(
            format!(
                "POST / HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\n\r\n{body}",
                server.address,
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    stream
}
fn request(wire: &[u8], id: u64) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"sendTransaction","params":send(wire)})
}
async fn held(server: &Server) -> Vec<ronchi::cases::HeldEffect> {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let effects = server.rpc.held_effects();
        if !effects.is_empty() {
            return effects;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
async fn released(server: &Server) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while !server.rpc.held_effects().is_empty() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
async fn eof(stream: &mut tokio::net::TcpStream) {
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert!(
        bytes.is_empty(),
        "withholding sent transport bytes: {bytes:?}"
    );
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
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_retries_and_reactivation_keep_one_fault_per_signature_not_per_hash() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 120,
            ..Default::default()
        })
        .await
        .unwrap();
    let hash = server.hash().await;
    let (a, wire) = signed(&hash, 41, false);
    let replies = tokio::join!(
        server.http(request(&wire, 1)),
        server.http(request(&wire, 2)),
        server.http(request(&wire, 3)),
        server.http(request(&wire, 4))
    );
    let replies = [replies.0, replies.1, replies.2, replies.3];
    assert_eq!(replies.iter().filter(|reply| reply.is_err()).count(), 1);
    for (_, body) in replies.into_iter().flatten() {
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["result"], a);
    }
    server.rpc.disarm_case().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 100,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        a
    );
    let (b, wire_b) = signed(&hash, 42, false);
    let mut stream = raw(&server, request(&wire_b, 5)).await;
    held(&server).await;
    eof(&mut stream).await;
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&a, Phase::Finalized).await;
    server.phase(&b, Phase::Finalized).await;
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert!(reader.response_loss(&a).unwrap().is_some());
    assert!(reader.response_loss(&b).unwrap().is_some());
    assert_eq!(
        events(&server)
            .await
            .iter()
            .filter(|event| event.kind == "fault.response_loss_selected")
            .count(),
        2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hold_capacity_rejects_before_admission_and_peer_disconnect_releases_it() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 2000,
            max_held: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    let hash = server.hash().await;
    let (a, wire) = signed(&hash, 43, false);
    let (b, wire_b) = signed(&hash, 44, false);
    let stream = raw(&server, request(&wire, 1)).await;
    held(&server).await;
    assert_eq!(
        server.call("sendTransaction", send(&wire_b)).await["error"]["data"]["code"],
        "case_capacity_exceeded"
    );
    assert!(server.record(&b).await.is_none());
    assert_eq!(
        server.writer.as_ref().unwrap().handle().status().health,
        WriterHealth::Healthy
    );
    drop(stream);
    released(&server).await;
    let mut next = raw(&server, request(&wire_b, 2)).await;
    held(&server).await;
    server.phase(&b, Phase::Pending).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(40), next.read_u8())
            .await
            .is_err()
    );
    drop(next);
    released(&server).await;
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&a, Phase::Finalized).await;
    server.phase(&b, Phase::Finalized).await;
    assert!(server.stop(false).await);
    let boundary_events = events(&server).await;
    let held_boundaries: Vec<_> = boundary_events
        .iter()
        .filter(|event| event.kind == "rpc.boundary" && event.data["shared_withholding"] == true)
        .collect();
    assert_eq!(held_boundaries.len(), 2);
    for event in held_boundaries {
        assert_eq!(event.data["rpc_response"], "no_transport_response");
        // Hyper can observe a transport error before the peer monitor observes EOF.
        // Both observations release the hold; neither establishes a client timeout.
        assert!(
            matches!(
                event.data["transport"].as_str(),
                Some(
                    "peer_closed_while_withheld"
                        | "transport_failed"
                        | "transport_failed_while_withheld"
                )
            ),
            "{:?}",
            event.data
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_case_registration_keeps_snapshot_when_arm_precedes_its_acceptance() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let (signature, wire) = signed(&server.hash().await, 45, false);
    let pause = server.rpc.pause_next_registration();
    let mut stream = raw(&server, request(&wire, 1)).await;
    tokio::time::timeout(Duration::from_secs(3), pause.registered())
        .await
        .unwrap();
    let armed = server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 100,
            ..Default::default()
        })
        .await
        .unwrap();
    pause.resume();
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    let reply: Value = serde_json::from_str(
        std::str::from_utf8(&bytes)
            .unwrap()
            .split_once("\r\n\r\n")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(reply["result"], signature);
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert!(reader.response_loss(&signature).unwrap().is_none());
    let record = reader.transaction(&signature).unwrap().unwrap();
    assert!(record.admission_request["binding"]["policy"].is_null());
    assert!(
        record.admission_request["registration_seq"]
            .as_u64()
            .unwrap()
            < armed.registration_seq
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disarm_and_replacement_keep_original_hold_deadline_and_policy_revision() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let initial = server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 400,
            ..Default::default()
        })
        .await
        .unwrap();
    let hash = server.hash().await;
    let (a, wire) = signed(&hash, 46, false);
    let (_, wire_b) = signed(&hash, 47, false);
    let mut stream = raw(&server, request(&wire, 1)).await;
    let effects = held(&server).await;
    assert_eq!(effects[0].policy_revision, initial.policy.unwrap().revision);
    server.rpc.disarm_case().await.unwrap();
    assert!(server.rpc.case_status().policy.is_none());
    assert!(
        server
            .call("sendTransaction", send(&wire_b))
            .await
            .get("result")
            .is_some()
    );
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 50,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), stream.read_u8())
            .await
            .is_err()
    );
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 2000,
            ..Default::default()
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_millis(800), eof(&mut stream))
        .await
        .unwrap();
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&a, Phase::Finalized).await;
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn batch_has_one_binding_one_hold_slot_and_per_element_boundary_without_notification_selection()
 {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let binding = server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 180,
            max_held: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        server.call("sendTransaction", json!([])).await["error"]["code"],
        -32602
    );
    let hash = server.hash().await;
    let (a, wa) = signed(&hash, 48, false);
    let (b, wb) = signed(&hash, 49, false);
    let (c, wc) = signed(&hash, 50, false);
    assert_eq!(
        server
            .call(
                "sendTransaction",
                json!([STANDARD.encode(&wa),{"encoding":"base64"}])
            )
            .await["error"]["data"]["code"],
        "unsupported_capability"
    );
    let body = json!([{"jsonrpc":"2.0","id":1,"method":"getHealth"},request(&wa,2),request(&wb,3),{"jsonrpc":"2.0","method":"sendTransaction","params":send(&wc)},false]);
    let mut stream = raw(&server, body).await;
    let effects = held(&server).await;
    assert_eq!(effects.len(), 1);
    eof(&mut stream).await;
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&a, Phase::Finalized).await;
    server.phase(&b, Phase::Finalized).await;
    server.phase(&c, Phase::Finalized).await;
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert!(reader.response_loss(&a).unwrap().is_some());
    assert!(reader.response_loss(&b).unwrap().is_some());
    assert!(reader.response_loss(&c).unwrap().is_none());
    let facts = events(&server).await;
    let selected: Vec<_> = facts
        .iter()
        .filter(|event| event.kind == "fault.response_loss_selected")
        .collect();
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[0].data["signature"], a);
    assert_eq!(selected[1].data["signature"], b);
    let seq = effects[0].registration_seq;
    let registered: Vec<_> = facts
        .iter()
        .filter(|event| event.kind == "rpc.registered" && event.data["registration_seq"] == seq)
        .collect();
    assert_eq!(registered.len(), 5);
    for event in registered {
        assert_eq!(
            event.data["binding"]["policy"]["revision"],
            binding.policy.as_ref().unwrap().revision
        );
        assert_eq!(event.data["batch_id"], seq);
    }
    let boundaries: Vec<_> = facts
        .iter()
        .filter(|event| event.kind == "rpc.boundary" && event.data["registration_seq"] == seq)
        .collect();
    assert_eq!(boundaries.len(), 5);
    for event in boundaries {
        assert_eq!(event.data["shared_withholding"], true);
        assert_eq!(event.data["client_receipt"], "unknown");
    }
    assert!(
        facts
            .iter()
            .any(|event| event.kind == "rpc.response_prepared"
                && event.data["registration_seq"] == seq
                && event.data["result"] == "ok")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ledger_write_failure_rolls_back_identity_acceptance_and_selection_without_starting_hold() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 100,
            ..Default::default()
        })
        .await
        .unwrap();
    let (signature, wire) = signed(&server.hash().await, 51, false);
    let path = server.directory.path().join("journal.sqlite");
    tokio::task::spawn_blocking(move||{let c=rusqlite::Connection::open(path).unwrap();c.execute_batch("CREATE TRIGGER fail_ledger BEFORE INSERT ON response_loss BEGIN SELECT RAISE(ABORT,'injected ledger failure'); END;").unwrap();}).await.unwrap();
    if let Ok((_, body)) = server.http(request(&wire, 1)).await {
        let response: Value = serde_json::from_slice(&body).unwrap();
        assert!(response.get("result").is_none());
    }
    assert!(!server.stop(true).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert!(reader.transaction(&signature).unwrap().is_none());
    assert!(reader.response_loss(&signature).unwrap().is_none());
    let facts = events(&server).await;
    assert!(!facts.iter().any(|event| {
        [
            "transaction.accepted",
            "fault.response_loss_selected",
            "fault.withholding_started",
        ]
        .contains(&event.kind.as_str())
    }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_shutdown_drains_original_hold_and_force_records_transport_cancellation() {
    for force in [false, true] {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        server.rpc.wait_initialized().await.unwrap();
        server
            .rpc
            .arm_case(ronchi::cases::Arm {
                hold_ms: 240,
                ..Default::default()
            })
            .await
            .unwrap();
        let (signature, wire) = signed(&server.hash().await, 52, false);
        let mut stream = raw(&server, request(&wire, 1)).await;
        held(&server).await;
        if !force {
            server.time.advance(Duration::from_secs(6)).unwrap();
        }
        {
            let stopping = server.stop(force);
            tokio::pin!(stopping);
            if !force {
                assert!(
                    tokio::time::timeout(Duration::from_millis(40), &mut stopping)
                        .await
                        .is_err()
                );
            }
            assert!(stopping.await);
        }
        eof(&mut stream).await;
        let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
        assert!(reader.response_loss(&signature).unwrap().is_some());
        assert_eq!(
            reader.transaction(&signature).unwrap().unwrap().stage,
            if force {
                Phase::Pending
            } else {
                Phase::Finalized
            }
        );
        assert!(
            events(&server)
                .await
                .iter()
                .any(|event| event.kind == "rpc.boundary"
                    && event.data["transport"]
                        == if force {
                            "cancelled"
                        } else {
                            "withholding_deadline"
                        })
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn held_nonce_submissions_preserve_formats_and_advance_once_despite_retries() {
    use ronchi::protocol::{nonce::NonceAccount, state::ProtocolState};
    for version in [-1, 0, 1] {
        let account = bs58::encode([75; 32]).into_string();
        let nonce = bs58::encode([98; 32]).into_string();
        let authority = bs58::encode(SigningKey::from_bytes(&[53; 32]).verifying_key().to_bytes())
            .into_string();
        let state = ProtocolState {
            lookup_tables: vec![],
            nonce_accounts: vec![NonceAccount {
                address: account.clone(),
                authority,
                nonce: nonce.clone(),
                slot: 0,
            }],
        };
        let mut server =
            Server::with_state(ClockProfile::default(), DiskLimits::default(), state).await;
        server.rpc.wait_initialized().await.unwrap();
        server
            .rpc
            .arm_case(ronchi::cases::Arm {
                hold_ms: 180,
                ..Default::default()
            })
            .await
            .unwrap();
        let (signature, wire) = signed_nonce(&nonce, 53, &account, version, 1);
        let mut stream = raw(&server, request(&wire, 1)).await;
        held(&server).await;
        server.time.advance(Duration::from_secs(6)).unwrap();
        server.phase(&signature, Phase::Finalized).await;
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        eof(&mut stream).await;
        let tx = server
            .call(
                "getTransaction",
                json!([signature,{"encoding":"base64","maxSupportedTransactionVersion":1}]),
            )
            .await;
        assert_eq!(tx["result"]["transaction"][0], STANDARD.encode(&wire));
        assert!(tx["result"]["meta"].is_null());
        assert!(server.stop(false).await);
        let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
        assert_ne!(reader.nonce(&account, None).unwrap().unwrap().nonce, nonce);
        assert!(reader.response_loss(&signature).unwrap().is_some());
        assert_eq!(
            events(&server)
                .await
                .iter()
                .filter(|event| event.kind == "transaction.transition"
                    && event.data["nonce_advance"].is_object())
                .count(),
            1
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signature_filter_notifications_and_default_hold_are_explicit() {
    assert_eq!(ronchi::cases::Arm::default().hold_ms, 20_000);
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    let hash = server.hash().await;
    let (a, wa) = signed(&hash, 54, false);
    let (b, wb) = signed(&hash, 55, false);
    let (c, wc) = signed(&hash, 56, false);
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 150,
            max_held: 1,
            signatures: vec![a.clone()],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(server.call("sendTransaction", send(&wb)).await["result"], b);
    let mut stream = raw(&server, request(&wa, 1)).await;
    held(&server).await;
    let notification = json!({"jsonrpc":"2.0","method":"sendTransaction","params":send(&wc)});
    let reply = server.http(notification).await.unwrap();
    assert_eq!(reply.0, 204);
    assert!(reply.1.is_empty());
    eof(&mut stream).await;
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&a, Phase::Finalized).await;
    server.phase(&c, Phase::Finalized).await;
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert!(reader.response_loss(&a).unwrap().is_some());
    assert!(reader.response_loss(&b).unwrap().is_none());
    assert!(reader.response_loss(&c).unwrap().is_none());
}
