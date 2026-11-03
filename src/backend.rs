//! One owned local Execution Backend is the exclusive blockchain RPC authority.
pub mod surfpool;
mod validation;
use crate::rpc::{MAX_RESPONSE_BYTES, RpcError};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{Client as HttpClient, connect::HttpConnector},
    rt::TokioExecutor,
};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
pub use validation::{Prepared, prepare};
#[derive(Clone)]
pub struct Client(std::sync::Arc<ClientState>);
struct ClientState {
    address: SocketAddr,
    authority: String,
    redactor: Option<crate::upstream::Upstream>,
    cancelled: tokio_util::sync::CancellationToken,
    client: HttpClient<HttpConnector, Full<Bytes>>,
}
pub const METHODS: [&str; 20] = [
    "getHealth",
    "getVersion",
    "getGenesisHash",
    "getSlot",
    "getBlockHeight",
    "getLatestBlockhash",
    "isBlockhashValid",
    "sendTransaction",
    "simulateTransaction",
    "getSignatureStatuses",
    "getTransaction",
    "getFeeForMessage",
    "getBalance",
    "getAccountInfo",
    "getMultipleAccounts",
    "getTokenAccountBalance",
    "getTokenAccountsByOwner",
    "getMinimumBalanceForRentExemption",
    "getRecentPrioritizationFees",
    "requestAirdrop",
];
impl Client {
    pub(crate) fn new(
        address: SocketAddr,
        authority: String,
        datasource: Option<&str>,
    ) -> Result<Self, String> {
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err("Execution Backend must be an owned loopback endpoint".into());
        }
        Ok(Self(std::sync::Arc::new(ClientState {
            address,
            authority,
            redactor: datasource.map(crate::upstream::Upstream::new).transpose()?,
            cancelled: Default::default(),
            client: HttpClient::builder(TokioExecutor::new())
                .pool_max_idle_per_host(0)
                .build_http(),
        })))
    }
    pub(crate) fn cancel(&self) {
        self.0.cancelled.cancel();
    }
    pub(crate) fn redact(&self, value: &Value) -> (Value, bool) {
        self.0
            .redactor
            .as_ref()
            .map(|redactor| redactor.redact(value))
            .unwrap_or_else(|| (value.clone(), false))
    }
    pub fn authority(&self) -> &str {
        &self.0.authority
    }
    /// Final network boundary. Unknown/provider-specific methods cannot reach this backend.
    pub async fn call(&self, method: &str, params: &Value) -> Result<Value, RpcError> {
        if !METHODS.contains(&method) {
            return Err(RpcError {
                code: -32601,
                message: "Method not found".into(),
                data: None,
            });
        }
        tokio::select! {
            _=self.0.cancelled.cancelled()=>Err(failure("backend_cancelled")),
            result=tokio::time::timeout(Duration::from_secs(10),self.exchange(method,params))=>result.map_err(|_|failure("backend_deadline"))?,
        }
    }
    async fn exchange(&self, method: &str, params: &Value) -> Result<Value, RpcError> {
        let bytes = serde_json::to_vec(
            &json!({"jsonrpc":"2.0","id":"backend","method":method,"params":params}),
        )
        .map_err(|_| failure("backend_request"))?;
        let request = hyper::Request::builder()
            .method("POST")
            .uri(format!("http://{}/", self.0.address))
            .header("content-type", "application/json")
            .header("connection", "close")
            .body(Full::new(Bytes::from(bytes)))
            .map_err(|_| failure("backend_request"))?;
        let response = self
            .0
            .client
            .request(request)
            .await
            .map_err(|_| failure("backend_transport"))?;
        if !response.status().is_success() {
            return Err(failure("backend_http_status"));
        }
        let mut body = response.into_body();
        let mut bytes = Vec::new();
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|_| failure("backend_transport"))?;
            if let Ok(chunk) = frame.into_data() {
                if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(bytes.len()) {
                    return Err(failure("backend_response_bytes"));
                }
                bytes.extend_from_slice(&chunk);
            }
        }
        let envelope: Value =
            serde_json::from_slice(&bytes).map_err(|_| failure("backend_json"))?;
        if envelope["jsonrpc"] != "2.0"
            || envelope["id"] != "backend"
            || envelope.get("result").is_some() == envelope.get("error").is_some()
        {
            return Err(failure("backend_envelope"));
        }
        if let Some(result) = envelope.get("result") {
            return Ok(result.clone());
        }
        let code = envelope["error"]["code"]
            .as_i64()
            .ok_or_else(|| failure("backend_envelope"))?;
        let (safe, redacted) = self.redact(&envelope["error"]);
        let message = safe["message"]
            .as_str()
            .unwrap_or("Managed Surfpool returned an RPC error")
            .to_string();
        let mut classification =
            json!({"code":"backend_rpc_error","authority":self.0.authority,"redacted":redacted});
        if let Some(data) = safe.get("data") {
            classification["backend_data"] = data.clone();
        }
        Err(RpcError {
            code,
            message,
            data: Some(classification),
        })
    }
    pub(crate) fn source_facts(
        &self,
        method: &str,
        result: &Result<Value, RpcError>,
        started: Instant,
    ) -> Vec<Value> {
        let mut context = json!({"method":method,"authority":"execution_backend","source_identity":self.0.authority,"observed_elapsed_ns":started.elapsed().as_nanos().min(i64::MAX as u128)as u64});
        if let Err(error) = result {
            context["error_code"] = json!(error.code);
            context["error_classification"] = json!(
                error
                    .data
                    .as_ref()
                    .and_then(|data| data.get("code"))
                    .and_then(Value::as_str)
                    .filter(|classification| classification.len() <= 128)
                    .unwrap_or("unclassified")
            );
        }
        let raw = match result {
            Ok(result) => json!({"result":result}),
            Err(error) => json!({"error":error}),
        };
        let (safe, redacted) = self
            .0
            .redactor
            .as_ref()
            .map(|redactor| redactor.redact(&raw))
            .unwrap_or((raw, false));
        let redacted = redacted
            || result
                .as_ref()
                .err()
                .and_then(|error| error.data.as_ref())
                .and_then(|data| data.get("redacted"))
                .and_then(Value::as_bool)
                == Some(true);
        let encoded = safe.to_string();
        let mut facts = Vec::new();
        let chunks = encoded.as_bytes().chunks(16 * 1024).collect::<Vec<_>>();
        use base64::Engine as _;
        for (index, chunk) in chunks.iter().enumerate() {
            let mut fact = context.clone();
            fact["chunk_schema_version"] = json!(2);
            fact["payload_status"] = json!(if redacted { "redacted" } else { "available" });
            fact["source_outcome"] = json!(if result.is_ok() { "result" } else { "error" });
            fact["source_result"] = json!({"availability":"retained_chunks","encoding":"base64-json","chunk":index,"chunks":chunks.len(),"bytes":encoded.len(),"redacted":redacted});
            fact["payload"] = json!(base64::engine::general_purpose::STANDARD.encode(chunk));
            facts.push(fact);
        }
        facts
    }
}
pub(crate) fn failure(code: &str) -> RpcError {
    RpcError {
        code: -32000,
        message: "Managed Surfpool result is unavailable".into(),
        data: Some(json!({"code":code,"effects":"unknown"})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::post};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn backend_internal_rpc_error_preserves_writer_and_later_submission() {
        use base64::Engine as _;
        use ed25519_dalek::{Signer, SigningKey};
        let key = SigningKey::from_bytes(&[97; 32]);
        let mut message = vec![1, 0, 0, 1];
        message.extend(key.verifying_key().to_bytes());
        message.extend([7u8; 32]);
        message.push(0);
        let signature = key.sign(&message);
        let mut wire = vec![1];
        wire.extend(signature.to_bytes());
        wire.extend(message);
        let encoded = base64::engine::general_purpose::STANDARD.encode(wire);
        let identity = bs58::encode(signature.to_bytes()).into_string();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let response_identity = identity.clone();
        let router=Router::new().route("/",post(move||{let count=count.clone();let identity=response_identity.clone();async move{
            Json(if count.fetch_add(1,Ordering::SeqCst)==0{json!({"jsonrpc":"2.0","id":"backend","error":{"code":-32603,"message":"fixture backend operation failed"}})}else{json!({"jsonrpc":"2.0","id":"backend","result":identity})})
        }}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let directory = tempfile::tempdir().unwrap();
        let store = crate::journal::JournalStore::create(
            &directory.path().join("journal.sqlite"),
            "L-backend-source-error",
        )
        .unwrap();
        let writer = crate::journal::JournalWriter::start(store).unwrap();
        let handle = writer.handle();
        let clock =
            crate::simulator::clock::Clock::realtime(Default::default(), b"backend-source-error")
                .unwrap();
        let (status, _) = tokio::sync::watch::channel(crate::cases::Binding::default());
        let (transactions, mut owner) = crate::simulator::transactions::Transactions::channel(
            clock,
            handle.clone(),
            Instant::now(),
            Default::default(),
            status,
            Default::default(),
        );
        owner.backend_mode();
        owner.initialize().await.unwrap();
        let closing = tokio_util::sync::CancellationToken::new();
        let close = closing.clone();
        let actor = tokio::spawn(async move { owner.run(close, Default::default()).await });
        let client = Client::new(address, "surfpool:fixture:source-error".into(), None).unwrap();
        let reference = json!({"binding":crate::cases::Binding::default(),"request_id":"first","responds":true,"batch_size":1,"execution_mode":"surfpool"});
        let first = transactions
            .backend_submit(
                client.clone(),
                prepare("sendTransaction", &json!([encoded,{"encoding":"base64"}]))
                    .await
                    .unwrap(),
                reference.clone(),
                Default::default(),
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(first.code, -32603);
        assert_eq!(first.data.unwrap()["code"], "backend_rpc_error");
        let mut second = reference;
        second["request_id"] = json!("second");
        let accepted = transactions
            .backend_submit(
                client,
                prepare("sendTransaction", &json!([encoded,{"encoding":"base64"}]))
                    .await
                    .unwrap(),
                second,
                Default::default(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(accepted, identity);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            handle.status().health,
            crate::journal::WriterHealth::Healthy
        );
        closing.cancel();
        actor.await.unwrap().unwrap();
        server.abort();
        let _ = server.await;
        tokio::task::spawn_blocking(move || writer.shutdown(None))
            .await
            .unwrap()
            .unwrap();
    }
}

#[cfg(test)]
#[path = "backend/status_tests.rs"]
mod status_tests;

#[cfg(test)]
#[path = "backend/error_tests.rs"]
mod error_tests;
