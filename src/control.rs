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
    pub queries: crate::query::Engine,
    pub journal_path: std::path::PathBuf,
    pub manifest: Arc<RwLock<Manifest>>,
    pub token: String,
    pub stop: mpsc::Sender<bool>,
    pub cases: Arc<RwLock<Option<crate::rpc::Rpc>>>,
    pub checks: Arc<RwLock<Option<crate::checks::Client>>>,
}
impl Control {
    pub fn snapshot(&self) -> Manifest {
        let mut manifest = self
            .manifest
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(rpc) = self
            .cases
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            manifest.held_effects = rpc.held_effects();
            let binding = rpc.case_status();
            manifest.active_case = binding.policy;
            manifest.case_registration_seq = binding.registration_seq;
        }
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
        .route(
            "/v1/labs/{id}/checks/observe",
            post(check_observe).layer(axum::extract::DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/v1/labs/{id}/check-collections/{collection}",
            get(check_status),
        )
        .route(
            "/v1/labs/{id}/check-collections/{collection}/cancel",
            post(check_cancel),
        )
        .route("/v1/labs/{id}/case/arm", post(case_arm))
        .route("/v1/labs/{id}/case/disarm", post(case_disarm))
        .route("/v1/labs/{id}/cases", get(case_list))
        .layer(axum::extract::DefaultBodyLimit::max(4096))
        .route(
            "/v1/labs/{id}/investigate",
            post(investigate).layer(axum::extract::DefaultBodyLimit::max(
                crate::query::MAX_REQUEST_BYTES,
            )),
        )
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

fn case_failure(id: String, error: LabError) -> (StatusCode, Json<serde_json::Value>) {
    let status = match error.exit {
        5 => StatusCode::CONFLICT,
        3 | 6 => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_REQUEST,
    };
    let details = crate::OperationError {
        code: error.code.into(),
        message: error.message,
        phase: "case".into(),
        retryable: false,
        effects: if error.exit == 6 {
            "unknown"
        } else {
            error.effects
        }
        .into(),
        recovery: "Inspect ronchi status and case list before another mutation".into(),
    };
    (
        status,
        Json(serde_json::json!({"contract_version":1,"lab_id":id,"error":details})),
    )
}
async fn case_apply(
    control: Control,
    id: String,
    headers: HeaderMap,
    args: Option<crate::cases::Arm>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    authorize(&control, &id, &headers)?;
    if control.snapshot().state != crate::lab::Lifecycle::Ready {
        return Ok(case_failure(
            id,
            LabError::conflict("Case changes require a ready Lab"),
        ));
    }
    let rpc = control
        .cases
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let result = match args {
        Some(args) => rpc.arm_case(args).await,
        None => rpc.disarm_case().await,
    };
    Ok(match result {
        Ok(binding) => (
            StatusCode::OK,
            Json(serde_json::json!({"contract_version":1,"lab_id":id,"result":binding})),
        ),
        Err(error) => case_failure(id, crate::cases::lab_error(error)),
    })
}
async fn case_arm(
    State(control): State<Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
    args: Result<Json<crate::cases::Arm>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    authorize(&control, &id, &headers)?;
    match args {
        Ok(Json(args)) => case_apply(control, id, headers, Some(args)).await,
        Err(_) => Ok(case_failure(
            id,
            LabError::invalid("Invalid Case JSON or unknown settings"),
        )),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DisarmArgs {}
async fn case_disarm(
    State(control): State<Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    authorize(&control, &id, &headers)?;
    if !body.is_empty() && serde_json::from_slice::<DisarmArgs>(&body).is_err() {
        return Ok(case_failure(
            id,
            LabError::invalid("Disarm accepts no settings"),
        ));
    }
    case_apply(control, id, headers, None).await
}
async fn case_list(
    State(control): State<Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    authorize(&control, &id, &headers)?;
    Ok(Json(
        serde_json::json!({"contract_version":1,"lab_id":id,"result":crate::cases::catalog()}),
    ))
}
pub async fn case_request(
    ws: &Workspace,
    manifest: &Manifest,
    args: Option<crate::cases::Arm>,
) -> Result<crate::cases::Binding, LabError> {
    let token = crate::workspace::read_token(&ws.lab(&manifest.lab_id)?.join("token"))?;
    let address = manifest
        .control_address
        .ok_or_else(|| LabError::conflict("No live control endpoint"))?;
    let request = hyper::Request::builder()
        .method("POST")
        .uri(format!(
            "http://{address}/v1/labs/{}/case/{}",
            manifest.lab_id,
            if args.is_some() { "arm" } else { "disarm" }
        ))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(
            serde_json::to_vec(
                &args
                    .as_ref()
                    .map(|args| serde_json::to_value(args).expect("typed arguments serialize"))
                    .unwrap_or_else(|| serde_json::json!({})),
            )
            .map_err(|_| LabError::internal("Cannot encode Case operation"))?,
        )))
        .map_err(|_| LabError::internal("Cannot construct Case operation"))?;
    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .pool_max_idle_per_host(0)
        .build_http();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let response = client
            .request(request)
            .await
            .map_err(|_| LabError::unavailable("Case operation transport failed"))?;
        let status = response.status();
        let bytes = Limited::new(response.into_body(), 64 * 1024)
            .collect()
            .await
            .map_err(|_| LabError::internal("Invalid Case response"))?
            .to_bytes();
        if status == StatusCode::CONFLICT {
            return Err(LabError::conflict("Case operation requires a ready Lab"));
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| LabError::internal("Invalid Case response"))?;
        if !status.is_success() {
            let code = value["error"]["code"].as_str().unwrap_or("internal_error");
            let message = value["error"]["message"]
                .as_str()
                .unwrap_or("Case operation failed");
            return Err(match code {
                "invalid_argument" => LabError::invalid(message),
                "unsupported_capability" => LabError::unsupported(message),
                "state_conflict" => LabError::conflict(message),
                _ => LabError::internal(message),
            });
        }
        serde_json::from_value(value["result"].clone())
            .map_err(|_| LabError::internal("Invalid Case binding"))
    })
    .await
    .map_err(|_| LabError::unavailable("Case operation deadline exceeded"))
    .and_then(|result| result);
    result.map_err(|mut error| {
        error.lab_id = Some(manifest.lab_id.clone());
        if error.exit == 3 || error.exit == 6 {
            error.effects = "unknown";
        }
        error
    })
}

