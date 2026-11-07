use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use ronchi::{
    journal::{JournalReader, JournalStore, JournalWriter},
    rpc::Rpc,
    simulator::clock::{Clock, ClockProfile},
};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
struct Server {
    rpc: Rpc,
    address: SocketAddr,
    stop: CancellationToken,
    force: CancellationToken,
    task: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    writer: Option<JournalWriter>,
    dir: tempfile::TempDir,
}
impl Server {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let writer = JournalWriter::start(
            JournalStore::create(&dir.path().join("journal.sqlite"), "L-timeout").unwrap(),
        )
        .unwrap();
        let rpc = Rpc::new(
            Clock::realtime(ClockProfile::default(), b"timeout-test").unwrap(),
            writer.handle(),
            Instant::now(),
        );
        rpc.set_ready(true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let stop = CancellationToken::new();
        let force = CancellationToken::new();
        let task = tokio::spawn(rpc.clone().serve(listener, stop.clone(), force.clone()));
        rpc.wait_initialized().await.unwrap();
        Self {
            rpc,
            address,
            stop,
            force,
            task: Some(task),
            writer: Some(writer),
            dir,
        }
    }
    async fn call(&self, method: &str, params: Value) -> Value {
        let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
            .pool_max_idle_per_host(0)
            .build_http();
        let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();
        let request = hyper::Request::builder()
            .method("POST")
            .uri(format!("http://{}/", self.address))
            .body(Full::new(Bytes::from(body)))
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), client.request(request))
            .await
            .unwrap()
            .unwrap();
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }
    async fn raw(&self, value: Value) -> tokio::net::TcpStream {
        let mut stream = tokio::net::TcpStream::connect(self.address).await.unwrap();
        let body = value.to_string();
        stream
            .write_all(
                format!(
                    "POST / HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\n\r\n{body}",
                    self.address,
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        stream
    }
    async fn events(&self) -> Vec<ronchi::journal::Event> {
        let path = self.dir.path().join("journal.sqlite");
        tokio::task::spawn_blocking(move || {
            let reader = JournalReader::open(&path).unwrap();
            (1..=reader.snapshot(true).unwrap().snapshot_seq)
                .map(|seq| reader.event(seq).unwrap().unwrap())
                .collect()
        })
        .await
        .unwrap()
    }
    async fn settle(&mut self) {
        self.stop.cancel();
        self.force.cancel();
        self.task.take().unwrap().await.unwrap().unwrap();
        let writer = self.writer.take().unwrap();
        tokio::task::spawn_blocking(move || writer.shutdown(None))
            .await
            .unwrap()
            .unwrap();
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
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outage_holds_without_dispatch_and_disarm_preserves_the_original_deadline() {
    let mut server = Server::new().await;
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            case_id: "provider-timeout-burst".into(),
            hold_ms: 150,
            ..Default::default()
        })
        .await
        .unwrap();
    let mut held = server
        .raw(json!({"jsonrpc":"2.0","id":9,"method":"getLatestBlockhash","params":[]}))
        .await;
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.rpc.held_effects().is_empty() {
        assert!(
            Instant::now() < deadline,
            "events {:?}",
            server.events().await
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    server.rpc.disarm_case().await.unwrap();
    assert!(
        server
            .call("getLatestBlockhash", json!([]))
            .await
            .get("result")
            .is_some()
    );
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), held.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert!(bytes.is_empty());
    let events = server.events().await;
    assert!(events.iter().any(|e| e.kind == "rpc.dispatch_prevented"));
    server.settle().await;
}
fn arm_timeout(
    hold_ms: u64,
    methods: Vec<&str>,
    outage_ms: u64,
    recovery_ms: u64,
) -> ronchi::cases::Arm {
    serde_json::from_value(json!({"case_id":"provider-timeout-burst","hold_ms":hold_ms,"timeout":{"methods":methods,"outage_ms":outage_ms,"recovery_ms":recovery_ms}})).unwrap()
}
#[test]
fn exact_cycle_boundaries_follow_real_registration_coordinates() {
    let settings = ronchi::cases::timeout::Settings::default();
    for (ns, cycle, position, outage) in [
        (0, 0, 0, true),
        (4_999_999_999, 0, 4_999_999_999, true),
        (5_000_000_000, 0, 5_000_000_000, false),
        (14_999_999_999, 0, 14_999_999_999, false),
        (15_000_000_000, 1, 0, true),
        (20_000_000_000, 1, 5_000_000_000, false),
    ] {
        assert_eq!(settings.position(ns), (cycle, position, outage));
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_unavailable_and_notification_calls_do_not_consume_timeout_capacity() {
    let mut server = Server::new().await;
    server
        .rpc
        .arm_case(arm_timeout(
            200,
            vec!["getLatestBlockhash", "getAccountInfo", "sendTransaction"],
            5000,
            10000,
        ))
        .await
        .unwrap();
    assert_eq!(
        server
            .call("getLatestBlockhash", json!([{"unknown":true}]))
            .await["error"]["code"],
        -32602
    );
    assert_eq!(
        server
            .call("sendTransaction", json!(["bad",{"skipPreflight":true}]))
            .await["error"]["code"],
        -32602
    );
    let account = bs58::encode([1u8; 32]).into_string();
    assert_eq!(
        server.call("getAccountInfo", json!([account])).await["error"]["data"]["code"],
        "unsupported_capability"
    );
    assert!(server.rpc.held_effects().is_empty());
    let mut stream=server.raw(json!([{"jsonrpc":"2.0","method":"getLatestBlockhash","params":[]},{"jsonrpc":"2.0","id":1,"method":"getSlot","params":[]}])).await;
    let mut bytes = [0; 4096];
    let read = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes[..read]).contains("result"));
    assert!(
        !server
            .events()
            .await
            .iter()
            .any(|e| e.kind == "rpc.dispatch_prevented")
    );
    drop(stream);
    server.settle().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delayed_batch_keeps_registration_cycle_and_old_policy_after_replacement() {
    let mut server = Server::new().await;
    // The test delays dispatch across a cycle boundary. Its registration window
    // must also allow the durable Case activation to finish on a busy CI disk.
    let outage_ms = 1000;
    let old = server
        .rpc
        .arm_case(arm_timeout(2000, vec![], outage_ms, 10000))
        .await
        .unwrap();
    let pause = server.rpc.pause_next_registration();
    let mut stream=server.raw(json!([{"jsonrpc":"2.0","id":1,"method":"getLatestBlockhash","params":[]},{"jsonrpc":"2.0","id":2,"method":"getSlot","params":[]}])).await;
    pause.registered().await;
    let registration = server
        .events()
        .await
        .into_iter()
        .find(|event| {
            event.kind == "rpc.registered" && event.data["method"] == "getLatestBlockhash"
        })
        .unwrap();
    let registered_elapsed = registration.data["registered_elapsed_ns"].as_u64().unwrap();
    let activated_elapsed = old.policy.as_ref().unwrap().activated_elapsed_ns;
    assert!(
        registered_elapsed - activated_elapsed < outage_ms * 1_000_000,
        "fixture registration must occur during the initial outage"
    );
    // The pause has observed registration; crossing one full outage interval
    // now guarantees that dispatch happens in recovery, regardless of I/O time.
    tokio::time::sleep(Duration::from_millis(outage_ms + 30)).await;
    server.rpc.disarm_case().await.unwrap();
    pause.resume();
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert!(bytes.is_empty());
    let done = Instant::now() + Duration::from_secs(2);
    while !server.rpc.held_effects().is_empty() {
        assert!(Instant::now() < done);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let events = loop {
        let events = server.events().await;
        if events.iter().filter(|e| e.kind == "rpc.boundary").count() == 2 {
            break events;
        }
        assert!(Instant::now() < done);
        tokio::time::sleep(Duration::from_millis(2)).await;
    };
    let prevented = events
        .iter()
        .find(|e| e.kind == "rpc.dispatch_prevented")
        .unwrap();
    assert_eq!(
        prevented.data["policy"]["revision"],
        old.policy.unwrap().revision
    );
    assert!(prevented.data["selection"]["position_ns"].as_u64().unwrap() < outage_ms * 1_000_000);
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == "rpc.response_prepared")
            .count(),
        1
    );
    let boundaries: Vec<_> = events.iter().filter(|e| e.kind == "rpc.boundary").collect();
    assert_eq!(boundaries.len(), 2);
    assert!(
        boundaries
            .iter()
            .all(|e| e.data["rpc_response"] == "no_transport_response")
    );
    server.settle().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_requests_share_one_lowered_hold_slot_and_disconnect_releases_it() {
    let mut server = Server::new().await;
    server
        .rpc
        .arm_case(ronchi::cases::Arm {
            max_held: 1,
            ..arm_timeout(1000, vec![], 5000, 10000)
        })
        .await
        .unwrap();
    let address = server.address;
    let replies=tokio::task::spawn_blocking(move||{
  use std::io::{Read,Write};
  let gate=std::sync::Arc::new(std::sync::Barrier::new(16));let mut threads=vec![];
  for i in 0..16 {let gate=gate.clone();threads.push(std::thread::spawn(move||{let mut s=std::net::TcpStream::connect(address).unwrap();s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();let b=json!({"jsonrpc":"2.0","id":i,"method":"getLatestBlockhash","params":[]}).to_string();gate.wait();write!(s,"POST / HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{b}",b.len()).unwrap();let mut response=String::new();s.read_to_string(&mut response).unwrap();response}));}
  threads.into_iter().map(|t|t.join().unwrap()).collect::<Vec<_>>()
 }).await.unwrap();
    assert_eq!(replies.iter().filter(|s| s.is_empty()).count(), 1);
    assert!(
        replies
            .iter()
            .filter(|s| !s.is_empty())
            .all(|s| s.contains("case_capacity_exceeded"))
    );
    assert_eq!(
        server
            .events()
            .await
            .iter()
            .filter(|e| e.kind == "rpc.dispatch_prevented")
            .count(),
        1
    );
    let stream = server
        .raw(json!({"jsonrpc":"2.0","id":100,"method":"getLatestBlockhash","params":[]}))
        .await;
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.rpc.held_effects().is_empty() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    drop(stream);
    while !server.rpc.held_effects().is_empty() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(server.call("getHealth", json!([])).await["result"], "ok");
    server.settle().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn factual_timeout_finding_preserves_scope_support_and_exact_drilldown() {
    use ronchi::query::{Arguments, Engine, Operation, Request};
    let mut server = Server::new().await;
    server
        .rpc
        .arm_case(arm_timeout(80, vec![], 5000, 10000))
        .await
        .unwrap();
    let mut stream = server
        .raw(json!({"jsonrpc":"2.0","id":1,"method":"getLatestBlockhash","params":[]}))
        .await;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    assert!(bytes.is_empty());
    server.rpc.disarm_case().await.unwrap();
    server.call("getLatestBlockhash", json!([])).await;
    server.settle().await;
    let path = server.dir.path().join("journal.sqlite");
    let engine = Engine::default();
    let request = Request {
        contract_version: 1,
        request_id: "timeout-findings".into(),
        lab_id: "L-timeout".into(),
        operation: Operation::Findings,
        arguments: Arguments::default(),
    };
    let result = engine
        .execute(
            path.clone(),
            false,
            "explicit",
            request.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let finding = result.result["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["rule_id"] == "provider-timeout-burst.later-rpc-attempt")
        .unwrap_or_else(|| panic!("{result:?}"));
    assert!(finding["scope"].get("accepted_request_id").is_none());
    assert!(finding["scope"].get("signature").is_none());
    assert_eq!(finding["scope"]["rpc_method"], "getLatestBlockhash");
    assert!(
        !finding["statement"]
            .as_str()
            .unwrap()
            .contains("client timeout")
    );
    let arguments: Arguments =
        serde_json::from_value(finding["evidence_arguments"].clone()).unwrap();
    let evidence = engine
        .execute(
            path.clone(),
            false,
            "explicit",
            Request {
                operation: Operation::Evidence,
                arguments,
                ..request.clone()
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(evidence.result["items"].as_array().unwrap().len(), 4);
    let mut narrow = request;
    narrow.arguments.scope.kind = vec!["rpc.dispatch_prevented".into()];
    let result = engine
        .execute(path, false, "explicit", narrow, CancellationToken::new())
        .await
        .unwrap();
    assert!(result.result["items"].as_array().unwrap().is_empty());
}
#[test]
fn legacy_finding_scope_roundtrip_and_request_scope_preserve_their_wire_shapes() {
    use ronchi::findings::FindingScope;
    let original = r#"{"signature":"A","accepted_request_id":"original","from_seq":1,"to_seq":8}"#;
    let scope: FindingScope = serde_json::from_str(original).unwrap();
    let wire = serde_json::to_string(&scope).unwrap();
    assert_eq!(wire, original);
    let current = FindingScope::RpcRequest {
        request_id: "r1:0".into(),
        rpc_method: "getLatestBlockhash".into(),
        signature: None,
        from_seq: 1,
        to_seq: 8,
    };
    let wire = serde_json::to_string(&current).unwrap();
    assert!(!wire.contains("accepted_request_id"));
    assert!(!wire.contains("signature"));
    assert_eq!(current, serde_json::from_str(&wire).unwrap());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_through_timeout_preserves_firewall_then_recovers_without_source_dispatch_during_outage()
 {
    use hyper::service::service_fn;
    use hyper_util::rt::TokioIo;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let source = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source_address = source.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let received = calls.clone();
    let source_task = tokio::spawn(async move {
        let (stream, _) = source.accept().await.unwrap();
        hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream),service_fn(move|request:hyper::Request<hyper::body::Incoming>|{let received=received.clone();async move {let body=request.into_body().collect().await.unwrap().to_bytes();let request:Value=serde_json::from_slice(&body).unwrap();assert_eq!(request["method"],"getBalance");received.fetch_add(1,Ordering::SeqCst);Ok::<_,std::convert::Infallible>(hyper::Response::new(Full::new(Bytes::from(json!({"jsonrpc":"2.0","id":request["id"],"result":{"context":{"slot":8},"value":123}}).to_string()))))}})).await.unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let writer = JournalWriter::start(
        JournalStore::create(&dir.path().join("journal.sqlite"), "L-timeout").unwrap(),
    )
    .unwrap();
    let rpc = Rpc::with_upstream_state(
        Clock::realtime(ClockProfile::default(), b"timeout-upstream").unwrap(),
        writer.handle(),
        Instant::now(),
        Default::default(),
        Some(&format!("http://{source_address}")),
    )
    .unwrap();
    rpc.set_ready(true);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let stop = CancellationToken::new();
    let force = CancellationToken::new();
    let task = tokio::spawn(rpc.clone().serve(listener, stop.clone(), force.clone()));
    rpc.wait_initialized().await.unwrap();
    let mut server = Server {
        rpc,
        address,
        stop,
        force,
        task: Some(task),
        writer: Some(writer),
        dir,
    };
    server
        .rpc
        .arm_case(arm_timeout(100, vec!["getBalance"], 5000, 10000))
        .await
        .unwrap();
    assert_eq!(
        server.call("getBalance", json!(["bad"])).await["error"]["code"],
        -32602
    );
    let account = bs58::encode([14u8; 32]).into_string();
    let mut stream = server
        .raw(json!({"jsonrpc":"2.0","id":1,"method":"getBalance","params":[account]}))
        .await;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    assert!(bytes.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    server.rpc.disarm_case().await.unwrap();
    assert_eq!(
        server.call("getBalance", json!([account])).await["result"]["value"],
        123
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    source_task.await.unwrap();
    server.settle().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recovery_interval_dispatches_new_requests_and_the_next_cycle_repeats_outage() {
    let mut server = Server::new().await;
    let started = Instant::now();
    server
        .rpc
        .arm_case(arm_timeout(60, vec![], 60, 180))
        .await
        .unwrap();
    // Wait through the bounded outage without issuing RPC requests.
    tokio::time::sleep_until((started + Duration::from_millis(90)).into()).await;
    assert!(
        server
            .call("getLatestBlockhash", json!([]))
            .await
            .get("result")
            .is_some()
    );
    tokio::time::sleep_until((started + Duration::from_millis(255)).into()).await;
    let mut stream = server
        .raw(json!({"jsonrpc":"2.0","id":2,"method":"getLatestBlockhash","params":[]}))
        .await;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    assert!(bytes.is_empty());
    let facts = server.events().await;
    let selected = facts
        .iter()
        .find(|f| f.kind == "rpc.dispatch_prevented")
        .unwrap();
    assert_eq!(selected.data["selection"]["cycle"], 1);
    server.settle().await;
}
