use crate::{
    lab::{LabError, Manifest},
    workspace::Workspace,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct Control {
    pub writer: crate::journal::WriterHandle,
    pub manifest: Arc<RwLock<Manifest>>,
    pub token: String,
    pub stop: mpsc::Sender<bool>,
}
impl Control {
    pub fn snapshot(&self) -> Manifest {
        let mut manifest = self
            .manifest
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let status = self.writer.status();
        manifest.snapshot_seq = status.snapshot_seq;
        if status.health == crate::journal::WriterHealth::Failed {
            manifest.health = "failed".into();
        }
        manifest
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Down {
    force: bool,
}
fn authorize(control: &Control, id: &str, headers: &HeaderMap) -> Result<(), StatusCode> {
    let manifest = control.snapshot();
    if id != manifest.lab_id {
        return Err(StatusCode::NOT_FOUND);
    }
    let endpoint = manifest
        .control_address
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .to_string();
    if headers.get("host").and_then(|h| h.to_str().ok()) != Some(endpoint.as_str())
        || headers.contains_key("origin")
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let provided = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .unwrap_or("");
    // Fixed-length random tokens; compare every byte before deciding.
    let mut difference = provided.len() ^ control.token.len();
    for (a, b) in provided.bytes().zip(control.token.bytes()) {
        difference |= (a ^ b) as usize;
    }
    if difference != 0 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}
async fn status(
    State(control): State<Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Manifest>, StatusCode> {
    authorize(&control, &id, &headers)?;
    Ok(Json(control.snapshot()))
}
async fn down(
    State(control): State<Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(args): Json<Down>,
) -> Result<(StatusCode, Json<Manifest>), StatusCode> {
    authorize(&control, &id, &headers)?;
    if !matches!(
        control.snapshot().state,
        crate::lab::Lifecycle::Ready | crate::lab::Lifecycle::Stopping
    ) {
        return Err(StatusCode::CONFLICT);
    }
    match control.stop.try_send(args.force) {
        Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => (),
        Err(_) => return Err(StatusCode::SERVICE_UNAVAILABLE),
    }
    Ok((StatusCode::ACCEPTED, Json(control.snapshot())))
}
pub fn router(control: Control) -> Router {
    Router::new()
        .route("/v1/labs/{id}/status", get(status))
        .route("/v1/labs/{id}/down", post(down))
        .layer(axum::extract::DefaultBodyLimit::max(1024))
        .layer(axum::middleware::from_fn(
            |request: axum::extract::Request, next: axum::middleware::Next| async move {
                use axum::response::IntoResponse;
                match tokio::time::timeout(std::time::Duration::from_secs(30), next.run(request))
                    .await
                {
                    Ok(response) => response,
                    Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
                }
            },
        ))
        .with_state(control)
}
pub async fn request(
    ws: &Workspace,
    manifest: &Manifest,
    stop: bool,
    force: bool,
) -> Result<Manifest, LabError> {
    let token = crate::workspace::read_token(&ws.lab(&manifest.lab_id)?.join("token"))?;
    if token.len() != 64 {
        return Err(LabError::conflict("Control credential is invalid"));
    }
    let address = manifest
        .control_address
        .ok_or_else(|| LabError::unavailable("Lab control endpoint is absent"))?;
    if !address.ip().is_loopback() {
        return Err(LabError::conflict("Control endpoint must be loopback"));
    }
    let uri = format!(
        "http://{address}/v1/labs/{}/{}",
        manifest.lab_id,
        if stop { "down" } else { "status" }
    );
    let body = if stop {
        serde_json::to_vec(&Down { force })
            .map_err(|_| LabError::internal("Cannot encode shutdown request"))?
    } else {
        Vec::new()
    };
    let request = hyper::Request::builder()
        .method(if stop { "POST" } else { "GET" })
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .map_err(|_| LabError::internal("Cannot construct control request"))?;
    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .pool_max_idle_per_host(0)
        .build_http();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let response = client
            .request(request)
            .await
            .map_err(|_| LabError::unavailable("Lab control is unavailable"))?;
        if !response.status().is_success() {
            return Err(LabError::conflict(
                "Daemon refused the authenticated control operation",
            ));
        }
        let bytes = Limited::new(response.into_body(), 64 * 1024)
            .collect()
            .await
            .map_err(|_| LabError::internal("Invalid bounded control response"))?
            .to_bytes();
        serde_json::from_slice(&bytes)
            .map_err(|_| LabError::conflict("Daemon response is incompatible"))
    })
    .await
    .map_err(|_| LabError::unavailable("Lab control deadline exceeded"))
    .and_then(|result| result);
    result.map_err(|mut error| {
        if stop {
            error.effects = "unknown";
            error.lab_id = Some(manifest.lab_id.clone());
        }
        error
    })
}
