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
}
pub(super) fn response(value: Value) -> Response {
    if crate::query::encoded_len(&value, 256 * 1024).is_err() {
        return failure(Error::Busy);
    }
    Json(value).into_response()
}
pub(super) fn failure(error: Error) -> Response {
    let status = match error {
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
pub(super) fn router(store: Store, address: std::net::SocketAddr) -> Router {
    Router::new()
        .route("/intents", post(intake))
        .route("/intents/{id}", get(intent))
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
                    Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
                }
            },
        ))
        .with_state(Http { store })
}
