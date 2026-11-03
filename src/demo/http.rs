//! Bounded local application HTTP. No redirects, retries, or public endpoints.
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use std::{net::SocketAddr, time::Duration};
use tokio_util::sync::CancellationToken;
struct Connection(Option<tokio::task::JoinHandle<Result<(), hyper::Error>>>);
impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}
pub(super) async fn request(
    address: SocketAddr,
    method: &str,
    path: &str,
    value: Value,
    cancel: &CancellationToken,
) -> Result<Value, &'static str> {
    if !address.ip().is_loopback() {
        return Err("non_local_endpoint");
    }
    let mut connection = Connection(None);
    let work = async {
        let stream = tokio::net::TcpStream::connect(address)
            .await
            .map_err(|_| "source_unavailable")?;
        let (mut sender, task) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .map_err(|_| "source_transport")?;
        connection.0 = Some(tokio::spawn(task));
        let bytes = if method == "GET" {
            Vec::new()
        } else {
            serde_json::to_vec(&value).map_err(|_| "invalid_request")?
        };
        let request = hyper::Request::builder()
            .method(method)
            .uri(path)
            .header("host", address.to_string())
            .header("content-type", "application/json")
            .header("connection", "close")
            .body(Full::new(Bytes::from(bytes)))
            .map_err(|_| "invalid_request")?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|_| "source_transport")?;
        if !response.status().is_success() {
            return Err("source_http_status");
        }
        let bytes = Limited::new(response.into_body(), 256 * 1024)
            .collect()
            .await
            .map_err(|_| "source_response_limit")?
            .to_bytes();
        serde_json::from_slice(&bytes).map_err(|_| "source_json")
    };
    let result = tokio::select! {biased;_=cancel.cancelled()=>Err("cancelled"),result=tokio::time::timeout(Duration::from_secs(5),work)=>result.unwrap_or(Err("source_timeout"))};
    if let Some(task) = connection.0.take() {
        task.abort();
        let _ = task.await;
    }
    result
}
