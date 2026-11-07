//! Real HTTP source-to-client histories preserve source error identity across the shared core.
use super::*;
use crate::{
    journal::{JournalReader, JournalStore, JournalWriter, WriterHealth},
    rpc::Rpc,
};
use axum::{Json, Router, routing::post};
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use tokio_util::sync::CancellationToken;
struct Server {
    address: SocketAddr,
    task: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    source: Option<tokio::task::JoinHandle<()>>,
    stop: CancellationToken,
    force: CancellationToken,
    writer: Option<JournalWriter>,
    dir: tempfile::TempDir,
    signature: String,
    encoded: String,
}
impl Server {
    async fn new(errors: Vec<Value>, datasource: Option<&str>) -> Self {
        let key = SigningKey::from_bytes(&[103; 32]);
        let mut message = vec![1, 0, 0, 1];
        message.extend(key.verifying_key().to_bytes());
        message.extend([7u8; 32]);
        message.push(0);
        let signature = key.sign(&message);
        let mut wire = vec![1];
        wire.extend(signature.to_bytes());
        wire.extend(message);
        let encoded = base64::engine::general_purpose::STANDARD.encode(wire);
        let signature = bs58::encode(signature.to_bytes()).into_string();
        let replies = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from(
            errors,
        )));
        let known = signature.clone();
        let router = Router::new().route(
            "/",
            post(move |Json(request): Json<Value>| {
                let result = replies.lock().unwrap().pop_front();
                let known = known.clone();
                async move {
                    Json(match result {
                        Some(error) => json!({"jsonrpc":"2.0","id":request["id"],"error":error}),
                        None => json!({"jsonrpc":"2.0","id":request["id"],"result":known}),
                    })
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_address = listener.local_addr().unwrap();
        let source = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let dir = tempfile::tempdir().unwrap();
        let writer = JournalWriter::start(
            JournalStore::create(&dir.path().join("journal.sqlite"), "L-source-error").unwrap(),
        )
        .unwrap();
        let clock =
            crate::simulator::clock::Clock::realtime(Default::default(), b"source-error-wire")
                .unwrap();
        let backend = Client::new(
            source_address,
            "surfpool:source-error-fixture".into(),
            datasource,
        )
        .unwrap();
        let rpc = Rpc::with_backend(clock, writer.handle(), Instant::now(), backend).unwrap();
        rpc.set_ready(true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let stop = CancellationToken::new();
        let force = CancellationToken::new();
        let task = tokio::spawn(rpc.clone().serve(listener, stop.clone(), force.clone()));
        rpc.wait_initialized().await.unwrap();
        Self {
            address,
            task: Some(task),
            source: Some(source),
            stop,
            force,
            writer: Some(writer),
            dir,
            signature,
            encoded,
        }
    }
    async fn call(&self, method: &str, params: Value) -> Value {
        self.try_call(method, params).await.unwrap()
    }
    async fn try_call(&self, method: &str, params: Value) -> Result<Value, String> {
        let client: HttpClient<HttpConnector, Full<Bytes>> =
            HttpClient::builder(TokioExecutor::new())
                .pool_max_idle_per_host(0)
                .build_http();
        let request = hyper::Request::builder()
            .method("POST")
            .uri(format!("http://{}/", self.address))
            .body(Full::new(Bytes::from(
                json!({"jsonrpc":"2.0","id":7,"method":method,"params":params}).to_string(),
            )))
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), client.request(request))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .to_bytes();
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    }
    async fn submit(&self) -> Value {
        self.call(
            "sendTransaction",
            json!([self.encoded,{"encoding":"base64"}]),
        )
        .await
    }
    async fn finish(mut self) {
        self.stop.cancel();
        self.force.cancel();
        self.task.take().unwrap().await.unwrap().unwrap();
        let source = self.source.take().unwrap();
        source.abort();
        let _ = source.await;
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
        if let Some(source) = &self.source {
            source.abort();
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preflight_error_data_reaches_http_client_in_canonical_shape() {
    let expected = json!({"code":-32002,"message":format!("Transaction simulation failed: Blockhash not found {}","safe detail ".repeat(80)),"data":{"err":"BlockhashNotFound","logs":["Program log: simulated failure"],"accounts":null,"unitsConsumed":0,"returnData":null}});
    let server = Server::new(vec![expected.clone()], None).await;
    let response = server.submit().await;
    assert_eq!(
        response["error"], expected,
        "HTTP must retain source error shape, without internal diagnostic wrapper"
    );
    assert_eq!(
        server.writer.as_ref().unwrap().handle().status().health,
        WriterHealth::Healthy
    );
    assert_eq!(server.submit().await["result"], server.signature);
    server.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bounded_long_source_messages_and_optional_data_keep_their_wire_shapes() {
    let long = json!({"code":-32603,"message":format!("Source operation failed: {}","safe Unicode details 界 ".repeat(160)),"data":{"cause":"source_internal_error"}});
    assert!(long["message"].as_str().unwrap().len() > 2048);
    let absent = json!({"code":-32002,"message":"Preflight rejected without data"});
    let null = json!({"code":-32002,"message":"Preflight rejected with null data","data":null});
    let server = Server::new(vec![long.clone(), absent.clone(), null.clone()], None).await;
    for expected in [long, absent, null] {
        let reply = server.submit().await;
        assert_eq!(reply["error"], expected);
        assert_eq!(
            server.writer.as_ref().unwrap().handle().status().health,
            WriterHealth::Healthy
        );
    }
    assert_eq!(server.submit().await["result"], server.signature);
    server.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn large_source_error_stays_payload_only_and_writer_accepts_later_submission() {
    use crate::query::{Arguments, Engine, EventRef, Operation, Request, Scope};
    let marker = "large-source-error-".repeat(6000);
    let error = json!({"code":-32002,"message":"Transaction simulation failed","data":{"err":"BlockhashNotFound","logs":[marker,"safe detail"],"unitsConsumed":0}});
    let expected = error.clone();
    let server = Server::new(vec![error], None).await;
    let reply = server.submit().await;
    assert_eq!(reply["error"], expected);
    assert_eq!(
        server.writer.as_ref().unwrap().handle().status().health,
        WriterHealth::Healthy
    );
    assert_eq!(server.submit().await["result"], server.signature);
    let path = server.dir.path().join("journal.sqlite");
    let reader = JournalReader::open(&path).unwrap();
    let snapshot = reader.snapshot(true).unwrap().snapshot_seq;
    for seq in 1..=snapshot {
        let event = reader.event(seq).unwrap().unwrap();
        assert!(serde_json::to_vec(&event).unwrap().len() <= crate::journal::MAX_EVENT_BYTES);
    }
    drop(reader);
    let engine = Engine::default();
    let request = Request {
        contract_version: 1,
        request_id: "source-error-query".into(),
        lab_id: "L-source-error".into(),
        operation: Operation::Timeline,
        arguments: Arguments {
            scope: Scope {
                kind: vec!["rpc.backend".into()],
                ..Default::default()
            },
            snapshot_seq: Some(snapshot),
            ..Default::default()
        },
    };
    let page = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            request.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let items = page.result["items"].as_array().unwrap();
    assert!(items.iter().all(|i| i.get("payload").is_none()));
    assert!(!page.result.to_string().contains("large-source-error-"));
    let source: Vec<_> = items
        .iter()
        .filter(|i| i["data"]["source_outcome"] == "error")
        .collect();
    assert!(source.len() > 1);
    assert!(source.iter().all(|i| i["data"]["error_code"] == -32002
        && i["data"]["error_classification"] == "backend_rpc_error"
        && i["data"]["chunk_schema_version"] == 2
        && i["data"]["source_result"]["redacted"] == false
        && i["payload_status"] == "available"));
    let mut reconstructed = Vec::new();
    for item in source {
        let reference: EventRef = serde_json::from_value(item["event_ref"].clone()).unwrap();
        let mut base64 = String::new();
        let mut offset = 0;
        loop {
            let args = Arguments {
                scope: Scope {
                    event_refs: vec![reference.clone()],
                    ..Default::default()
                },
                include_payload: true,
                payload_offset: offset,
                snapshot_seq: Some(snapshot),
                ..Default::default()
            };
            let preview = engine
                .execute(
                    path.clone(),
                    true,
                    "explicit",
                    Request {
                        operation: Operation::Evidence,
                        arguments: args,
                        ..request.clone()
                    },
                    CancellationToken::new(),
                )
                .await
                .unwrap();
            let payload = &preview.result["items"][0]["payload"];
            base64.push_str(payload["preview"].as_str().unwrap());
            if let Some(next) = payload["next_offset"].as_u64() {
                offset = next as u32;
            } else {
                break;
            }
        }
        reconstructed.extend(
            base64::engine::general_purpose::STANDARD
                .decode(base64)
                .unwrap(),
        );
    }
    let retained: Value = serde_json::from_slice(&reconstructed).unwrap();
    assert_eq!(retained["error"]["data"]["backend_data"], expected["data"]);
    assert_eq!(
        server.writer.as_ref().unwrap().handle().status().health,
        WriterHealth::Healthy
    );
    server.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_required_evidence_failure_is_not_classified_as_backend_rpc_error() {
    let mut server = Server::new(vec![], None).await;
    let database = rusqlite::Connection::open(server.dir.path().join("journal.sqlite")).unwrap();
    database.execute_batch("CREATE TRIGGER fail_backend_dispatch BEFORE INSERT ON events WHEN json_extract(NEW.event_json,'$.kind')='rpc.backend_started' BEGIN SELECT RAISE(ABORT,'required evidence fixture'); END;").unwrap();
    let result = server
        .try_call(
            "sendTransaction",
            json!([server.encoded,{"encoding":"base64"}]),
        )
        .await;
    // Fail-stop may close the owned RPC transport before an error body is written.
    if let Ok(result) = result {
        assert_eq!(result["error"]["code"], -32603);
        assert_eq!(result["error"]["data"], json!({"code":"journal_failed"}));
    }
    assert_ne!(
        server.writer.as_ref().unwrap().handle().status().health,
        WriterHealth::Healthy
    );
    server.stop.cancel();
    server.force.cancel();
    assert!(server.task.take().unwrap().await.unwrap().is_err());
    let source = server.source.take().unwrap();
    source.abort();
    let _ = source.await;
    let writer = server.writer.take().unwrap();
    assert!(
        tokio::task::spawn_blocking(move || writer.shutdown(None))
            .await
            .unwrap()
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_redaction_provenance_survives_canonical_wire_conversion_and_payload_policy() {
    use crate::query::{Arguments, Engine, EventRef, Operation, Request, Scope};
    let error = json!({"code":-32002,"message":"Preflight source-secret","data":{"err":"BlockhashNotFound","logs":["credential source-secret"]}});
    let server = Server::new(
        vec![error],
        Some("http://127.0.0.1:1/private-fixture?token=source-secret"),
    )
    .await;
    let reply = server.submit().await;
    assert_eq!(reply["error"]["message"], "[redacted]");
    assert_eq!(reply["error"]["data"]["logs"][0], "[redacted]");
    assert!(reply["error"]["data"].get("backend_data").is_none());
    let path = server.dir.path().join("journal.sqlite");
    let engine = Engine::default();
    let request = Request {
        contract_version: 1,
        request_id: "redacted-source-error".into(),
        lab_id: "L-source-error".into(),
        operation: Operation::Timeline,
        arguments: Arguments {
            scope: Scope {
                kind: vec!["rpc.backend".into()],
                ..Default::default()
            },
            ..Default::default()
        },
    };
    let page = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            request.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(!page.result.to_string().contains("source-secret"));
    let item = &page.result["items"][0];
    assert_eq!(item["payload_status"], "redacted");
    assert_eq!(item["data"]["source_result"]["redacted"], true);
    let reference: EventRef = serde_json::from_value(item["event_ref"].clone()).unwrap();
    let args = Arguments {
        scope: Scope {
            event_refs: vec![reference],
            ..Default::default()
        },
        include_payload: true,
        snapshot_seq: Some(page.view.snapshot_seq),
        ..Default::default()
    };
    let evidence = engine
        .execute(
            path,
            true,
            "explicit",
            Request {
                operation: Operation::Evidence,
                arguments: args,
                ..request
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(evidence.result["items"][0]["payload"], Value::Null);
    assert!(!evidence.result.to_string().contains("source-secret"));
    assert_eq!(
        server.writer.as_ref().unwrap().handle().status().health,
        WriterHealth::Healthy
    );
    assert_eq!(server.submit().await["result"], server.signature);
    server.finish().await;
}