async fn investigate(
    State(control): State<Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
    args: Result<Json<crate::query::Request>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    authorize(&control, &id, &headers)?;
    let request = match args {
        Ok(Json(request)) => request,
        Err(_) => {
            return Ok(query_failure(
                &id,
                "unparsed",
                crate::query::Error::invalid("Invalid typed query JSON or request exceeds 64 KiB"),
            ));
        }
    };
    let request_id = request.request_id.clone();
    if request.lab_id != id {
        return Ok(query_failure(
            &id,
            &request_id,
            crate::query::Error::invalid("Route and request Lab identity differ"),
        ));
    }
    match control
        .queries
        .execute(
            control.journal_path,
            true,
            "explicit",
            request,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
    {
        Ok(response) => Ok((
            StatusCode::OK,
            Json(serde_json::to_value(response).expect("typed response serializes")),
        )),
        Err(error) => Ok(query_failure(&id, &request_id, error)),
    }
}
fn query_failure(
    id: &str,
    request_id: &str,
    error: crate::query::Error,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(
            serde_json::to_value(crate::query::Failure {
                contract_version: 1,
                request_id: request_id.into(),
                lab_id: Some(id.into()),
                error,
            })
            .expect("typed error serializes"),
        ),
    )
}
pub async fn query_request(
    ws: &Workspace,
    manifest: &Manifest,
    request: &crate::query::Request,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<crate::query::Response, crate::query::Error> {
    use crate::query::Error;
    let token = crate::workspace::read_token(
        &ws.lab(&manifest.lab_id)
            .map_err(|_| Error::storage())?
            .join("token"),
    )
    .map_err(|_| Error::storage())?;
    let address = manifest
        .control_address
        .filter(|a| a.ip().is_loopback())
        .ok_or_else(|| {
            Error::new(
                "lab_unavailable",
                "No authenticated loopback control endpoint",
            )
        })?;
    let request = hyper::Request::builder()
        .method("POST")
        .uri(format!(
            "http://{address}/v1/labs/{}/investigate",
            manifest.lab_id
        ))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(
            serde_json::to_vec(request).map_err(|_| Error::storage())?,
        )))
        .map_err(|_| Error::storage())?;
    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .pool_max_idle_per_host(0)
        .build_http();
    let future = async {
        let response = client
            .request(request)
            .await
            .map_err(|_| Error::new("lab_unavailable", "Investigation control transport failed"))?;
        let status = response.status();
        let bytes = Limited::new(response.into_body(), crate::query::MAX_RESPONSE_BYTES)
            .collect()
            .await
            .map_err(|_| Error::storage())?
            .to_bytes();
        if status.is_success() {
            serde_json::from_slice(&bytes).map_err(|_| Error::storage())
        } else {
            #[derive(Deserialize)]
            struct Failure {
                error: Error,
            }
            Err(serde_json::from_slice::<Failure>(&bytes)
                .map_err(|_| Error::storage())?
                .error)
        }
    };
    tokio::select! {
        result=tokio::time::timeout(std::time::Duration::from_secs(10),future)=>result.map_err(|_|Error::new("deadline_exceeded","Investigation control deadline exceeded"))?,
        _=cancel.cancelled()=>Err(Error::new("cancelled","Investigation cancelled; the Lab keeps running"))
    }
}

