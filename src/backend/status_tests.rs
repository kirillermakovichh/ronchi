//! Controlled backend responses exercise the real RPC adapter without claiming Surfpool behavior.
use super::*;
use crate::{
    cases::Arm,
    journal::{JournalStore, JournalWriter},
    rpc::Rpc,
};
use axum::{Json, Router, routing::post};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use tokio_util::sync::CancellationToken;
async fn call(address: SocketAddr, method: &str, params: Value) -> Value {
    let client: HttpClient<HttpConnector, Full<Bytes>> =
        HttpClient::builder(TokioExecutor::new()).build_http();
    let request = hyper::Request::builder()
        .method("POST")
        .uri(format!("http://{address}/"))
        .body(Full::new(Bytes::from(
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string(),
        )))
        .unwrap();
    let response = client.request(request).await.unwrap();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_backend_snapshots_are_observed_and_commitment_compatible() {
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    let signer = SigningKey::from_bytes(&[127; 32]);
    let mut message = vec![1, 0, 0, 1];
    message.extend(signer.verifying_key().to_bytes());
    message.extend([7; 32]);
    message.push(0);
    let signature = signer.sign(&message);
    let mut wire = vec![1];
    wire.extend(signature.to_bytes());
    wire.extend(message);
    let wire = base64::engine::general_purpose::STANDARD.encode(wire);
    let signature = bs58::encode(signature.to_bytes()).into_string();
    let phase = Arc::new(AtomicU8::new(0));
    let observed = phase.clone();
    let known = signature.clone();
    let retained = wire.clone();
    let router=Router::new().route("/",post(move|Json(request):Json<Value>|{let phase=observed.load(Ordering::SeqCst);let known=known.clone();let wire=retained.clone();async move{
        let result=match request["method"].as_str().unwrap(){
            "sendTransaction"=>json!(known),
            "getSignatureStatuses"=>json!({"context":{"slot":50+u64::from(phase)},"value":request["params"][0].as_array().unwrap().iter().map(|signature|if signature==&known&&phase>0{if phase==5{json!(123)}else{let mut value=json!({"slot":7,"confirmations":if phase==3{Value::Null}else{json!(2)},"err":null,"confirmationStatus":match phase{1=>"processed",2|6=>"confirmed",_=>"finalized"}});if phase==6{value["padding"]=json!("x".repeat(20*1024));}value}}else{Value::Null}).collect::<Vec<_>>()}),
            "getTransaction"=>if request["params"][0]==known&&phase>=2{json!({"slot":7,"blockTime":null,"meta":{"err":null},"transaction":[wire,"base64"]})}else{Value::Null},
            _=>Value::Null,
        };Json(json!({"jsonrpc":"2.0","id":"backend","result":result}))
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_address = listener.local_addr().unwrap();
    let source = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let directory = tempfile::tempdir().unwrap();
    let writer = JournalWriter::start(
        JournalStore::create(&directory.path().join("journal.sqlite"), "L-status-backend").unwrap(),
    )
    .unwrap();
    let clock =
        crate::simulator::clock::Clock::realtime(Default::default(), b"status-backend").unwrap();
    let backend = Client::new(backend_address, "surfpool:status-fixture".into(), None).unwrap();
    let rpc = Rpc::with_backend(clock, writer.handle(), Instant::now(), backend).unwrap();
    rpc.set_ready(true);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let stop = CancellationToken::new();
    let force = CancellationToken::new();
    let task = tokio::spawn(rpc.clone().serve(listener, stop.clone(), force));
    rpc.wait_initialized().await.unwrap();
    rpc.arm_case(Arm {
        case_id: "stale-transaction-read-after-submission".into(),
        ..Default::default()
    })
    .await
    .unwrap();
    assert_eq!(
        call(
            address,
            "sendTransaction",
            json!([wire,{"encoding":"base64"}])
        )
        .await["result"],
        signature
    );
    phase.store(5, Ordering::SeqCst);
    assert_eq!(
        call(address, "getSignatureStatuses", json!([[signature]])).await["result"]["value"][0],
        123,
        "malformed source must not become a snapshot or masking Fault"
    );
    phase.store(6, Ordering::SeqCst);
    assert_eq!(
        call(address, "getSignatureStatuses", json!([[signature]])).await["error"]["data"]["code"],
        "case_capacity_exceeded"
    );
    assert_eq!(
        writer.handle().status().health,
        crate::journal::WriterHealth::Healthy
    );
    phase.store(2, Ordering::SeqCst);
    assert!(
        call(
            address,
            "getTransaction",
            json!([signature,{"commitment":"confirmed","encoding":"base64"}])
        )
        .await["result"]
            .is_object(),
        "a transaction response cannot invent an earlier status snapshot"
    );
    let first =
        call(address, "getSignatureStatuses", json!([[signature]])).await["result"]["value"][0]
            .clone();
    assert_eq!(first["confirmationStatus"], "confirmed");
    phase.store(3, Ordering::SeqCst);
    let unknown = bs58::encode([128; 64]).into_string();
    let frozen = call(
        address,
        "getSignatureStatuses",
        json!([[signature, unknown, signature]]),
    )
    .await;
    assert_eq!(frozen["result"]["value"], json!([first, null, first]));
    assert_eq!(frozen["result"]["context"]["slot"], 53);
    assert!(
        call(
            address,
            "getTransaction",
            json!([signature,{"commitment":"finalized","encoding":"base64"}])
        )
        .await["result"]
            .is_null()
    );
    assert!(
        call(
            address,
            "getTransaction",
            json!([signature,{"commitment":"confirmed","encoding":"base64"}])
        )
        .await["result"]
            .is_object()
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    loop {
        let recovered = call(address, "getSignatureStatuses", json!([[signature]])).await;
        if recovered["result"]["value"][0]["confirmationStatus"] == "finalized" {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "stale backend view did not recover: {recovered}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    rpc.disarm_case().await.unwrap();
    assert_eq!(
        call(address, "getSignatureStatuses", json!([[signature]])).await["result"]["value"][0]["confirmationStatus"],
        "finalized"
    );
    stop.cancel();
    task.await.unwrap().unwrap();
    source.abort();
    let _ = source.await;
    tokio::task::spawn_blocking(move || writer.shutdown(None))
        .await
        .unwrap()
        .unwrap();
    let reader =
        crate::journal::JournalReader::open(&directory.path().join("journal.sqlite")).unwrap();
    let mut observed = 0;
    let mut changed = 0;
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "rpc.lookup_source" {
            observed += 1;
            assert_eq!(event.source, "surfpool");
            assert_eq!(event.execution_mode, crate::config::Mode::Surfpool);
        }
        if event.kind == "fault.lookup_changed" {
            changed += 1;
        }
    }
    assert!(observed >= 5);
    assert!(changed >= 3);
}
