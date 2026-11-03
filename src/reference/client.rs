use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{net::SocketAddr, time::Duration};
use tokio_util::sync::CancellationToken;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", content = "value", rename_all = "snake_case")]
pub(super) enum Outcome {
    Response(Value),
    RpcError(i64),
    Timeout,
    TransportError,
    InvalidResponse,
    Cancelled,
}
impl Outcome {
    pub fn response(&self) -> Option<&Value> {
        if let Self::Response(v) = self {
            Some(v)
        } else {
            None
        }
    }
}
pub(super) async fn rpc(
    address: SocketAddr,
    method: &str,
    params: Value,
    deadline: Duration,
    stop: &CancellationToken,
) -> Outcome {
    if !address.ip().is_loopback() {
        return Outcome::InvalidResponse;
    }
    let mut connection = None;
    let result = {
        let work = async {
            let stream = tokio::net::TcpStream::connect(address)
                .await
                .map_err(|_| ())?;
            let (mut sender, task) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
                .await
                .map_err(|_| ())?;
            connection = Some(tokio::spawn(task));
            let body = serde_json::to_vec(
                &json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}),
            )
            .map_err(|_| ())?;
            let request = hyper::Request::builder()
                .method("POST")
                .uri(format!("http://{address}/"))
                .header("host", address.to_string())
                .header("content-type", "application/json")
                .header("connection", "close")
                .body(Full::new(Bytes::from(body)))
                .map_err(|_| ())?;
            let response = sender.send_request(request).await.map_err(|_| ())?;
            if !response.status().is_success() {
                return Ok(Outcome::TransportError);
            }
            let bytes = Limited::new(response.into_body(), 256 * 1024)
                .collect()
                .await
                .map_err(|_| ())?
                .to_bytes();
            let value: Value = match serde_json::from_slice(&bytes) {
                Ok(value) => value,
                Err(_) => return Ok(Outcome::InvalidResponse),
            };
            if value["jsonrpc"] != "2.0" || value["id"] != 1 {
                return Ok(Outcome::InvalidResponse);
            }
            Ok(if let Some(result) = value.get("result") {
                Outcome::Response(result.clone())
            } else if let Some(code) = value["error"]["code"].as_i64() {
                Outcome::RpcError(code)
            } else {
                Outcome::InvalidResponse
            })
        };
        tokio::select! {biased;_ =stop.cancelled()=>Outcome::Cancelled,result=tokio::time::timeout(deadline,work)=>match result{Err(_)=>Outcome::Timeout,Ok(Err(()))=>Outcome::TransportError,Ok(Ok(outcome))=>outcome}}
    };
    if let Some(task) = connection {
        task.abort();
        let _ = task.await;
    }
    result
}
