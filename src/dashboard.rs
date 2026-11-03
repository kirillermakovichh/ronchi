pub mod viewer;
// Browser transport. Static files come only from the build-time asset map.
use axum::{
    body::Body,
    http::{Response, StatusCode},
};
include!(concat!(env!("OUT_DIR"), "/dashboard_assets.rs"));

pub fn asset(path: &str) -> Response<Body> {
    let path = if path.is_empty() { "index.html" } else { path };
    let Some((_, bytes)) = ASSETS.iter().find(|(name, _)| *name == path) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    };
    let mime = match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    };
    Response::builder()
        .header("content-type", mime)
        .header("cache-control", if path == "index.html" { "no-store" } else { "public, max-age=31536000, immutable" })
        .header("content-security-policy", "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'")
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "no-referrer")
        .body(Body::from(*bytes)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_assets_do_not_fallback_to_index() {
        assert_eq!(asset("").status(), StatusCode::OK);
        assert_eq!(asset("api/unknown").status(), StatusCode::NOT_FOUND);
        assert_eq!(asset("../../Cargo.toml").status(), StatusCode::NOT_FOUND);
        assert_eq!(asset("index.html").headers()["cache-control"], "no-store");
    }
}

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue},
    middleware::Next,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct Sessions(Arc<Mutex<Pool>>);