fn check_failure(
    id: &str,
    request_id: &str,
    collection_id: Option<String>,
    error: crate::checks::Error,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(
            serde_json::to_value(crate::checks::Failure {
                contract_version: 1,
                request_id: request_id.into(),
                lab_id: Some(id.into()),
                collection_id,
                error,
            })
            .expect("typed Check error serializes"),
        ),
    )
}
fn check_client(control: &Control) -> Result<crate::checks::Client, crate::checks::Error> {
    control
        .checks
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(crate::checks::Error::invalid)
}
async fn check_observe(
    State(control): State<Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
    args: Result<Json<crate::checks::Request>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    authorize(&control, &id, &headers)?;
    let Ok(Json(request)) = args else {
        return Ok(check_failure(
            &id,
            "unparsed",
            None,
            crate::checks::Error::invalid(),
        ));
    };
    let collection = Some(request.arguments.collection_id.clone());
    if request.contract_version != 1
        || request.operation != "check.observe"
        || request.lab_id != id
        || !crate::workspace::identifier(&request.request_id)
    {
        return Ok(check_failure(
            &id,
            &request.request_id,
            collection,
            crate::checks::Error::invalid(),
        ));
    }
    if control.snapshot().state != crate::lab::Lifecycle::Ready {
        return Ok(check_failure(
            &id,
            &request.request_id,
            collection,
            crate::checks::Error::new("state_conflict", "none"),
        ));
    }
    let result = match check_client(&control) {
        Ok(client) => client.start(request.arguments).await,
        Err(error) => Err(error),
    };
    Ok(match result {
        Ok(report) => (
            StatusCode::OK,
            Json(
                serde_json::to_value(crate::checks::Reply {
                    contract_version: 1,
                    request_id: request.request_id,
                    lab_id: id,
                    result: report,
                })
                .expect("typed Check reply serializes"),
            ),
        ),
        Err(error) => check_failure(&id, &request.request_id, collection, error),
    })
}
async fn check_status(
    State(control): State<Control>,
    Path((id, collection)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    authorize(&control, &id, &headers)?;
    check_action(control, id, collection, false).await
}
async fn check_cancel(
    State(control): State<Control>,
    Path((id, collection)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    authorize(&control, &id, &headers)?;
    if !body.is_empty()
        && serde_json::from_slice::<serde_json::Value>(&body).ok() != Some(serde_json::json!({}))
    {
        return Ok(check_failure(
            &id,
            &collection,
            Some(collection.clone()),
            crate::checks::Error::invalid(),
        ));
    }
    check_action(control, id, collection, true).await
}
async fn check_action(
    control: Control,
    id: String,
    collection: String,
    cancel: bool,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    if !crate::workspace::identifier(&collection) {
        return Ok(check_failure(
            &id,
            "unparsed",
            None,
            crate::checks::Error::invalid(),
        ));
    }
    let result = match check_client(&control) {
        Ok(client) => {
            if cancel {
                client.cancel(collection.clone()).await
            } else {
                client.status(collection.clone()).await
            }
        }
        Err(error) => Err(error),
    };
    Ok(match result {
        Ok(report) => (
            StatusCode::OK,
            Json(
                serde_json::to_value(crate::checks::Reply {
                    contract_version: 1,
                    request_id: collection,
                    lab_id: id,
                    result: report,
                })
                .expect("typed Check reply serializes"),
            ),
        ),
        Err(error) => check_failure(&id, &collection, Some(collection.clone()), error),
    })
}

/// One bounded authenticated collection control exchange; no transport retry.
pub async fn check_request(
    ws: &Workspace,
    manifest: &Manifest,
    start: Option<&crate::checks::Request>,
    collection: &str,
    cancel: bool,
) -> Result<crate::checks::Reply, crate::checks::Error> {
    use crate::checks::Error;
    let token = crate::workspace::read_token(
        &ws.lab(&manifest.lab_id)
            .map_err(|_| Error::journal())?
            .join("token"),
    )
    .map_err(|_| Error::journal())?;
    let address = manifest
        .control_address
        .filter(|a| a.ip().is_loopback())
        .ok_or_else(|| Error::new("lab_unavailable", "unknown"))?;
    let (method, suffix, body) = if let Some(request) = start {
        (
            "POST",
            "checks/observe".to_string(),
            serde_json::to_vec(request).map_err(|_| Error::invalid())?,
        )
    } else if cancel {
        (
            "POST",
            format!("check-collections/{collection}/cancel"),
            b"{}".to_vec(),
        )
    } else {
        ("GET", format!("check-collections/{collection}"), Vec::new())
    };
    let request = hyper::Request::builder()
        .method(method)
        .uri(format!(
            "http://{address}/v1/labs/{}/{suffix}",
            manifest.lab_id
        ))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .map_err(|_| Error::invalid())?;
    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .pool_max_idle_per_host(0)
        .build_http();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let response = client
            .request(request)
            .await
            .map_err(|_| Error::new("lab_unavailable", "unknown"))?;
        let status = response.status();
        let bytes = Limited::new(response.into_body(), 32 * 1024)
            .collect()
            .await
            .map_err(|_| Error::journal())?
            .to_bytes();
        if status.is_success() {
            serde_json::from_slice(&bytes).map_err(|_| Error::journal())
        } else {
            Err(serde_json::from_slice::<crate::checks::Failure>(&bytes)
                .map_err(|_| Error::new("lab_unavailable", "unknown"))?
                .error)
        }
    })
    .await
    .map_err(|_| Error::new("lab_unavailable", "unknown"))?
}
