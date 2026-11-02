use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use ronchi::{
    journal::{JournalReader, JournalStore, JournalWriter},
    rpc::{MAX_BATCH, MAX_REQUEST_BYTES, Rpc},
    simulator::clock::{Clock, ClockProfile, ManualTime},
};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

struct Server {
    address: SocketAddr,
    time: ManualTime,
    stop: CancellationToken,
    force: CancellationToken,
    task: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    writer: Option<JournalWriter>,
    directory: tempfile::TempDir,
}
impl Server {
    async fn new(profile: ClockProfile) -> Self {
        let directory = tempfile::TempDir::new().unwrap();
        let writer = JournalWriter::start(
            JournalStore::create(&directory.path().join("journal.sqlite"), "L-http").unwrap(),
        )
        .unwrap();
        let (clock, time) = Clock::controlled(profile, b"independent-test-network").unwrap();
        let rpc = Rpc::new(clock, writer.handle(), Instant::now());
        rpc.set_ready(true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let stop = CancellationToken::new();
        let force = CancellationToken::new();
        let task = tokio::spawn(rpc.serve(listener, stop.clone(), force.clone()));
        Self {
            address,
            time,
            stop,
            force,
            task: Some(task),
            writer: Some(writer),
            directory,
        }
    }
    async fn raw(&self, body: &str) -> (u16, Vec<u8>) {
        self.request("POST", "application/json", body).await
    }
    async fn request(&self, method: &str, content_type: &str, body: &str) -> (u16, Vec<u8>) {
        let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
            .pool_max_idle_per_host(0)
            .build_http();
        let request = hyper::Request::builder()
            .method(method)
            .uri(format!("http://{}/", self.address))
            .header("content-type", content_type)
            .body(Full::new(Bytes::from(body.to_string())))
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(5), client.request(request))
            .await
            .unwrap()
            .unwrap();
        let status = response.status().as_u16();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, body.to_vec())
    }
    async fn json(&self, value: Value) -> Value {
        let (status, bytes) = self.raw(&value.to_string()).await;
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
        serde_json::from_slice(&bytes).unwrap()
    }
    async fn call(&self, method: &str, params: Value) -> Value {
        self.json(json!({"jsonrpc":"2.0","id":"q","method":method,"params":params}))
            .await
    }
    async fn shutdown(&mut self) {
        self.stop.cancel();
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
async fn actual_http_envelopes_ids_notifications_batches_and_limits() {
    let mut server = Server::new(ClockProfile::default()).await;
    for id in [
        json!(null),
        json!("quoted\"id"),
        json!(1.25),
        serde_json::from_str("123456789012345678901234567890").unwrap(),
    ] {
        let response = server
            .json(json!({"jsonrpc":"2.0","id":id,"method":"getHealth"}))
            .await;
        assert_eq!(response["id"], id);
        assert_eq!(response["result"], "ok");
    }
    let result = server
        .json(json!([
            {"jsonrpc":"2.0","id":"repeat","method":"getHealth"},
            {"jsonrpc":"2.0","id":"repeat","method":"getSlot"},
            {"jsonrpc":"2.0","method":"getHealth"},17,
            {"jsonrpc":"2.0","id":true,"method":"getHealth"}
        ]))
        .await;
    assert_eq!(result.as_array().unwrap().len(), 4);
    assert_eq!(result[0]["id"], "repeat");
    assert_eq!(result[1]["id"], "repeat");
    assert_eq!(result[2]["error"]["code"], -32600);
    assert!(result[3]["id"].is_null());
    for value in [
        json!({"jsonrpc":"2.0","method":"getHealth"}),
        json!([{"jsonrpc":"2.0","method":"unknown"}]),
    ] {
        let (status, body) = server.raw(&value.to_string()).await;
        assert_eq!(status, 204);
        assert!(body.is_empty());
    }
    let batch: Vec<_> = (0..MAX_BATCH)
        .map(|id| json!({"jsonrpc":"2.0","id":id,"method":"getHealth"}))
        .collect();
    assert_eq!(
        server.json(json!(batch)).await.as_array().unwrap().len(),
        MAX_BATCH
    );
    let over: Vec<_> = (0..=MAX_BATCH)
        .map(|id| json!({"jsonrpc":"2.0","id":id,"method":"getHealth"}))
        .collect();
    assert_eq!(server.json(json!(over)).await["error"]["code"], -32600);
    assert_eq!(server.json(json!([])).await["error"]["code"], -32600);
    let (status, _) = server.raw(&" ".repeat(MAX_REQUEST_BYTES + 1)).await;
    assert_eq!(status, 413);
    let (status, _) = server.request("GET", "application/json", "").await;
    assert_eq!(status, 405);
    let (status, _) = server.request("POST", "text/plain", "{}").await;
    assert_eq!(status, 415);
    server.shutdown().await;
    let reader = JournalReader::open(&server.directory.path().join("journal.sqlite")).unwrap();
    let snapshot = reader.snapshot(false).unwrap();
    let mut notifications = 0;
    let mut registered = 0;
    let mut boundaries = 0;
    for seq in 1..=snapshot.snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "rpc.registered" {
            registered += 1;
        }
        if event.kind == "rpc.boundary" {
            boundaries += 1;
            assert_eq!(event.data["client_receipt"], "unknown");
            if event.data["rpc_response"] == "notification_no_response" {
                notifications += 1;
            }
        }
    }
    assert_eq!(notifications, 3);
    assert_eq!(registered, boundaries);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_http_rejects_invalid_protocol_and_method_parameters() {
    let mut server = Server::new(ClockProfile::default()).await;
    let (_, bytes) = server.raw("{").await;
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["error"]["code"],
        -32700
    );
    for value in [
        json!(42),
        json!({"jsonrpc":"1.0","method":"getHealth","id":1}),
        json!({"jsonrpc":"2.0","method":42,"id":1}),
        json!({"jsonrpc":"2.0","method":"getHealth","params":true,"id":1}),
    ] {
        assert_eq!(server.json(value).await["error"]["code"], -32600);
    }
    for (method, params) in [
        ("getHealth", json!([{}])),
        ("getSlot", json!({"commitment":"processed"})),
        ("getSlot", json!([{"commitment":"bogus"}])),
        ("getBlockHeight", json!([{"minContextSlot":-1}])),
        ("getLatestBlockhash", json!([{"unknown":1}])),
        ("isBlockhashValid", json!([])),
        ("isBlockhashValid", json!(["not-a-hash"])),
    ] {
        assert_eq!(
            server.call(method, params).await["error"]["code"],
            -32602,
            "{method}"
        );
    }
    assert_eq!(
        server.call("unknownWrite", json!([])).await["error"]["code"],
        -32601
    );
    for method in ["simulateTransaction", "requestAirdrop", "getBalance"] {
        assert_eq!(
            server.call(method, json!([])).await["error"]["data"]["code"],
            "unsupported_capability"
        );
    }
    assert_eq!(
        server.call("getSlot", json!([{"commitment":null}])).await["result"],
        0
    );
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn autonomous_clock_views_and_min_context_are_coherent_over_http() {
    let mut server = Server::new(ClockProfile {
        initial_slot: 100,
        initial_block_height: 10,
        validity_window_blocks: 3,
        ..Default::default()
    })
    .await;
    let genesis = server.call("getGenesisHash", json!([])).await["result"].clone();
    assert_eq!(
        bs58::decode(genesis.as_str().unwrap())
            .into_vec()
            .unwrap()
            .len(),
        32
    );
    let version = server.call("getVersion", json!([])).await;
    assert!(
        version["result"]["solana-core"]
            .as_str()
            .unwrap()
            .contains("ronchi")
    );
    assert_eq!(version["result"]["ronchi"]["compatibility"], "partial");
    server.time.advance(Duration::from_secs(8)).unwrap(); // No RPC requests drive these eight seconds.
    for (commitment, slot, height) in [
        ("processed", 120, 30),
        ("confirmed", 115, 25),
        ("finalized", 110, 20),
    ] {
        let config = json!({"commitment":commitment});
        assert_eq!(
            server.call("getSlot", json!([config])).await["result"],
            slot
        );
        assert_eq!(
            server.call("getBlockHeight", json!([config])).await["result"],
            height
        );
        let latest = server.call("getLatestBlockhash", json!([config])).await;
        assert_eq!(latest["result"]["context"]["slot"], slot);
        assert_eq!(
            latest["result"]["value"]["lastValidBlockHeight"],
            height + 3
        );
        assert_eq!(
            server
                .call(
                    "isBlockhashValid",
                    json!([latest["result"]["value"]["blockhash"], config])
                )
                .await["result"]["value"],
            true
        );
    }
    assert_eq!(server.call("getSlot", json!([])).await["result"], 110);
    let future = server
        .call("getLatestBlockhash", json!([{"commitment":"processed"}]))
        .await["result"]["value"]["blockhash"]
        .clone();
    assert_eq!(
        server
            .call(
                "isBlockhashValid",
                json!([future,{"commitment":"finalized"}])
            )
            .await["result"]["value"],
        false
    );
    let min = server
        .call(
            "getLatestBlockhash",
            json!([{"commitment":"finalized","minContextSlot":115}]),
        )
        .await;
    assert_eq!(min["error"]["code"], -32016);
    assert_eq!(min["error"]["data"]["contextSlot"], 110);
    assert_eq!(
        server.call("getGenesisHash", json!([])).await["result"],
        genesis
    );
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn validity_before_at_and_after_last_valid_height_uses_real_http() {
    let mut server = Server::new(ClockProfile {
        initial_slot: 90,
        initial_block_height: 9,
        validity_window_blocks: 3,
        ..Default::default()
    })
    .await;
    let latest = server
        .call("getLatestBlockhash", json!([{"commitment":"processed"}]))
        .await;
    let hash = latest["result"]["value"]["blockhash"].clone();
    assert_eq!(latest["result"]["value"]["lastValidBlockHeight"], 12);
    for (advance, height, valid) in [(1199, 11, true), (1, 12, true), (400, 13, false)] {
        server.time.advance(Duration::from_millis(advance)).unwrap();
        assert_eq!(
            server
                .call("getBlockHeight", json!([{"commitment":"processed"}]))
                .await["result"],
            height
        );
        let answer = server
            .call("isBlockhashValid", json!([hash,{"commitment":"processed"}]))
            .await;
        assert_eq!(answer["result"]["value"], valid);
        assert_eq!(answer["result"]["context"]["slot"], height + 81);
    }
    server.shutdown().await;
}