#[derive(Default)]
struct Pool {
    tickets: HashMap<String, Instant>,
    sessions: HashMap<String, Instant>,
}
impl Pool {
    fn prune(&mut self) {
        let now = Instant::now();
        self.tickets.retain(|_, expiry| *expiry > now);
        self.sessions.retain(|_, expiry| *expiry > now);
    }
}
fn nonce() -> Result<String, StatusCode> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
impl Sessions {
    pub fn ticket(&self) -> Result<String, StatusCode> {
        let mut pool = self.0.lock().unwrap_or_else(|e| e.into_inner());
        pool.prune();
        if pool.tickets.len() >= 16 {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        let token = nonce()?;
        pool.tickets
            .insert(token.clone(), Instant::now() + Duration::from_secs(60));
        Ok(token)
    }
    fn exchange(&self, ticket: &str) -> Result<String, StatusCode> {
        let mut pool = self.0.lock().unwrap_or_else(|e| e.into_inner());
        pool.prune();
        if pool.tickets.remove(ticket).is_none() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        if pool.sessions.len() >= 32 {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        let session = nonce()?;
        pool.sessions
            .insert(session.clone(), Instant::now() + Duration::from_secs(1800));
        Ok(session)
    }
    fn valid(&self, session: &str) -> bool {
        let mut pool = self.0.lock().unwrap_or_else(|e| e.into_inner());
        pool.prune();
        pool.sessions.contains_key(session)
    }
}
fn same_origin(
    address: Option<std::net::SocketAddr>,
    headers: &HeaderMap,
    require_origin: bool,
) -> Result<(), StatusCode> {
    let address = address.ok_or(StatusCode::SERVICE_UNAVAILABLE)?.to_string();
    if headers.get("host").and_then(|v| v.to_str().ok()) != Some(&address) {
        return Err(StatusCode::FORBIDDEN);
    }
    match headers.get("origin").and_then(|v| v.to_str().ok()) {
        Some(origin) if origin == format!("http://{address}") => Ok(()),
        None if !require_origin
            && headers.get("sec-fetch-site").and_then(|v| v.to_str().ok())
                == Some("same-origin") =>
        {
            Ok(())
        }
        _ => Err(StatusCode::FORBIDDEN),
    }
}
fn cookie_name(control: &crate::control::Control) -> String {
    let manifest = control.snapshot();
    format!(
        "ronchi_{}_{}",
        manifest.lab_id,
        manifest.control_address.map(|a| a.port()).unwrap_or(0)
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange {
    ticket: String,
}
#[derive(Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Launch {
    pub launch_url: String,
    pub expires_in_seconds: u32,
}

pub async fn ticket(
    State(control): State<crate::control::Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Launch>, StatusCode> {
    crate::control::authorize(&control, &id, &headers)?;
    let manifest = control.snapshot();
    let ticket = control.browser.ticket()?;
    Ok(Json(Launch {
        launch_url: format!(
            "http://{}/ui/{}/#ticket={ticket}",
            manifest
                .control_address
                .ok_or(StatusCode::SERVICE_UNAVAILABLE)?,
            manifest.lab_id
        ),
        expires_in_seconds: 60,
    }))
}
async fn exchange(
    State(control): State<crate::control::Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(args): Json<Exchange>,
) -> Result<Response<Body>, StatusCode> {
    if id != control.snapshot().lab_id {
        return Err(StatusCode::NOT_FOUND);
    }
    same_origin(control.snapshot().control_address, &headers, true)?;
    if args.ticket.len() != 64 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let session = control.browser.exchange(&args.ticket)?;
    Ok(Response::builder()
        .header(
            "set-cookie",
            format!(
                "{}={session}; HttpOnly; SameSite=Strict; Path=/ui/{id}/; Max-Age=1800",
                cookie_name(&control)
            ),
        )
        .header("cache-control", "no-store")
        .body(Body::from("{}"))
        .unwrap())
}
pub async fn authenticate(
    State(control): State<crate::control::Control>,
    mut request: axum::extract::Request,
    next: Next,
) -> axum::response::Response {
    let result = same_origin(
        control.snapshot().control_address,
        request.headers(),
        request.method() != axum::http::Method::GET,
    );
    if let Err(status) = result {
        return status.into_response();
    }
    let name = cookie_name(&control);
    let session = request
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split(';')
                .find_map(|part| part.trim().strip_prefix(&format!("{name}=")))
        });
    if !session.is_some_and(|s| control.browser.valid(s)) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // This bridge exists only on cookie-authenticated browser routes. The public
    // bearer routes still reject all Origin headers.
    request.headers_mut().remove("origin");
    request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", control.token)).unwrap(),
    );
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}
async fn index(
    State(control): State<crate::control::Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response<Body> {
    if id != control.snapshot().lab_id {
        return StatusCode::NOT_FOUND.into_response();
    }
    if headers.get("host").and_then(|v| v.to_str().ok())
        != control
            .snapshot()
            .control_address
            .map(|a| a.to_string())
            .as_deref()
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    asset("")
}
async fn static_file(
    State(control): State<crate::control::Control>,
    Path((id, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response<Body> {
    if id != control.snapshot().lab_id {
        return StatusCode::NOT_FOUND.into_response();
    }
    if headers.get("host").and_then(|v| v.to_str().ok())
        != control
            .snapshot()
            .control_address
            .map(|a| a.to_string())
            .as_deref()
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    asset(&format!("assets/{path}"))
}
pub fn routes() -> Router<crate::control::Control> {
    Router::new()
        .route("/ui/{id}/", get(index))
        .route("/ui/{id}/assets/{*path}", get(static_file))
        .route(
            "/ui/{id}/session",
            post(exchange).layer(axum::extract::DefaultBodyLimit::max(256)),
        )
}

pub async fn launch(
    project: std::path::PathBuf,
    name: &str,
    id: Option<&str>,
) -> Result<Launch, crate::lab::LabError> {
    use crate::lab::LabError;
    use http_body_util::{BodyExt, Full, Limited};
    use hyper_util::{client::legacy::Client, rt::TokioExecutor};
    let result = crate::lab::status(project.clone(), name, id).await?;
    let manifest = result.manifest;
    if manifest.state != crate::lab::Lifecycle::Ready {
        return Err(LabError::unavailable(
            "Live dashboard requires a ready Lab; use ronchi dashboard --serve --lab-id <id> for retained evidence",
        ));
    }
    let ws = crate::workspace::Workspace::new(&project);
    let token = crate::workspace::read_token(&ws.lab(&manifest.lab_id)?.join("token"))?;
    let address = manifest
        .control_address
        .ok_or_else(|| LabError::unavailable("Control endpoint is absent"))?;
    let request = hyper::Request::builder()
        .method("POST")
        .uri(format!(
            "http://{address}/v1/labs/{}/dashboard-ticket",
            manifest.lab_id
        ))
        .header("authorization", format!("Bearer {token}"))
        .body(Full::new(bytes::Bytes::new()))
        .map_err(|_| LabError::internal("Cannot encode dashboard request"))?;
    tokio::time::timeout(Duration::from_secs(3), async {
        let client = Client::builder(TokioExecutor::new())
            .pool_max_idle_per_host(0)
            .build_http();
        let response = client
            .request(request)
            .await
            .map_err(|_| LabError::unavailable("Dashboard control request failed"))?;
        if !response.status().is_success() {
            return Err(LabError::unavailable("Dashboard launch refused"));
        }
        let body = Limited::new(response.into_body(), 4096)
            .collect()
            .await
            .map_err(|_| LabError::unavailable("Dashboard response exceeded its limit"))?
            .to_bytes();
        serde_json::from_slice(&body)
            .map_err(|_| LabError::internal("Dashboard launch response is invalid"))
    })
    .await
    .map_err(|_| LabError::unavailable("Dashboard launch deadline exceeded"))?
}

#[cfg(test)]
mod session_tests {
    use super::*;
    #[test]
    fn ticket_and_session_expiry_and_capacity_are_bounded() {
        let sessions = Sessions::default();
        let ticket = sessions.ticket().unwrap();
        sessions
            .0
            .lock()
            .unwrap()
            .tickets
            .insert(ticket.clone(), Instant::now() - Duration::from_secs(1));
        assert_eq!(sessions.exchange(&ticket), Err(StatusCode::UNAUTHORIZED));
        let ticket = sessions.ticket().unwrap();
        let session = sessions.exchange(&ticket).unwrap();
        assert!(sessions.valid(&session));
        sessions
            .0
            .lock()
            .unwrap()
            .sessions
            .insert(session.clone(), Instant::now() - Duration::from_secs(1));
        assert!(!sessions.valid(&session));
        for _ in 0..16 {
            sessions.ticket().unwrap();
        }
        assert_eq!(sessions.ticket(), Err(StatusCode::TOO_MANY_REQUESTS));
    }
}
