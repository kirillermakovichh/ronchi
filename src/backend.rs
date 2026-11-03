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
        let safe = self
            .0
            .redactor
            .as_ref()
            .map(|redactor| redactor.redact(&envelope["error"]).0)
            .unwrap_or_else(|| envelope["error"].clone());
        let message = safe["message"]
            .as_str()
            .filter(|message| message.len() <= 2048)
            .unwrap_or("Managed Surfpool returned an RPC error")
            .to_string();
        Err(RpcError {
            code,
            message,
            data: Some(
                json!({"code":"backend_rpc_error","authority":self.0.authority,"backend_data":safe.get("data")}),
            ),
        })
    }
    pub(crate) fn source_facts(
        &self,
        method: &str,
        result: &Result<Value, RpcError>,
        started: Instant,
    ) -> Vec<Value> {
        let context = json!({"method":method,"authority":"execution_backend","source_identity":self.0.authority,"observed_elapsed_ns":started.elapsed().as_nanos().min(i64::MAX as u128)as u64});
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
        let encoded = safe.to_string();
        let mut facts = Vec::new();
        let chunks = encoded.as_bytes().chunks(16 * 1024).collect::<Vec<_>>();
        use base64::Engine as _;
        for (index, chunk) in chunks.iter().enumerate() {
            let mut fact = context.clone();
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
