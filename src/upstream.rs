//! Explicit read-only outbound boundary. No fallback routing grants forwarding rights.
use crate::rpc::{MAX_RESPONSE_BYTES, RpcError};
use base64::Engine;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const READ_METHODS: [&str; 7] = [
    "getBalance",
    "getAccountInfo",
    "getMultipleAccounts",
    "getTokenAccountBalance",
    "getTokenAccountsByOwner",
    "getMinimumBalanceForRentExemption",
    "getRecentPrioritizationFees",
];
/// A stable opaque identifier. Endpoint paths, credentials and query values are never public facts.
pub fn source_identity(url: &str) -> String {
    let digest: String = Sha256::digest(url.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("read-upstream:sha256:{digest}")
}
#[derive(Clone)]
pub struct Upstream {
    uri: hyper::Uri,
    authorization: Option<hyper::header::HeaderValue>,
    identity: String,
    secrets: Vec<String>,
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
}
pub(crate) struct Read {
    pub result: Result<Value, RpcError>,
    pub facts: Vec<Value>,
}
impl Upstream {
    pub fn new(raw: &str) -> Result<Self, String> {
        if raw.len() > 8192 || raw.chars().any(char::is_whitespace) {
            return Err("Read Upstream URL is invalid or exceeds 8192 bytes".into());
        }
        let mut url = url::Url::parse(raw)
            .map_err(|_| "Read Upstream requires an absolute HTTP or HTTPS URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || url.fragment().is_some()
        {
            return Err(
                "Read Upstream requires an absolute HTTP or HTTPS URL without a fragment".into(),
            );
        }
        let mut secrets = vec![raw.to_string()];
        let username = url.username().to_string();
        let password = url.password().map(str::to_string);
        secrets.extend(
            [
                Some(username.clone()).filter(|s| !s.is_empty()),
                password.clone(),
            ]
            .into_iter()
            .flatten(),
        );
        secrets.extend(
            url.query_pairs()
                .map(|(_, value)| value.into_owned())
                .filter(|s| !s.is_empty()),
        );
        if let Some(query) = url.query() {
            secrets.extend(
                query
                    .split('&')
                    .filter_map(|pair| pair.split_once('=').map(|(_, value)| value.to_string()))
                    .filter(|s| !s.is_empty()),
            );
        }
        // Provider paths can contain API credentials. Their identity is opaque as well.
        secrets.extend(
            url.path_segments()
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        );
        let decoded_paths: Vec<_> = url
            .path_segments()
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .map(percent_decode)
            .collect::<Result<_, _>>()?;
        secrets.extend(decoded_paths);
        let authorization = if !username.is_empty() || password.is_some() {
            let user = percent_decode(&username)?;
            let password = percent_decode(password.as_deref().unwrap_or(""))?;
            secrets.extend(
                [user.clone(), password.clone()]
                    .into_iter()
                    .filter(|s| !s.is_empty()),
            );
            let value = format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
            );
            secrets.push(value.clone());
            Some(
                hyper::header::HeaderValue::from_str(&value)
                    .map_err(|_| "Read Upstream credentials are invalid")?,
            )
        } else {
            None
        };
        url.set_username("")
            .map_err(|_| "Read Upstream URL is invalid")?;
        url.set_password(None)
            .map_err(|_| "Read Upstream URL is invalid")?;
        secrets.push(url.to_string());
        let uri = url
            .as_str()
            .parse()
            .map_err(|_| "Read Upstream URL is invalid")?;
        secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
        secrets.dedup();
        let connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new())
            .pool_max_idle_per_host(0)
            .build(connector);
        Ok(Self {
            uri,
            authorization,
            identity: source_identity(raw),
            secrets,
            client,
        })
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    /// Final forwarding boundary. Callers cannot authorize additional methods.
    pub(crate) async fn read(&self, method: &str, params: &Value) -> Read {
        let result = if !READ_METHODS.contains(&method) {
            Err(failure("upstream_method_forbidden"))
        } else {
            tokio::time::timeout(Duration::from_secs(10), self.exchange(method, params))
                .await
                .unwrap_or_else(|_| Err(failure("upstream_timeout")))
        };
        let mut fact = json!({"source_identity":self.identity,"method":method,"authority":"read_upstream","transaction_overlay_mutates_accounts":false});
        let mut facts = Vec::new();
        match &result {
            Ok(value) => {
                let (safe, redacted) = self.redact(value);
                let source_bytes = serde_json::to_vec(value)
                    .expect("JSON values serialize")
                    .len();
                let mut bytes = serde_json::to_vec(&safe).expect("JSON values serialize");
                let safe = if bytes.len() > MAX_RESPONSE_BYTES {
                    let omitted = json!({"availability":"redacted","reason":"credential_redaction_byte_budget","source_result_bytes":source_bytes});
                    bytes = serde_json::to_vec(&omitted).expect("JSON values serialize");
                    omitted
                } else {
                    safe
                };
                fact["source_result_bytes"] = json!(source_bytes);
                fact["outcome"] = json!("result");
                fact["redacted"] = json!(redacted);
                if redacted {
                    fact["payload_status"] = json!("redacted");
                }
                fact["result_bytes"] = json!(bytes.len());
                if bytes.len() <= 24 * 1024 {
                    fact["result"] = safe;
                } else {
                    fact["result_encoding"] = json!("base64-json-chunks");
                    fact["chunk_schema_version"] = json!(2);
                    fact["result_chunks"] = json!(bytes.len().div_ceil(24 * 1024));
                    for (index, chunk) in bytes.chunks(24 * 1024).enumerate() {
                        facts.push(json!({"source_identity":self.identity,"method":method,"authority":"read_upstream","result_chunk":index,"encoding":"base64-json","chunk_schema_version":2,"payload":base64::engine::general_purpose::STANDARD.encode(chunk),"payload_status":if redacted {"redacted"} else {"available"},"redacted":redacted}));
                    }
                }
            }
            Err(error) => {
                fact["outcome"] = json!("error");
                fact["error_code"] = json!(error.code);
                fact["error_classification"] = error.data.clone().unwrap_or(Value::Null);
            }
        }
        facts.insert(0, fact);
        Read { result, facts }
    }
    pub(crate) fn redact(&self, value: &Value) -> (Value, bool) {
        fn contains_secret(value: &str, secrets: &[String]) -> bool {
            secrets
                .iter()
                .any(|secret| !secret.is_empty() && value.contains(secret))
        }
        fn visit(value: &Value, secrets: &[String], changed: &mut bool) -> Value {
            match value {
                Value::String(value) if contains_secret(value, secrets) => {
                    *changed = true;
                    json!("[redacted]")
                }
                Value::Array(values) => Value::Array(
                    values
                        .iter()
                        .map(|value| visit(value, secrets, changed))
                        .collect(),
                ),
                Value::Object(values) if values.keys().any(|key| contains_secret(key, secrets)) => {
                    *changed = true;
                    json!({"availability":"redacted","reason":"credential_in_key"})
                }
                Value::Object(values) => Value::Object(
                    values
                        .iter()
                        .map(|(key, value)| (key.clone(), visit(value, secrets, changed)))
                        .collect(),
                ),
                other => other.clone(),
            }
        }
        let mut changed = false;
        (visit(value, &self.secrets, &mut changed), changed)
    }
    async fn exchange(&self, method: &str, params: &Value) -> Result<Value, RpcError> {
        // Repeat permission at the actual network effect; no redirect or fallback is possible.
        if !READ_METHODS.contains(&method) {
            return Err(failure("upstream_method_forbidden"));
        }
        let payload = serde_json::to_vec(
            &json!({"jsonrpc":"2.0","id":"read","method":method,"params":params}),
        )
        .map_err(|_| failure("upstream_request"))?;
        let mut builder = hyper::Request::builder()
            .method("POST")
            .uri(self.uri.clone())
            .header("content-type", "application/json")
            .header("connection", "close");
        if let Some(value) = &self.authorization {
            builder = builder.header("authorization", value);
        }
        let request = builder
            .body(Full::new(Bytes::from(payload)))
            .map_err(|_| failure("upstream_request"))?;
        let response = self
            .client
            .request(request)
            .await
            .map_err(|_| failure("upstream_transport"))?;
        if response.status().is_redirection() {
            return Err(failure("upstream_redirect"));
        }
        if !response.status().is_success() {
            return Err(failure("upstream_http_status"));
        }
        let mut body = response.into_body();
        let mut bytes = Vec::new();
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|_| failure("upstream_transport"))?;
            if let Ok(chunk) = frame.into_data() {
                if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(bytes.len()) {
                    return Err(failure("upstream_response_bytes"));
                }
                bytes.extend_from_slice(&chunk);
            }
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| failure("upstream_json"))?;
        if value.get("jsonrpc") != Some(&json!("2.0"))
            || value.get("id") != Some(&json!("read"))
            || value.get("result").is_some() == value.get("error").is_some()
        {
            return Err(failure("upstream_envelope"));
        }
        if let Some(result) = value.get("result") {
            return Ok(result.clone());
        }
        let code = value["error"]["code"]
            .as_i64()
            .ok_or_else(|| failure("upstream_envelope"))?;
        Err(RpcError {
            code,
            message: "Read Upstream returned an RPC error".into(),
            data: Some(json!({"code":"upstream_rpc_error","source_identity":self.identity})),
        })
    }
}
fn failure(code: &str) -> RpcError {
    RpcError {
        code: -32000,
        message: "Read Upstream result is unavailable".into(),
        data: Some(json!({"code":code})),
    }
}
fn percent_decode(value: &str) -> Result<String, String> {
    // URL form decoding supplies percent decoding; protect literal plus signs.
    let encoded = format!("value={}", value.replace('+', "%2B"));
    url::form_urlencoded::parse(encoded.as_bytes())
        .next()
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| "Read Upstream credentials are invalid".into())
}
