use super::{Definition, Error, SourceRequest, SourceResponse};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::rt::TokioIo;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
pub(super) struct Runtime {
    pub definition: Definition,
    authorization: Option<hyper::header::HeaderValue>,
    pub secrets: Vec<String>,
}
impl Runtime {
    pub fn new(definition: Definition) -> Result<Self, Error> {
        definition.validate().map_err(|_| Error::invalid())?;
        let text = definition
            .authorization_env
            .as_ref()
            .map(|key| std::env::var(key).map_err(|_| Error::invalid()))
            .transpose()?;
        let authorization = text
            .as_ref()
            .map(|v| {
                if v.is_empty() || v.len() > 4096 {
                    return Err(Error::invalid());
                }
                hyper::header::HeaderValue::from_str(v).map_err(|_| Error::invalid())
            })
            .transpose()?;
        let mut secrets = Vec::new();
        if let Some(text) = text {
            if let Some(token) = text.strip_prefix("Bearer ")
                && !token.is_empty()
            {
                secrets.push(token.into());
            }
            secrets.push(text);
        }
        Ok(Self {
            definition,
            authorization,
            secrets,
        })
    }
}
#[derive(Default)]
struct Connection(Option<tokio::task::JoinHandle<Result<(), hyper::Error>>>);
impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}
pub(super) async fn post(
    runtime: &Runtime,
    request: &SourceRequest,
    deadline: tokio::time::Instant,
    cancel: &CancellationToken,
) -> Result<SourceResponse, &'static str> {
    let mut connection = Connection::default();
    let result = {
        let work = async {
            let url =
                url::Url::parse(&runtime.definition.url).map_err(|_| "source_configuration")?;
            let host = url
                .host_str()
                .ok_or("source_configuration")?
                .trim_matches(['[', ']']);
            let mut addresses = tokio::net::lookup_host((
                host,
                url.port_or_known_default().ok_or("source_configuration")?,
            ))
            .await
            .map_err(|_| "source_unavailable")?;
            let address = addresses.next().ok_or("source_unavailable")?;
            if runtime.definition.allowed_origin.is_none() && !address.ip().is_loopback() {
                return Err("source_origin");
            }
            let stream = tokio::net::TcpStream::connect(address)
                .await
                .map_err(|_| "source_unavailable")?;
            let (mut sender, task) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
                .await
                .map_err(|_| "source_transport")?;
            connection.0 = Some(tokio::spawn(task));
            let bytes = serde_json::to_vec(request).map_err(|_| "source_configuration")?;
            let mut builder = hyper::Request::builder()
                .method("POST")
                .uri(url.as_str())
                .header("content-type", "application/json")
                .header("connection", "close");
            if let Some(value) = &runtime.authorization {
                builder = builder.header("authorization", value);
            }
            let request = builder
                .body(Full::new(Bytes::from(bytes)))
                .map_err(|_| "source_configuration")?;
            let response = sender
                .send_request(request)
                .await
                .map_err(|_| "source_transport")?;
            if response.status().is_redirection() {
                return Err("source_redirect");
            }
            if !response.status().is_success() {
                return Err("source_http_status");
            }
            let mut body = response.into_body();
            let mut bytes = Vec::new();
            let limit = runtime.definition.effective_bytes();
            while let Some(frame) = body.frame().await {
                let frame = frame.map_err(|_| "source_transport")?;
                if let Ok(chunk) = frame.into_data() {
                    if chunk.len() > limit.saturating_sub(bytes.len()) {
                        return Err("source_response_bytes");
                    }
                    bytes.extend_from_slice(&chunk);
                }
            }
            serde_json::from_slice(&bytes).map_err(|_| "source_json")
        };
        tokio::select! {biased;_=cancel.cancelled()=>Err("cancelled"),result=tokio::time::timeout_at(deadline,work)=>result.unwrap_or(Err("source_timeout"))}
    };
    if let Some(task) = connection.0.take() {
        task.abort();
        let _ = task.await;
    }
    result
}
