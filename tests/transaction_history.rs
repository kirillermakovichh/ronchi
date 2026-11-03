use base64::{Engine, engine::general_purpose::STANDARD};
use bytes::Bytes;
use ed25519_dalek::{Signer, SigningKey};
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use ronchi::{
    journal::{
        DiskLimits, JournalReader, JournalStore, JournalWriter, MIN_SHUTDOWN_RESERVE,
        TransactionPhase as Phase, TransactionRecord, WriterHealth,
    },
    rpc::Rpc,
    simulator::clock::{Clock, ClockProfile, ManualTime},
};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

struct Server {
    rpc: Rpc,
    address: SocketAddr,
    time: ManualTime,
    stop: CancellationToken,
    force: CancellationToken,
    task: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    writer: Option<JournalWriter>,
    directory: tempfile::TempDir,
}
impl Server {
    async fn new(profile: ClockProfile, limits: DiskLimits) -> Self {
        Self::with_state(profile, limits, Default::default()).await
    }
    async fn with_state(
        profile: ClockProfile,
        limits: DiskLimits,
        state: ronchi::protocol::state::ProtocolState,
    ) -> Self {
        let directory = tempfile::TempDir::new().unwrap();
        let writer = JournalWriter::start(
            JournalStore::with_disk_limits(
                &directory.path().join("journal.sqlite"),
                "L-transactions",
                limits,
            )
            .unwrap(),
        )
        .unwrap();
        let (clock, time) = Clock::controlled(profile, b"transaction-test-network").unwrap();
        let rpc = Rpc::with_state(clock, writer.handle(), Instant::now(), state).unwrap();
        rpc.set_ready(true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let stop = CancellationToken::new();
        let force = CancellationToken::new();
        let task = tokio::spawn(rpc.clone().serve(listener, stop.clone(), force.clone()));
        Self {
            rpc,
            address,
            time,
            stop,
            force,
            task: Some(task),
            writer: Some(writer),
            directory,
        }
    }
    async fn http(&self, value: Value) -> Result<(u16, Vec<u8>), String> {
        let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
            .pool_max_idle_per_host(0)
            .build_http();
        let request = hyper::Request::builder()
            .method("POST")
            .uri(format!("http://{}/", self.address))
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(value.to_string())))
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(5), client.request(request))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .to_bytes();
        Ok((status, bytes.to_vec()))
    }
    async fn call(&self, method: &str, params: Value) -> Value {
        let (status, body) = self
            .http(json!({"jsonrpc":"2.0","id":"tx-query","method":method,"params":params}))
            .await
            .unwrap();
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["id"], "tx-query");
        value
    }
    async fn hash(&self) -> String {
        self.call("getLatestBlockhash", json!([{"commitment":"processed"}]))
            .await["result"]["value"]["blockhash"]
            .as_str()
            .unwrap()
            .to_string()
    }
    async fn record(&self, signature: &str) -> Option<TransactionRecord> {
        let path = self.directory.path().join("journal.sqlite");
        let signature = signature.to_string();
        tokio::task::spawn_blocking(move || {
            JournalReader::open(&path)
                .unwrap()
                .transaction(&signature)
                .unwrap()
        })
        .await
        .unwrap()
    }
    async fn phase(&self, signature: &str, phase: Phase) {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if self
                .record(signature)
                .await
                .is_some_and(|record| record.stage == phase)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "autonomous phase {phase:?} was not committed"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    async fn stop(&mut self, force: bool) -> bool {
        self.stop.cancel();
        if force {
            self.force.cancel();
        }
        let result = self.task.take().unwrap().await.unwrap();
        let writer = self.writer.take().unwrap();
        let persisted = tokio::task::spawn_blocking(move || writer.shutdown(None))
            .await
            .unwrap();
        result.is_ok() && persisted.is_ok()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
        self.force.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Independent legacy/v0 wire construction; production uses the official wincode decoder.
fn signed(hash: &str, seed: u8, v0: bool) -> (String, Vec<u8>) {
    let signer = SigningKey::from_bytes(&[seed; 32]);
    let mut message = Vec::new();
    if v0 {
        message.push(0x80);
    }
    message.extend_from_slice(&[1, 0, 0, 1]);
    message.extend_from_slice(&signer.verifying_key().to_bytes());
    message.extend_from_slice(&bs58::decode(hash).into_vec().unwrap());
    message.push(0);
    if v0 {
        message.push(0);
    }
    let signature = signer.sign(&message).to_bytes();
    let mut wire = vec![1];
    wire.extend_from_slice(&signature);
    wire.extend_from_slice(&message);
    (bs58::encode(signature).into_string(), wire)
}
fn send(wire: &[u8]) -> Value {
    json!([STANDARD.encode(wire),{"encoding":"base64","skipPreflight":true,"maxRetries":0}])
}
fn statuses(signature: &str) -> Value {
    json!([[signature]])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn durable_identity_and_autonomous_two_four_six_second_history_over_http() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let (signature, wire) = signed(&server.hash().await, 7, false);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    let accepted = server.record(&signature).await.unwrap();
    assert_eq!(accepted.wire_base64, STANDARD.encode(&wire));
    assert_eq!(accepted.accepted_ns, 0);
    assert_eq!(accepted.stage, Phase::Pending);
    assert!(
        server
            .call("getSignatureStatuses", statuses(&signature))
            .await["result"]["value"][0]
            .is_null()
    );
    assert!(server.call("getTransaction", json!([signature])).await["result"].is_null());
    server.time.advance(Duration::from_secs(1)).unwrap();
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    assert_eq!(server.record(&signature).await.unwrap().accepted_ns, 0);
    server.time.advance(Duration::from_secs(1)).unwrap();
    server.phase(&signature, Phase::Processed).await; // No status RPC drove the transition.
    let processed = server
        .call("getSignatureStatuses", statuses(&signature))
        .await;
    assert_eq!(
        processed["result"]["value"][0]["confirmationStatus"],
        "processed"
    );
    assert_eq!(processed["result"]["value"][0]["slot"], 5);
    assert_eq!(processed["result"]["value"][0]["confirmations"], 0);
    server.time.advance(Duration::from_secs(2)).unwrap();
    server.phase(&signature, Phase::Confirmed).await;
    let confirmed = server
        .call("getSignatureStatuses", statuses(&signature))
        .await;
    assert_eq!(
        confirmed["result"]["value"][0]["confirmationStatus"],
        "confirmed"
    );
    assert_eq!(confirmed["result"]["value"][0]["confirmations"], 5);
    assert!(server.call("getTransaction", json!([signature])).await["result"].is_null());
    let lookup = server
        .call(
            "getTransaction",
            json!([signature,{"commitment":"confirmed","encoding":"base64"}]),
        )
        .await;
    assert!(lookup["result"]["meta"].is_null());
    assert_eq!(
        lookup["result"]["transaction"],
        json!([STANDARD.encode(&wire), "base64"])
    );
    assert_eq!(lookup["result"]["slot"], 5);
    server.time.advance(Duration::from_secs(2)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    let unknown = bs58::encode([99u8; 64]).into_string();
    let status = server
        .call(
            "getSignatureStatuses",
            json!([[signature, unknown, signature]]),
        )
        .await;
    assert_eq!(
        status["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert!(status["result"]["value"][0]["confirmations"].is_null());
    assert!(status["result"]["value"][1].is_null());
    assert_eq!(status["result"]["value"][0], status["result"]["value"][2]);
    let lookup = server.call("getTransaction", json!([signature])).await;
    assert_eq!(lookup["result"]["transaction"]["signatures"][0], signature);
    assert!(lookup["result"]["meta"].is_null());
    assert!(lookup["result"]["blockTime"].is_null());
    server.time.advance(Duration::from_secs(140)).unwrap();
    assert!(
        server
            .call("getSignatureStatuses", statuses(&signature))
            .await["result"]["value"][0]
            .is_null()
    );
    assert_eq!(
        server
            .call(
                "getSignatureStatuses",
                json!([[signature],{"searchTransactionHistory":true}])
            )
            .await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    ); // Expiry never erases prior processing.
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert_eq!(
        reader.transaction(&signature).unwrap().unwrap().wire_base64,
        STANDARD.encode(&wire)
    );
    let mut acceptances = 0;
    let mut attempts = 0;
    let mut stages = Vec::new();
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "transaction.accepted" {
            acceptances += 1;
            assert_eq!(event.data["execution"], "not_performed");
        }
        if event.kind == "transaction.submission" {
            attempts += 1;
        }
        if event.kind == "transaction.transition" {
            stages.push((
                event.data["stage"].as_str().unwrap().to_string(),
                event.data["scheduled_transition_ns"].as_u64().unwrap(),
            ));
        }
    }
    assert_eq!(acceptances, 1);
    assert_eq!(attempts, 3);
    assert_eq!(
        stages,
        vec![
            ("processed".into(), 2_000_000_000),
            ("confirmed".into(), 4_000_000_000),
            ("finalized".into(), 6_000_000_000)
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expiry_before_processing_and_processing_at_the_inclusive_bound_are_distinct() {
    for (window, expected) in [(4, Phase::Expired), (5, Phase::Processed)] {
        let mut server = Server::new(
            ClockProfile {
                validity_window_blocks: window,
                ..Default::default()
            },
            DiskLimits::default(),
        )
        .await;
        let (signature, wire) = signed(&server.hash().await, 8, false);
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        server.time.advance(Duration::from_secs(2)).unwrap();
        server.phase(&signature, expected).await;
        let status = server
            .call("getSignatureStatuses", statuses(&signature))
            .await;
        if expected == Phase::Expired {
            assert!(status["result"]["value"][0].is_null());
            assert!(server.call("getTransaction", json!([signature])).await["result"].is_null());
            assert!(
                server
                    .record(&signature)
                    .await
                    .unwrap()
                    .processed_slot
                    .is_none()
            );
        } else {
            assert_eq!(
                status["result"]["value"][0]["confirmationStatus"],
                "processed"
            );
            server.time.advance(Duration::from_millis(400)).unwrap();
            assert_eq!(
                server
                    .call("getSignatureStatuses", statuses(&signature))
                    .await["result"]["value"][0]["confirmationStatus"],
                "processed"
            );
            server.time.advance(Duration::from_millis(3600)).unwrap();
            server.phase(&signature, Phase::Finalized).await;
        }
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        assert!(server.stop(false).await);
    }
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let (signature, wire) = signed(&bs58::encode([9u8; 32]).into_string(), 9, false);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    server.time.advance(Duration::from_secs(2)).unwrap();
    server.phase(&signature, Phase::Unprocessed).await;
    assert!(
        server
            .call("getSignatureStatuses", statuses(&signature))
            .await["result"]["value"][0]
            .is_null()
    );
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_wire_concurrent_submissions_have_one_lifecycle_and_notifications_do_not_lose_it() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let (signature, wire) = signed(&server.hash().await, 10, true);
    let mut tasks = tokio::task::JoinSet::new();
    for id in 0..8 {
        let address = server.address;
        let wire = wire.clone();
        tasks.spawn(async move {
            let client: Client<HttpConnector, Full<Bytes>> =
                Client::builder(TokioExecutor::new()).build_http();
            let body =
                json!({"jsonrpc":"2.0","id":id,"method":"sendTransaction","params":send(&wire)});
            let request = hyper::Request::builder()
                .method("POST")
                .uri(format!("http://{address}/"))
                .header("content-type", "application/json")
                .body(Full::new(Bytes::from(body.to_string())))
                .unwrap();
            let response = client.request(request).await.unwrap();
            serde_json::from_slice::<Value>(
                &response.into_body().collect().await.unwrap().to_bytes(),
            )
            .unwrap()
        });
    }
    while let Some(reply) = tasks.join_next().await {
        assert_eq!(reply.unwrap()["result"], signature);
    }
    let response=server.http(json!([{"jsonrpc":"2.0","method":"sendTransaction","params":send(&wire)},{"jsonrpc":"2.0","id":7,"method":"getSignatureStatuses","params":statuses(&signature)}])).await.unwrap();
    let values: Value = serde_json::from_slice(&response.1).unwrap();
    assert_eq!(values.as_array().unwrap().len(), 1);
    assert!(values[0]["result"]["value"][0].is_null());
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    assert_eq!(
        server
            .call("getTransaction", json!([signature,{"encoding":"base64"}]))
            .await["error"]["code"],
        -32015
    );
    let tx = server
        .call(
            "getTransaction",
            json!([signature,{"encoding":"base64","maxSupportedTransactionVersion":0}]),
        )
        .await;
    assert_eq!(tx["result"]["version"], 0);
    assert_eq!(
        tx["result"]["transaction"],
        json!([STANDARD.encode(&wire), "base64"])
    );
    assert!(tx["result"]["meta"].is_null());
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    let mut accepted = 0;
    let mut attempts = 0;
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "transaction.accepted" {
            accepted += 1;
        }
        if event.kind == "transaction.submission" {
            attempts += 1;
        }
    }
    assert_eq!(accepted, 1);
    assert_eq!(attempts, 9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preflight_parameter_validation_and_signature_errors_are_explicit_over_http() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let (signature, wire) = signed(&server.hash().await, 11, false);
    let encoded = STANDARD.encode(&wire);
    for params in [
        json!([encoded]),
        json!([encoded,{"encoding":"base64","skipPreflight":false}]),
    ] {
        let reply = server.call("sendTransaction", params).await;
        assert_eq!(reply["error"]["data"]["code"], "unsupported_capability");
        assert!(
            reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("preflight")
        );
    }
    assert_eq!(
        server
            .call(
                "sendTransaction",
                json!([encoded,{"encoding":"base64","skipPreflight":true,"maxRetries":1}])
            )
            .await["error"]["data"]["code"],
        "unsupported_capability"
    );
    for params in [
        json!([encoded,{"skipPreflight":true,"unknown":1}]),
        json!([encoded,{"skipPreflight":true,"preflightCommitment":"wrong"}]),
        json!(["bad",{"encoding":"base64","skipPreflight":true}]),
    ] {
        assert_eq!(
            server.call("sendTransaction", params).await["error"]["code"],
            -32602
        );
    }
    let mut invalid = wire.clone();
    invalid[5] ^= 1;
    assert_eq!(
        server.call("sendTransaction", send(&invalid)).await["error"]["code"],
        -32003
    );
    assert!(server.record(&signature).await.is_none());
    assert_eq!(
        server
            .call(
                "sendTransaction",
                json!([encoded,{"encoding":"base64","skipPreflight":true,"minContextSlot":1}])
            )
            .await["error"]["code"],
        -32016
    );
    assert_eq!(server.call("sendTransaction",json!([bs58::encode(&wire).into_string(),{"skipPreflight":true,"maxRetries":0,"preflightCommitment":"finalized"}])).await["result"],signature);
    assert_eq!(
        server
            .call(
                "getSignatureStatuses",
                json!([[signature],{"commitment":"finalized"}])
            )
            .await["error"]["code"],
        -32602
    );
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
        server
            .call(
                "getSignatureStatuses",
                json!([vec![signature.clone(); 257]])
            )
            .await["error"]["code"],
        -32602
    );
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    assert!(
        server
            .call(
                "getTransaction",
                json!([signature,{"encoding":"jsonParsed"}])
            )
            .await["result"]["transaction"]["message"]["accountKeys"]
            .is_array()
    );
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reserved_future_work_drains_on_normal_stop_and_force_preserves_pending_identity() {
    for force in [false, true] {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        let (signature, wire) = signed(&server.hash().await, 12, false);
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        let usage = server
            .writer
            .as_ref()
            .unwrap()
            .handle()
            .disk_usage()
            .unwrap();
        assert!(usage.reserved_bytes > 3 * 1024 * 1024);
        assert!(usage.used_bytes > 0);
        if !force {
            server.stop.cancel();
            assert!(
                tokio::time::timeout(Duration::from_millis(50), server.task.as_mut().unwrap())
                    .await
                    .is_err()
            );
            assert_eq!(
                server.record(&signature).await.unwrap().stage,
                Phase::Pending
            );
            server.time.advance(Duration::from_secs(6)).unwrap();
        }
        assert!(server.stop(force).await);
        let row = server.record(&signature).await.unwrap();
        assert_eq!(
            row.stage,
            if force {
                Phase::Pending
            } else {
                Phase::Finalized
            }
        );
        assert_eq!(row.wire_base64, STANDARD.encode(wire));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn future_capacity_failure_never_acknowledges_an_uncommitted_identity() {
    let mut server = Server::new(
        ClockProfile::default(),
        DiskLimits {
            total_bytes: MIN_SHUTDOWN_RESERVE + 7 * 1024 * 1024,
            shutdown_reserve_bytes: MIN_SHUTDOWN_RESERVE,
        },
    )
    .await;
    let hash = server.hash().await;
    let (first, wire) = signed(&hash, 13, false);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        first
    );
    let (second, wire2) = signed(&hash, 14, false);
    let response = server
        .http(json!({"jsonrpc":"2.0","id":2,"method":"sendTransaction","params":send(&wire2)}))
        .await;
    if let Ok((_, body)) = response {
        let response: Value = serde_json::from_slice(&body).unwrap();
        assert!(response.get("result").is_none());
    }
    assert_eq!(
        server.writer.as_ref().unwrap().handle().status().health,
        WriterHealth::Failed
    );
    assert!(server.record(&second).await.is_none());
    assert_eq!(
        server.record(&first).await.unwrap().wire_base64,
        STANDARD.encode(&wire)
    );
    assert!(!server.stop(true).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert!(reader.transaction(&first).unwrap().is_some());
    assert!(reader.transaction(&second).unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn acceptance_projection_failure_rolls_back_signed_identity_and_attempt() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let hash = server.hash().await;
    let (signature, wire) = signed(&hash, 15, false);
    let path = server.directory.path().join("journal.sqlite");
    tokio::task::spawn_blocking(move || {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection.execute_batch("CREATE TRIGGER reject_identity BEFORE INSERT ON transactions BEGIN SELECT RAISE(ABORT, 'injected identity storage failure'); END;").unwrap();
    }).await.unwrap();
    let response = server
        .http(json!({"jsonrpc":"2.0","id":9,"method":"sendTransaction","params":send(&wire)}))
        .await;
    if let Ok((_, body)) = response {
        let response: Value = serde_json::from_slice(&body).unwrap();
        assert!(response.get("result").is_none());
    }
    assert!(!server.stop(true).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert!(reader.transaction(&signature).unwrap().is_none());
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        assert!(!matches!(
            event.kind.as_str(),
            "transaction.accepted" | "transaction.submission"
        ));
    }
}

/// SIMD-0385 wire assembled without the production codec.
fn signed_v1(hash: &str, seed: u8) -> (String, Vec<u8>) {
    let signer = SigningKey::from_bytes(&[seed; 32]);
    let mut message = vec![0x81, 1, 0, 0];
    message.extend_from_slice(&0u32.to_le_bytes());
    message.extend_from_slice(&bs58::decode(hash).into_vec().unwrap());
    message.extend_from_slice(&[0, 1]); // instruction count, address count
    message.extend_from_slice(&signer.verifying_key().to_bytes());
    let signature = signer.sign(&message).to_bytes();
    let mut wire = message;
    wire.extend_from_slice(&signature);
    (bs58::encode(signature).into_string(), wire)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn independent_three_format_binary_roundtrips_and_client_version_limits() {
    for version in [-1, 0, 1] {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        let hash = server.hash().await;
        let (signature, wire) = if version == 1 {
            signed_v1(&hash, 21)
        } else {
            signed(&hash, 21, version == 0)
        };
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        assert_eq!(
            server
                .call(
                    "sendTransaction",
                    json!([bs58::encode(&wire).into_string(),{"skipPreflight":true}])
                )
                .await["result"],
            signature
        );
        server.time.advance(Duration::from_secs(6)).unwrap();
        server.phase(&signature, Phase::Finalized).await;
        for encoding in ["base64", "base58"] {
            let reply = server
                .call(
                    "getTransaction",
                    json!([signature,{"encoding":encoding,"maxSupportedTransactionVersion":1}]),
                )
                .await;
            let bytes = reply["result"]["transaction"][0].as_str().unwrap();
            let decoded = if encoding == "base64" {
                STANDARD.decode(bytes).unwrap()
            } else {
                bs58::decode(bytes).into_vec().unwrap()
            };
            assert_eq!(decoded, wire);
        }
        if version >= 0 {
            assert_eq!(
                server.call("getTransaction", json!([signature])).await["error"]["code"],
                -32015
            );
        }
        if version == 1 {
            assert_eq!(
                server
                    .call(
                        "getTransaction",
                        json!([signature,{"maxSupportedTransactionVersion":0}])
                    )
                    .await["error"]["code"],
                -32015
            );
            let reply = server
                .call(
                    "getTransaction",
                    json!([signature,{"maxSupportedTransactionVersion":1}]),
                )
                .await;
            assert_eq!(reply["result"]["version"], 1);
            assert_eq!(
                reply["result"]["transaction"]["message"]["recentBlockhash"],
                hash
            );
            assert!(reply["result"]["transaction"]["message"]["transactionConfig"].is_object());
            assert_eq!(reply["result"]["transaction"]["signatures"][0], signature);
        }
        let parsed = server
            .call(
                "getTransaction",
                json!([signature,{"encoding":"jsonParsed","maxSupportedTransactionVersion":1}]),
            )
            .await;
        assert_eq!(parsed["result"]["transaction"]["accountKeys"], Value::Null);
        assert_eq!(
            parsed["result"]["transaction"]["message"]["accountKeys"][0]["signer"],
            true
        );
        assert_eq!(parsed["result"]["transaction"]["signatures"][0], signature);
        let mut trailing = wire.clone();
        trailing.push(0);
        assert_eq!(
            server.call("sendTransaction", send(&trailing)).await["error"]["code"],
            -32602
        );
        assert!(server.stop(false).await);
        assert_eq!(
            server.record(&signature).await.unwrap().wire_base64,
            STANDARD.encode(wire)
        );
    }
}

fn signed_lookup(hash: &str, seed: u8, tables: &[(&str, Vec<u8>, Vec<u8>)]) -> (String, Vec<u8>) {
    let signer = SigningKey::from_bytes(&[seed; 32]);
    let mut message = vec![0x80, 1, 0, 1, 2];
    message.extend_from_slice(&signer.verifying_key().to_bytes());
    message.extend_from_slice(&[0; 32]); // system program
    message.extend_from_slice(&bs58::decode(hash).into_vec().unwrap());
    message.extend_from_slice(&[2, 1, 2, 0, 2, 12]); // transfer to first loaded writable
    message.extend_from_slice(&2u32.to_le_bytes());
    message.extend_from_slice(&42u64.to_le_bytes());
    message.extend_from_slice(&[1, 2, 2, 3, 2, 9, 8]); // unknown system instruction
    message.push(tables.len() as u8);
    for (table, w, r) in tables {
        message.extend_from_slice(&bs58::decode(table).into_vec().unwrap());
        message.push(w.len() as u8);
        message.extend_from_slice(w);
        message.push(r.len() as u8);
        message.extend_from_slice(r);
    }
    let signature = signer.sign(&message).to_bytes();
    let mut wire = vec![1];
    wire.extend_from_slice(&signature);
    wire.extend(message);
    (bs58::encode(signature).into_string(), wire)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parsed_lookup_requires_state_and_preserves_writable_then_readonly_order() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let table = bs58::encode([31; 32]).into_string();
    let (signature, wire) = signed_lookup(&server.hash().await, 22, &[(&table, vec![1], vec![0])]);
    let reply = server.call("sendTransaction", send(&wire)).await;
    assert_eq!(reply["error"]["data"]["code"], "state_unavailable");
    assert!(server.record(&signature).await.is_none());
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_lookup_fixture_resolves_parsed_instructions_and_persists_context() {
    use ronchi::protocol::state::{LookupTable, ProtocolState};
    let key = |n| bs58::encode([n; 32]).into_string();
    let a = key(31);
    let b = key(32);
    let state = ProtocolState {
        nonce_accounts: vec![],
        lookup_tables: vec![
            LookupTable {
                address: a.clone(),
                addresses: vec![key(41), key(42)],
                last_extended_slot: 0,
                last_extended_slot_start_index: 2,
                deactivation_slot: None,
            },
            LookupTable {
                address: b.clone(),
                addresses: vec![key(43), key(44)],
                last_extended_slot: 0,
                last_extended_slot_start_index: 2,
                deactivation_slot: None,
            },
        ],
    };
    let mut server =
        Server::with_state(ClockProfile::default(), DiskLimits::default(), state).await;
    let (signature, wire) = signed_lookup(
        &server.hash().await,
        23,
        &[(&a, vec![1], vec![0]), (&b, vec![0], vec![1])],
    );
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    let tx = server
        .call(
            "getTransaction",
            json!([signature,{"encoding":"jsonParsed","maxSupportedTransactionVersion":0}]),
        )
        .await;
    let message = &tx["result"]["transaction"]["message"];
    let keys = message["accountKeys"].as_array().unwrap();
    assert_eq!(keys[2]["pubkey"], key(42));
    assert_eq!(keys[3]["pubkey"], key(43));
    assert_eq!(keys[4]["pubkey"], key(41));
    assert_eq!(keys[5]["pubkey"], key(44));
    assert_eq!(keys[2]["source"], "lookupTable");
    assert_eq!(keys[2]["writable"], true);
    assert_eq!(keys[4]["writable"], false);
    assert_eq!(
        message["instructions"][0]["parsed"]["info"]["destination"],
        key(42)
    );
    assert_eq!(message["instructions"][0]["parsed"]["info"]["lamports"], 42);
    assert_eq!(
        message["instructions"][1]["accounts"],
        json!([key(42), key(43)])
    );
    assert_eq!(
        message["instructions"][1]["data"],
        bs58::encode([9, 8]).into_string()
    );
    assert!(tx["result"]["meta"].is_null());
    assert!(server.stop(false).await);
    let row = server.record(&signature).await.unwrap();
    assert_eq!(row.loaded_addresses.writable, vec![key(42), key(43)]);
    assert_eq!(row.loaded_addresses.readonly, vec![key(41), key(44)]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lookup_warmup_bad_index_and_unknown_program_are_explicit() {
    use ronchi::protocol::state::{LookupTable, ProtocolState};
    let key = |n| bs58::encode([n; 32]).into_string();
    let table = key(33);
    let state = ProtocolState {
        nonce_accounts: vec![],
        lookup_tables: vec![LookupTable {
            address: table.clone(),
            addresses: vec![key(51), key(52)],
            last_extended_slot: 0,
            last_extended_slot_start_index: 1,
            deactivation_slot: None,
        }],
    };
    let mut server =
        Server::with_state(ClockProfile::default(), DiskLimits::default(), state).await;
    let hash = server.hash().await;
    let (signature, wire) = signed_lookup(&hash, 24, &[(&table, vec![1], vec![0])]);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["error"]["data"]["code"],
        "state_unavailable"
    );
    server.time.advance(Duration::from_millis(400)).unwrap();
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    let (_, invalid) = signed_lookup(&hash, 25, &[(&table, vec![2], vec![0])]);
    assert_eq!(
        server.call("sendTransaction", send(&invalid)).await["error"]["data"]["code"],
        "state_unavailable"
    );
    // Replace the system program with an unknown program and independently sign again.
    let signer = SigningKey::from_bytes(&[24; 32]);
    let mut message = wire[65..].to_vec();
    message[37..69].copy_from_slice(&[81; 32]);
    let sig = signer.sign(&message).to_bytes();
    let mut unknown = vec![1];
    unknown.extend_from_slice(&sig);
    unknown.extend(message);
    let identity = bs58::encode(sig).into_string();
    assert_eq!(
        server.call("sendTransaction", send(&unknown)).await["result"],
        identity
    );
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&identity, Phase::Finalized).await;
    let response = server
        .call(
            "getTransaction",
            json!([identity,{"encoding":"jsonParsed","maxSupportedTransactionVersion":0}]),
        )
        .await;
    let ix = &response["result"]["transaction"]["message"]["instructions"][0];
    assert_eq!(ix["programId"], key(81));
    assert!(ix.get("parsed").is_none());
    assert_eq!(ix["accounts"][1], key(52));
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fee_quotes_use_message_versions_and_selected_lifetime_context_over_http() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let hash = server.hash().await;
    for version in [-1, 0, 1] {
        let (_, wire) = if version == 1 {
            signed_v1(&hash, 26)
        } else {
            signed(&hash, 26, version == 0)
        };
        let message = if version == 1 {
            &wire[..wire.len() - 64]
        } else {
            &wire[65..]
        };
        let quote = server
            .call("getFeeForMessage", json!([STANDARD.encode(message)]))
            .await;
        assert_eq!(quote["result"]["value"], 5000);
        assert_eq!(quote["result"]["context"]["slot"], 0);
    }
    assert_eq!(
        server.call("getFeeForMessage", json!(["!"])).await["error"]["code"],
        -32602
    );
    let (_, wire) = signed(&hash, 26, false);
    let message = STANDARD.encode(&wire[65..]);
    assert_eq!(
        server
            .call("getFeeForMessage", json!([message,{"minContextSlot":1}]))
            .await["error"]["code"],
        -32016
    );
    assert_eq!(
        server
            .call("getFeeForMessage", json!([message,{"encoding":"base58"}]))
            .await["error"]["code"],
        -32602
    );
    server.time.advance(Duration::from_millis(60_400)).unwrap();
    assert!(
        server
            .call(
                "getFeeForMessage",
                json!([message,{"commitment":"processed"}])
            )
            .await["result"]["value"]
            .is_null()
    );
    assert_eq!(
        server
            .call(
                "getFeeForMessage",
                json!([message,{"commitment":"finalized"}])
            )
            .await["result"]["value"],
        5000
    );
    assert!(server.stop(false).await);
}

fn budget_message(hash: &str, instructions: Vec<Vec<u8>>, version: i16, priority: u64) -> Vec<u8> {
    let signer = SigningKey::from_bytes(&[27; 32]);
    let program = bs58::decode("ComputeBudget111111111111111111111111111111")
        .into_vec()
        .unwrap();
    let mut message = if version == 1 {
        vec![0x81, 1, 0, 1]
    } else if version == 0 {
        vec![0x80, 1, 0, 1, 2]
    } else {
        vec![1, 0, 1, 2]
    };
    if version == 1 {
        message.extend_from_slice(&3u32.to_le_bytes());
        message.extend_from_slice(&bs58::decode(hash).into_vec().unwrap());
        message.extend_from_slice(&[instructions.len() as u8, 2]);
    }
    message.extend_from_slice(&signer.verifying_key().to_bytes());
    message.extend_from_slice(&program);
    if version == 1 {
        message.extend_from_slice(&priority.to_le_bytes());
    } else {
        message.extend_from_slice(&bs58::decode(hash).into_vec().unwrap());
        message.push(instructions.len() as u8);
    }
    for data in instructions {
        message.extend_from_slice(&[1, 0]);
        if version == 1 {
            message.extend_from_slice(&(data.len() as u16).to_le_bytes());
        } else {
            message.push(data.len() as u8);
        }
        message.extend(data);
    }
    if version == 0 {
        message.push(0);
    }
    message
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fee_priority_rounding_duplicate_budget_and_v1_header_rules() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let hash = server.hash().await;
    let mut limit = vec![2];
    limit.extend_from_slice(&100_001u32.to_le_bytes());
    let mut price = vec![3];
    price.extend_from_slice(&11u64.to_le_bytes());
    for version in [-1, 0] {
        let message = budget_message(&hash, vec![limit.clone(), price.clone()], version, 0);
        assert_eq!(
            server
                .call("getFeeForMessage", json!([STANDARD.encode(message)]))
                .await["result"]["value"],
            5002
        );
        let duplicate = budget_message(&hash, vec![limit.clone(), limit.clone()], version, 0);
        assert_eq!(
            server
                .call("getFeeForMessage", json!([STANDARD.encode(duplicate)]))
                .await["error"]["code"],
            -32602
        );
    }
    let default = budget_message(&hash, vec![price.clone()], -1, 0);
    assert_eq!(
        server
            .call("getFeeForMessage", json!([STANDARD.encode(default)]))
            .await["result"]["value"],
        5001
    );
    let ignored = budget_message(&hash, vec![vec![255]], 1, 9);
    assert_eq!(
        server
            .call("getFeeForMessage", json!([STANDARD.encode(ignored)]))
            .await["result"]["value"],
        5009
    );
    let message = budget_message(&hash, vec![], 1, 123);
    assert_eq!(
        server
            .call("getFeeForMessage", json!([STANDARD.encode(message)]))
            .await["result"]["value"],
        5123
    );
    assert_eq!(
        server.call("getRecentPrioritizationFees", json!([])).await["error"]["data"]["code"],
        "unsupported_capability"
    );
    assert!(server.stop(false).await);
}

fn signed_nonce(hash: &str, seed: u8, account: &str, version: i16, salt: u8) -> (String, Vec<u8>) {
    let signer = SigningKey::from_bytes(&[seed; 32]);
    let keys = [
        signer.verifying_key().to_bytes().to_vec(),
        bs58::decode(account).into_vec().unwrap(),
        bs58::decode("SysvarRecentB1ockHashes11111111111111111111")
            .into_vec()
            .unwrap(),
        vec![0; 32],
    ];
    let mut message = if version == 1 {
        vec![0x81, 1, 0, 2]
    } else if version == 0 {
        vec![0x80, 1, 0, 2, 4]
    } else {
        vec![1, 0, 2, 4]
    };
    if version == 1 {
        message.extend_from_slice(&0u32.to_le_bytes());
        message.extend_from_slice(&bs58::decode(hash).into_vec().unwrap());
        message.extend_from_slice(&[2, 4]);
    }
    for key in keys {
        message.extend(key);
    }
    if version != 1 {
        message.extend_from_slice(&bs58::decode(hash).into_vec().unwrap());
        message.push(2);
    }
    if version == 1 {
        message.extend_from_slice(&[3, 3, 4, 0, 3, 0, 1, 0]); // both fixed headers precede payloads
        message.extend_from_slice(&[1, 2, 0]);
        message.extend_from_slice(&4u32.to_le_bytes());
        message.push(salt);
    } else {
        message.extend_from_slice(&[3, 3, 1, 2, 0, 4]);
        message.extend_from_slice(&4u32.to_le_bytes());
        message.extend_from_slice(&[3, 0, 1, salt]);
    }
    if version == 0 {
        message.push(0);
    }
    let signature = signer.sign(&message).to_bytes();
    let wire = if version == 1 {
        let mut v = message;
        v.extend_from_slice(&signature);
        v
    } else {
        let mut v = vec![1];
        v.extend_from_slice(&signature);
        v.extend(message);
        v
    };
    (bs58::encode(signature).into_string(), wire)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonce_requires_explicit_account_state_without_creating_it() {
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let nonce = bs58::encode([91; 32]).into_string();
    let account = bs58::encode([61; 32]).into_string();
    let (signature, wire) = signed_nonce(&nonce, 28, &account, -1, 0);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["error"]["data"]["code"],
        "state_unavailable"
    );
    assert!(server.record(&signature).await.is_none());
    assert!(server.stop(false).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonce_survives_recent_hash_age_and_advances_once_with_processing_for_all_versions() {
    use ronchi::protocol::{nonce::NonceAccount, state::ProtocolState};
    for version in [-1, 0, 1] {
        let account = bs58::encode([62; 32]).into_string();
        let nonce = bs58::encode([92; 32]).into_string();
        let authority = bs58::encode(SigningKey::from_bytes(&[29; 32]).verifying_key().to_bytes())
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
        server.time.advance(Duration::from_secs(120)).unwrap();
        let (signature, wire) = signed_nonce(&nonce, 29, &account, version, 1);
        let message = if version == 1 {
            &wire[..wire.len() - 64]
        } else {
            &wire[65..]
        };
        let encoded = STANDARD.encode(message);
        let quote = server
            .call(
                "getFeeForMessage",
                json!([encoded,{"commitment":"processed"}]),
            )
            .await;
        assert_eq!(quote["result"]["value"], 5000, "version {version}: {quote}");
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        let row = server.record(&signature).await.unwrap();
        assert!(row.expiry_at_ns.is_none());
        assert!(row.last_valid_height.is_none());
        server.time.advance(Duration::from_secs(2)).unwrap();
        server.phase(&signature, Phase::Processed).await;
        assert!(
            server
                .call(
                    "getFeeForMessage",
                    json!([encoded,{"commitment":"processed"}])
                )
                .await["result"]["value"]
                .is_null()
        );
        assert_eq!(
            server
                .call(
                    "getFeeForMessage",
                    json!([encoded,{"commitment":"finalized"}])
                )
                .await["result"]["value"],
            5000
        );
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        server.time.advance(Duration::from_secs(4)).unwrap();
        server.phase(&signature, Phase::Finalized).await;
        assert_eq!(
            server
                .call(
                    "getTransaction",
                    json!([signature,{"encoding":"jsonParsed","maxSupportedTransactionVersion":1}])
                )
                .await["result"]["transaction"]["message"]["instructions"][0]["parsed"]["type"],
            "advanceNonce"
        );
        let (stale, wire2) = signed_nonce(&nonce, 29, &account, version, 2);
        assert_eq!(
            server.call("sendTransaction", send(&wire2)).await["result"],
            stale
        );
        server.time.advance(Duration::from_secs(2)).unwrap();
        server.phase(&stale, Phase::Unprocessed).await;
        assert!(
            server.call("getSignatureStatuses", statuses(&stale)).await["result"]["value"][0]
                .is_null()
        );
        assert!(server.stop(false).await);
        let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
        let current = reader.nonce(&account, None).unwrap().unwrap();
        assert_ne!(current.nonce, nonce);
        assert_eq!(current.slot, 305);
        assert_eq!(
            reader.nonce(&account, Some(304)).unwrap().unwrap().nonce,
            nonce
        );
        assert_eq!(
            reader.transaction(&signature).unwrap().unwrap().wire_base64,
            STANDARD.encode(&wire)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn competing_nonce_identities_consume_one_value_and_reject_wrong_authority() {
    use ronchi::protocol::{nonce::NonceAccount, state::ProtocolState};
    let account = bs58::encode([63; 32]).into_string();
    let nonce = bs58::encode([93; 32]).into_string();
    let authority =
        bs58::encode(SigningKey::from_bytes(&[30; 32]).verifying_key().to_bytes()).into_string();
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
    let (_, wrong) = signed_nonce(&nonce, 31, &account, -1, 0);
    assert_eq!(
        server.call("sendTransaction", send(&wrong)).await["error"]["code"],
        -32602
    );
    let (a, wire_a) = signed_nonce(&nonce, 30, &account, -1, 1);
    let (b, wire_b) = signed_nonce(&nonce, 30, &account, -1, 2);
    let (first, second) = tokio::join!(
        server.call("sendTransaction", send(&wire_a)),
        server.call("sendTransaction", send(&wire_b))
    );
    assert_eq!(first["result"], a);
    assert_eq!(second["result"], b);
    server.time.advance(Duration::from_secs(6)).unwrap();
    // Both due decisions must have completed before this status response.
    let response = server.call("getSignatureStatuses", json!([[a, b]])).await;
    let values = response["result"]["value"].as_array().unwrap();
    assert_eq!(
        values
            .iter()
            .filter(|value| value["confirmationStatus"] == "finalized")
            .count(),
        1
    );
    assert_eq!(values.iter().filter(|value| value.is_null()).count(), 1);
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    let current = reader.nonce(&account, None).unwrap().unwrap();
    assert_ne!(current.nonce, nonce);
    let mut advances = 0;
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "transaction.transition" && event.data["nonce_advance"].is_object() {
            advances += 1;
        }
    }
    assert_eq!(advances, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_nonce_advancement_rolls_back_processing_and_preserves_acknowledged_identity() {
    use ronchi::protocol::{nonce::NonceAccount, state::ProtocolState};
    let account = bs58::encode([64; 32]).into_string();
    let nonce = bs58::encode([94; 32]).into_string();
    let authority =
        bs58::encode(SigningKey::from_bytes(&[32; 32]).verifying_key().to_bytes()).into_string();
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
    let (signature, wire) = signed_nonce(&nonce, 32, &account, -1, 0);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    let path = server.directory.path().join("journal.sqlite");
    tokio::task::spawn_blocking(move||{let c=rusqlite::Connection::open(path).unwrap();c.execute_batch("CREATE TRIGGER fail_nonce BEFORE INSERT ON nonce_versions WHEN NEW.slot>0 BEGIN SELECT RAISE(ABORT,'injected nonce failure'); END;").unwrap();}).await.unwrap();
    server.time.advance(Duration::from_secs(2)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while server.writer.as_ref().unwrap().handle().status().health != WriterHealth::Failed {
        assert!(Instant::now() < deadline);
        tokio::task::yield_now().await;
    }
    assert!(!server.stop(true).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert_eq!(reader.nonce(&account, None).unwrap().unwrap().nonce, nonce);
    let row = reader.transaction(&signature).unwrap().unwrap();
    assert_eq!(row.stage, Phase::Pending);
    assert!(row.processed_slot.is_none());
    assert_eq!(row.wire_base64, STANDARD.encode(wire));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonce_same_bank_cannot_advance_again_and_lookup_loaded_nonce_can_process() {
    use ronchi::protocol::{
        nonce::NonceAccount,
        state::{LookupTable, ProtocolState},
    };
    use sha2::{Digest, Sha256};
    let account = bs58::encode([65; 32]).into_string();
    let table = bs58::encode([66; 32]).into_string();
    let signer = SigningKey::from_bytes(&[33; 32]);
    let authority = bs58::encode(signer.verifying_key().to_bytes()).into_string();
    let (clock, _) = Clock::controlled(
        ClockProfile {
            slot_duration_ms: 60_000,
            ..Default::default()
        },
        b"transaction-test-network",
    )
    .unwrap();
    let hash = bs58::decode(clock.blockhash(0)).into_vec().unwrap();
    let nonce = bs58::encode(Sha256::digest(
        [b"DURABLE_NONCE".as_slice(), &hash].concat(),
    ))
    .into_string();
    let state = ProtocolState {
        lookup_tables: vec![],
        nonce_accounts: vec![NonceAccount {
            address: account.clone(),
            authority: authority.clone(),
            nonce: nonce.clone(),
            slot: 0,
        }],
    };
    let mut server = Server::with_state(clock.profile, DiskLimits::default(), state).await;
    let (signature, wire) = signed_nonce(&nonce, 33, &account, 1, 0);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    server.time.advance(Duration::from_secs(2)).unwrap();
    server.phase(&signature, Phase::Unprocessed).await;
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert_eq!(reader.nonce(&account, None).unwrap().unwrap().nonce, nonce);
    // v0 nonce account can be loaded writable; its authority must remain a static signer.
    let nonce = bs58::encode([95; 32]).into_string();
    let state = ProtocolState {
        lookup_tables: vec![LookupTable {
            address: table.clone(),
            addresses: vec![account.clone()],
            last_extended_slot: 0,
            last_extended_slot_start_index: 1,
            deactivation_slot: None,
        }],
        nonce_accounts: vec![NonceAccount {
            address: account.clone(),
            authority,
            nonce: nonce.clone(),
            slot: 0,
        }],
    };
    let mut server =
        Server::with_state(ClockProfile::default(), DiskLimits::default(), state).await;
    let mut message = vec![0x80, 1, 0, 2, 3];
    message.extend_from_slice(&signer.verifying_key().to_bytes());
    message.extend(
        bs58::decode("SysvarRecentB1ockHashes11111111111111111111")
            .into_vec()
            .unwrap(),
    );
    message.extend_from_slice(&[0; 32]);
    message.extend(bs58::decode(&nonce).into_vec().unwrap());
    message.extend_from_slice(&[1, 2, 3, 3, 1, 0, 4]);
    message.extend_from_slice(&4u32.to_le_bytes());
    message.push(1);
    message.extend(bs58::decode(table).into_vec().unwrap());
    message.extend_from_slice(&[1, 0, 0]);
    let signature = signer.sign(&message).to_bytes();
    let identity = bs58::encode(signature).into_string();
    let mut wire = vec![1];
    wire.extend_from_slice(&signature);
    wire.extend(message);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        identity
    );
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&identity, Phase::Finalized).await;
    assert!(server.stop(false).await);
    let row = server.record(&identity).await.unwrap();
    assert_eq!(row.nonce_context.unwrap().address, account);
}

fn packet_fixture(hash: &str, version: i16, extra: usize) -> (String, Vec<u8>) {
    let signer = SigningKey::from_bytes(&[34; 32]);
    let (maximum, overhead) = match version {
        -1 => (1232, 170),
        0 => (1232, 172),
        _ => (4096, 174),
    };
    let size = maximum - overhead + extra;
    let mut message = if version == 1 {
        vec![0x81, 1, 0, 1]
    } else if version == 0 {
        vec![0x80, 1, 0, 1, 2]
    } else {
        vec![1, 0, 1, 2]
    };
    if version == 1 {
        message.extend_from_slice(&0u32.to_le_bytes());
        message.extend(bs58::decode(hash).into_vec().unwrap());
        message.extend_from_slice(&[1, 2]);
    }
    message.extend_from_slice(&signer.verifying_key().to_bytes());
    message.extend_from_slice(&[81; 32]);
    if version != 1 {
        message.extend(bs58::decode(hash).into_vec().unwrap());
        message.push(1);
    }
    message.extend_from_slice(&[1, 0]);
    if version == 1 {
        message.extend_from_slice(&(size as u16).to_le_bytes());
    } else {
        message.extend_from_slice(&[((size & 127) as u8) | 128, (size >> 7) as u8]);
    }
    message.resize(message.len() + size, 7);
    if version == 0 {
        message.push(0);
    }
    let sig = signer.sign(&message).to_bytes();
    let wire = if version == 1 {
        let mut v = message;
        v.extend_from_slice(&sig);
        v
    } else {
        let mut v = vec![1];
        v.extend_from_slice(&sig);
        v.extend(message);
        v
    };
    assert_eq!(wire.len(), maximum + extra);
    (bs58::encode(sig).into_string(), wire)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exact_packet_bounds_for_all_formats_preserve_binary_and_reject_one_extra_byte() {
    for version in [-1, 0, 1] {
        let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
        let hash = server.hash().await;
        let (signature, wire) = packet_fixture(&hash, version, 0);
        assert_eq!(
            server.call("sendTransaction", send(&wire)).await["result"],
            signature
        );
        assert_eq!(
            server
                .call(
                    "sendTransaction",
                    json!([bs58::encode(&wire).into_string(),{"skipPreflight":true}])
                )
                .await["result"],
            signature
        );
        let (_, oversized) = packet_fixture(&hash, version, 1);
        assert_eq!(
            server.call("sendTransaction", send(&oversized)).await["error"]["code"],
            -32602
        );
        server.time.advance(Duration::from_secs(6)).unwrap();
        server.phase(&signature, Phase::Finalized).await;
        assert_eq!(
            server
                .call(
                    "getTransaction",
                    json!([signature,{"encoding":"base64","maxSupportedTransactionVersion":1}])
                )
                .await["result"]["transaction"][0],
            STANDARD.encode(&wire)
        );
        assert!(server.stop(false).await);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn response_loss_silence_retains_identity_and_autonomous_status() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    server.rpc.wait_initialized().await.unwrap();
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            hold_ms: 300,
            ..Default::default()
        })
        .await
        .unwrap();
    let (signature, wire) = signed(&server.hash().await, 40, false);
    let body =
        json!({"jsonrpc":"2.0","id":1,"method":"sendTransaction","params":send(&wire)}).to_string();
    let mut stream = tokio::net::TcpStream::connect(server.address)
        .await
        .unwrap();
    stream
        .write_all(
            format!(
                "POST / HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\n\r\n{}",
                server.address,
                body.len(),
                body
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    server.phase(&signature, Phase::Pending).await;
    let mut bytes = [0; 1];
    assert!(
        tokio::time::timeout(Duration::from_millis(40), stream.read(&mut bytes))
            .await
            .is_err()
    );
    server.time.advance(Duration::from_secs(6)).unwrap();
    server.phase(&signature, Phase::Finalized).await;
    assert_eq!(
        server
            .call("getSignatureStatuses", statuses(&signature))
            .await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), stream.read(&mut bytes))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    assert!(server.stop(false).await);
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    assert!(reader.response_loss(&signature).unwrap().is_some());
    let mut started = 0;
    let mut ended = 0;
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "fault.withholding_started" {
            started += 1;
        }
        if event.kind == "rpc.boundary" && event.data["transport"] == "withholding_deadline" {
            ended += 1;
            assert_eq!(event.data["client_receipt"], "unknown");
        }
    }
    assert_eq!((started, ended), (1, 1));
}

#[path = "cases/response_loss.rs"]
mod response_loss_cases;

#[tokio::test]
async fn investigation_correlates_http_signature_lookups_and_exposes_signed_bytes_only_explicitly()
{
    use ronchi::query::{Arguments, Engine as QueryEngine, Operation, Request, Scope};
    let mut server = Server::new(ClockProfile::default(), DiskLimits::default()).await;
    let hash = server.hash().await;
    let (signature, wire) = signed(&hash, 81, false);
    assert_eq!(
        server.call("sendTransaction", send(&wire)).await["result"],
        signature
    );
    server
        .call("getSignatureStatuses", json!([[signature]]))
        .await;
    let engine = QueryEngine::default();
    let path = server.directory.path().join("journal.sqlite");
    let req = Request {
        contract_version: 1,
        request_id: "q-http".into(),
        lab_id: "L-transactions".into(),
        operation: Operation::Timeline,
        arguments: Arguments {
            scope: Scope {
                signature: vec![signature.clone()],
                rpc_method: vec!["getSignatureStatuses".into()],
                ..Default::default()
            },
            ..Default::default()
        },
    };
    let response = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            req,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let items = response.result["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i["kind"] == "rpc.registered"));
    assert!(
        items
            .iter()
            .all(|i| i["rpc_method"] == "getSignatureStatuses")
    );
    assert!(
        !serde_json::to_string(&response)
            .unwrap()
            .contains(&STANDARD.encode(&wire))
    );
    let req = Request {
        contract_version: 1,
        request_id: "q-wire".into(),
        lab_id: "L-transactions".into(),
        operation: Operation::Evidence,
        arguments: Arguments {
            scope: Scope {
                signature: vec![signature],
                kind: vec!["transaction.accepted".into()],
                ..Default::default()
            },
            include_payload: true,
            ..Default::default()
        },
    };
    let response = engine
        .execute(path, true, "explicit", req, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        response.result["items"][0]["payload"]["preview"],
        STANDARD.encode(&wire)
    );
    assert_eq!(
        response.result["items"][0]["payload"]["payload_truncated"],
        false
    );
    server.time.advance(Duration::from_secs(6)).unwrap();
    server
        .phase(
            response.result["items"][0]["transaction_signature"]
                .as_str()
                .unwrap(),
            Phase::Finalized,
        )
        .await;
    assert!(server.stop(false).await);
}
