use super::{Error, Store};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
#[derive(Clone)]
pub(super) struct Http {
    pub store: Store,
    pub lab_id: String,
}
pub(super) fn response(value: Value) -> Response {
    if crate::query::encoded_len(&value, 256 * 1024).is_err() {
        return failure(Error::Busy);
    }
    Json(value).into_response()
}
pub(super) fn failure(error: Error) -> Response {
    let status = match error {
        Error::Deadline => StatusCode::REQUEST_TIMEOUT,
        Error::Invalid => StatusCode::BAD_REQUEST,
        Error::Conflict => StatusCode::CONFLICT,
        Error::Busy => StatusCode::TOO_MANY_REQUESTS,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    (
        status,
        Json(
            json!({"contract_version":1,"error":{"code":error.code(),"message":error.to_string()}}),
        ),
    )
        .into_response()
}
async fn intake(
    State(http): State<Http>,
    input: Result<Json<super::IntentInput>, axum::extract::rejection::JsonRejection>,
) -> Response {
    match input {
        Ok(Json(input)) => match http.store.intake(input).await {
            Ok(value) => response(value),
            Err(error) => failure(error),
        },
        Err(_) => failure(Error::Invalid),
    }
}
async fn intent(State(http): State<Http>, Path(id): Path<String>) -> Response {
    if !crate::workspace::identifier(&id) {
        return failure(Error::Invalid);
    }
    match http.store.snapshot(&id).await {
        Ok(value) => response(value),
        Err(error) => failure(error),
    }
}
pub(super) fn router(store: Store, address: std::net::SocketAddr, lab_id: String) -> Router {
    Router::new()
        .route("/intents", post(intake))
        .route("/intents/{id}", get(intent))
        .route("/checks/observe", post(observe))
        .layer(axum::extract::DefaultBodyLimit::max(8192))
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| async move {
                if request.headers().contains_key("origin")
                    || request.headers().get("host").and_then(|h| h.to_str().ok())
                        != Some(address.to_string().as_str())
                {
                    return StatusCode::FORBIDDEN.into_response();
                }
                match tokio::time::timeout(std::time::Duration::from_secs(5), next.run(request))
                    .await
                {
                    Ok(response) => response,
                    Err(_) => failure(Error::Deadline),
                }
            },
        ))
        .with_state(Http { store, lab_id })
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationScope {
    intent_ids: Vec<String>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Observe {
    contract_version: u32,
    invocation_id: String,
    check_id: String,
    lab_id: String,
    snapshot_seq: u64,
    scope: ObservationScope,
    facts: Vec<String>,
    deadline: String,
}
async fn observe(
    State(http): State<Http>,
    input: Result<Json<Observe>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Ok(Json(input)) = input else {
        return failure(Error::Invalid);
    };
    if input.contract_version != 1
        || !crate::workspace::identifier(&input.invocation_id)
        || !crate::workspace::identifier(&input.check_id)
        || input.lab_id != http.lab_id
        || input.snapshot_seq > i64::MAX as u64
        || input.scope.intent_ids.len() != 1
        || !crate::workspace::identifier(&input.scope.intent_ids[0])
        || input.facts != ["intent"]
        || input.deadline.len() > 64
    {
        return failure(Error::Invalid);
    }
    let deadline = match time::OffsetDateTime::parse(
        &input.deadline,
        &time::format_description::well_known::Rfc3339,
    ) {
        Ok(time) => time,
        Err(_) => return failure(Error::Invalid),
    };
    let remaining = deadline - time::OffsetDateTime::now_utc();
    if remaining <= time::Duration::ZERO {
        return failure(Error::Deadline);
    }
    if remaining > time::Duration::seconds(30) {
        return failure(Error::Invalid);
    }
    let snapshot = match tokio::time::timeout(
        remaining.unsigned_abs(),
        http.store.snapshot(&input.scope.intent_ids[0]),
    )
    .await
    {
        Ok(Ok(snapshot)) => snapshot,
        Ok(Err(error)) => return failure(error),
        Err(_) => return failure(Error::Deadline),
    };
    let revision = snapshot
        .get("revision")
        .and_then(Value::as_i64)
        .map(|v| v.to_string());
    let fact = if snapshot["availability"] == "available" {
        let mut value = snapshot;
        value
            .as_object_mut()
            .expect("typed intent object")
            .remove("availability");
        super::available(value)
    } else {
        json!({"availability":"unavailable","reason":"intent_not_found"})
    };
    let mut result = json!({"contract_version":1,"invocation_id":input.invocation_id,"check_id":input.check_id,"lab_id":http.lab_id,"source":"reference-relayer","schema_version":1,"observed_at":time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).expect("current timestamp formats"),"scope":{"intent_ids":input.scope.intent_ids},"snapshot_seq":input.snapshot_seq,"snapshot_relation":"application_only","facts":{"intent":fact}});
    if let Some(revision) = revision {
        result["revision"] = json!(revision);
    }
    response(result)
}
