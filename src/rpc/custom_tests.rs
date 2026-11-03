//! Controlled source histories test Custom primitive contracts, not native Surfpool consensus.
use super::*;
use crate::journal::{JournalReader, JournalStore, JournalWriter};
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Source {
    height: AtomicU64,
    close_status: AtomicBool,
    status_probes: AtomicU64,
    signature: String,
    wire: String,
}
impl Source {
    fn new() -> Self {
        let signer = SigningKey::from_bytes(&[149; 32]);
        let mut message = vec![1, 0, 0, 1];
        message.extend(signer.verifying_key().to_bytes());
        message.extend([7; 32]);
        message.push(0);
        let signed = signer.sign(&message);
        let mut wire = vec![1];
        wire.extend(signed.to_bytes());
        wire.extend(message);
        Self {
            height: AtomicU64::new(0),
            close_status: AtomicBool::new(false),
            status_probes: AtomicU64::new(0),
            signature: bs58::encode(signed.to_bytes()).into_string(),
            wire: base64::engine::general_purpose::STANDARD.encode(wire),
        }
    }
    async fn serve(self: Arc<Self>, mut stream: tokio::net::TcpStream) {
        let mut bytes = Vec::new();
        let request = loop {
            let mut chunk = [0; 4096];
            let n = stream.read(&mut chunk).await.unwrap_or(0);
            if n == 0 {
                return;
            }
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() <= 128 * 1024);
            let Some(header_end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") else {
                continue;
            };
            let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (k, v) = line.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() < header_end + 4 + length {
                continue;
            }
            break serde_json::from_slice::<Value>(&bytes[header_end + 4..header_end + 4 + length])
                .unwrap();
        };
        let method = request["method"].as_str().unwrap();
        if method == "getSignatureStatuses" {
            self.status_probes.fetch_add(1, Ordering::SeqCst);
            if self.close_status.load(Ordering::SeqCst) {
                let _ = stream.shutdown().await;
                return;
            }
        }
        let height = self.height.load(Ordering::SeqCst);
        let result = match method {
            "getBlockHeight" => json!(height),
            "getLatestBlockhash" => {
                let marker = if request["params"][0]["commitment"] == "processed" {
                    10
                } else {
                    20
                };
                json!({"context":{"slot":height},"value":{"blockhash":bs58::encode([marker+height as u8;32]).into_string(),"lastValidBlockHeight":height+2}})
            }
            "sendTransaction" => json!(self.signature),
            "getTransaction" => {
                json!({"slot":7,"blockTime":null,"meta":{"err":null},"transaction":[self.wire,"base64"]})
            }
            "getSignatureStatuses" => {
                json!({"context":{"slot":7},"value":[{"slot":7,"confirmations":null,"err":null,"status":{"Ok":null},"confirmationStatus":"finalized"}]})
            }
            _ => Value::Null,
        };
        let body = json!({"jsonrpc":"2.0","id":request["id"],"result":result}).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
    }
}
struct Recorded {
    events: Vec<crate::journal::Event>,
    dir: tempfile::TempDir,
}
impl std::ops::Deref for Recorded {
    type Target = [crate::journal::Event];
    fn deref(&self) -> &Self::Target {
        &self.events
    }
}
impl Drop for Recorded {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.dir.disable_cleanup(true);
            eprintln!(
                "retained custom controlled fixture {}",
                self.dir.path().display()
            );
        }
    }
}
struct Fixture {
    rpc: Rpc,
    address: std::net::SocketAddr,
    state: Arc<Source>,
    source_task: tokio::task::JoinHandle<()>,
    rpc_task: tokio::task::JoinHandle<std::io::Result<()>>,
    stop: CancellationToken,
    source_stop: CancellationToken,
    writer: JournalWriter,
    dir: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let state = Arc::new(Source::new());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend = crate::backend::Client::new(
            listener.local_addr().unwrap(),
            "surfpool:custom-controlled".into(),
            None,
        )
        .unwrap();
        let source_stop = CancellationToken::new();
        let stop = source_stop.clone();
        let state_clone = state.clone();
        let source_task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                tokio::select! {_=stop.cancelled()=>break,result=listener.accept()=>{
                    let(stream,_)=result.unwrap();
                    connections.spawn(state_clone.clone().serve(stream));
                },Some(result)=connections.join_next(),if !connections.is_empty()=>{result.unwrap();}}
            }
            connections.abort_all();
            while connections.join_next().await.is_some() {}
        });
        let dir = tempfile::tempdir().unwrap();
        let writer = JournalWriter::start(
            JournalStore::create(&dir.path().join("journal.sqlite"), "L-custom-controlled")
                .unwrap(),
        )
        .unwrap();
        let (clock, _) = Clock::controlled(Default::default(), b"custom-controlled").unwrap();
        let rpc = Rpc::with_backend(clock, writer.handle(), Instant::now(), backend).unwrap();
        rpc.set_ready(true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let stop = CancellationToken::new();
        let rpc_task = tokio::spawn(rpc.clone().serve(
            listener,
            stop.clone(),
            CancellationToken::new(),
        ));
        rpc.wait_initialized().await.unwrap();
        Self {
            rpc,
            address,
            state,
            source_task,
            rpc_task,
            stop,
            source_stop,
            writer,
            dir,
        }
    }
    async fn call(&self, method: &str, params: Value) -> Value {
        let client: hyper_util::client::legacy::Client<
            hyper_util::client::legacy::connect::HttpConnector,
            Full<Bytes>,
        > = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .pool_max_idle_per_host(0)
            .build_http();
        let request = hyper::Request::builder()
            .method("POST")
            .uri(format!("http://{}/", self.address))
            .body(Full::new(Bytes::from(
                json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string(),
            )))
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), client.request(request))
            .await
            .unwrap()
            .unwrap();
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }
    async fn arm(&self, primitive: &str, key: &str, methods: Value, arguments: Value) {
        let yaml=json!({"schema":"ronchi.case/v1","id":"custom.controlled","revision":"1","requires":{"primitives":[primitive]},"modes":["surfpool"],"filter":{"methods":methods,"transaction_origin":"known_local"},"state":{"key":key,"scope":"activation","max_keys":8,"on_exhausted":"error"},"sequence":[{"id":"fault","using":primitive,"with":arguments,"until":{"matches":100},"next":"done"}]}).to_string();
        self.rpc
            .arm_case(crate::cases::Arm {
                case_id: "custom".into(),
                custom_yaml: Some(yaml),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    async fn finish(self) -> Recorded {
        self.stop.cancel();
        self.rpc_task.await.unwrap().unwrap();
        self.source_stop.cancel();
        self.source_task.await.unwrap();
        tokio::task::spawn_blocking(move || self.writer.shutdown(None))
            .await
            .unwrap()
            .unwrap();
        let reader = JournalReader::open(&self.dir.path().join("journal.sqlite")).unwrap();
        let events = (1..=reader.snapshot(false).unwrap().snapshot_seq)
            .map(|seq| reader.event(seq).unwrap().unwrap())
            .collect();
        drop(reader);
        Recorded {
            events,
            dir: self.dir,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_blockhash_keeps_commitments_separate_and_uses_configured_expiry_window() {
    let fixture = Fixture::new().await;
    fixture
        .arm(
            "rpc.blockhash-reuse@1",
            "case",
            json!(["getLatestBlockhash"]),
            json!({"expiry_window_real_ms":100}),
        )
        .await;
    let processed = json!([{"commitment":"processed"}]);
    let finalized = json!([{"commitment":"finalized"}]);
    let p = fixture.call("getLatestBlockhash", processed.clone()).await["result"].clone();
    let f = fixture.call("getLatestBlockhash", finalized.clone()).await["result"].clone();
    assert_ne!(p["value"]["blockhash"], f["value"]["blockhash"]);
    fixture.state.height.store(3, Ordering::SeqCst);
    assert_eq!(
        fixture.call("getLatestBlockhash", processed.clone()).await["result"],
        p
    );
    assert_eq!(
        fixture.call("getLatestBlockhash", finalized.clone()).await["result"],
        f
    );
    // Both responses above committed actual same-source height > lastValidBlockHeight.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let fresh_p = fixture.call("getLatestBlockhash", processed).await;
    let fresh_f = fixture.call("getLatestBlockhash", finalized).await;
    assert!(fresh_p["result"].is_object(), "{fresh_p}");
    assert!(fresh_f["result"].is_object(), "{fresh_f}");
    assert_ne!(fresh_p["result"], p);
    assert_ne!(fresh_f["result"], f);
    let events = fixture.finish().await;
    let expired: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "fault.blockhash_expired")
        .collect();
    assert_eq!(expired.len(), 2);
    for e in expired {
        assert_eq!(e.data["source_identity"], "surfpool:custom-controlled");
        assert_eq!(e.data["observed_block_height"], 3);
    }
    let elapsed: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "fault.blockhash_window_elapsed")
        .collect();
    assert_eq!(elapsed.len(), 2);
    for event in elapsed {
        assert_eq!(
            event.data["window_end_elapsed_ns"].as_u64().unwrap()
                - event.data["expiry_observed_elapsed_ns"].as_u64().unwrap(),
            100_000_000
        );
        assert_eq!(event.data["source_identity"], "surfpool:custom-controlled");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_snapshot_probe_transport_failure_is_bounded_and_does_not_invent_a_lifecycle() {
    let fixture = Fixture::new().await;
    assert_eq!(
        fixture
            .call(
                "sendTransaction",
                json!([fixture.state.wire,{"encoding":"base64"}])
            )
            .await["result"],
        fixture.state.signature
    );
    fixture
        .arm(
            "rpc.lookup-snapshot@1",
            "transaction_signature",
            json!(["getTransaction", "getSignatureStatuses"]),
            json!({}),
        )
        .await;
    fixture.state.close_status.store(true, Ordering::SeqCst);
    let params = json!([fixture.state.signature,{"commitment":"confirmed","encoding":"base64"}]);
    let started = Instant::now();
    let failed_probe = fixture.call("getTransaction", params.clone()).await;
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(
        failed_probe["result"].is_object(),
        "source transaction remains authoritative: {failed_probe}"
    );
    assert_eq!(fixture.state.status_probes.load(Ordering::SeqCst), 1);
    fixture.state.close_status.store(false, Ordering::SeqCst);
    assert_eq!(
        fixture.call("getTransaction", params).await["result"],
        failed_probe["result"]
    );
    assert_eq!(
        fixture
            .call("getSignatureStatuses", json!([[fixture.state.signature]]))
            .await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    assert_eq!(
        fixture.writer.handle().status().health,
        crate::journal::WriterHealth::Healthy
    );
    let events = fixture.finish().await;
    let probes: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "rpc.custom_lifecycle_source")
        .collect();
    assert!(probes.len() >= 2);
    let unavailable = probes
        .iter()
        .find(|e| e.data["status_view"].is_null())
        .expect("failed source probe must remain visibly unavailable");
    assert_eq!(
        unavailable.data["error_classification"],
        "backend_transport"
    );
    assert_eq!(unavailable.data["source_outcome"], "source_failure");
    assert_eq!(
        unavailable.data["error_code"], -32000,
        "transport failure must remain distinct from a successful null status: {}",
        unavailable.data
    );
    assert!(
        probes
            .iter()
            .any(|e| e.data["status_view"]["confirmationStatus"] == "finalized")
    );
    assert!(!events.iter().any(|e| e.kind == "transaction.transition"));
}
