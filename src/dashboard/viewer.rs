//! Foreground read-only viewer. No simulator, relayer, or Journal writer.
use super::*;
use crate::{
    lab::{LabError, Manifest},
    query,
    workspace::Workspace,
};
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Viewer {
    manifest: Manifest,
    path: std::path::PathBuf,
    sessions: Sessions,
    queries: query::Engine,
    sockets: socket::Hub,
}
fn host(v: &Viewer, id: &str, h: &HeaderMap) -> Result<(), StatusCode> {
    if id != v.manifest.lab_id {
        return Err(StatusCode::NOT_FOUND);
    }
    if h.get("host").and_then(|x| x.to_str().ok())
        != v.manifest.control_address.map(|a| a.to_string()).as_deref()
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}
fn cookie(v: &Viewer) -> String {
    format!(
        "ronchi_{}_{}",
        v.manifest.lab_id,
        v.manifest.control_address.unwrap().port()
    )
}
async fn index(
    State(v): State<Viewer>,
    Path(id): Path<String>,
    h: HeaderMap,
) -> Result<Response<Body>, StatusCode> {
    host(&v, &id, &h)?;
    Ok(asset(""))
}
async fn file(
    State(v): State<Viewer>,
    Path((id, path)): Path<(String, String)>,
    h: HeaderMap,
) -> Result<Response<Body>, StatusCode> {
    host(&v, &id, &h)?;
    Ok(asset(&format!("assets/{path}")))
}
async fn exchange(
    State(v): State<Viewer>,
    Path(id): Path<String>,
    h: HeaderMap,
    Json(a): Json<Exchange>,
) -> Result<Response<Body>, StatusCode> {
    host(&v, &id, &h)?;
    same_origin(v.manifest.control_address, &h, true)?;
    if a.ticket.len() != 64 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let session = v.sessions.exchange(&a.ticket)?;
    Ok(Response::builder()
        .header(
            "set-cookie",
            format!(
                "{}={session}; HttpOnly; SameSite=Strict; Path=/ui/{id}/",
                cookie(&v)
            ),
        )
        .header("cache-control", "no-store")
        .body(Body::from("{}"))
        .unwrap())
}
async fn authorize(
    State(v): State<Viewer>,
    r: axum::extract::Request,
    next: Next,
) -> axum::response::Response {
    if let Err(s) = same_origin(
        v.manifest.control_address,
        r.headers(),
        r.method() != axum::http::Method::GET || r.uri().path().ends_with("/socket"),
    ) {
        return s.into_response();
    }
    let name = cookie(&v);
    let session = r
        .headers()
        .get("cookie")
        .and_then(|x| x.to_str().ok())
        .and_then(|cs| {
            cs.split(';')
                .find_map(|p| p.trim().strip_prefix(&format!("{name}=")))
        });
    if !session.is_some_and(|s| v.sessions.valid(s)) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut result = next.run(r).await;
    result
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    result
}
async fn websocket(
    State(v): State<Viewer>,
    Path(id): Path<String>,
    h: HeaderMap,
    ws: axum::extract::WebSocketUpgrade,
) -> Result<axum::response::Response, StatusCode> {
    host(&v, &id, &h)?;
    socket::upgrade(
        ws,
        socket::Source::Retained {
            manifest: std::sync::Arc::new(v.manifest),
            path: v.path,
            queries: v.queries,
        },
        v.sockets,
    )
}
pub async fn serve(
    project: std::path::PathBuf,
    name: &str,
    id: Option<&str>,
    json: bool,
    selection: &str,
) -> Result<(), LabError> {
    use std::io::Write;
    let mut manifest = crate::lab::status(project.clone(), name, id)
        .await?
        .manifest;
    if matches!(
        manifest.state,
        crate::lab::Lifecycle::Ready
            | crate::lab::Lifecycle::Starting
            | crate::lab::Lifecycle::Stopping
    ) {
        return Err(LabError::conflict(
            "Retained viewer requires a stopped or interrupted Lab; use ronchi dashboard for a live Lab",
        ));
    }
    let _evidence_lease =
        crate::maintenance::read_lock(&Workspace::new(&project), &manifest.lab_id)?;
    let path = Workspace::new(&project)
        .lab(&manifest.lab_id)?
        .join("journal.sqlite");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| LabError::unavailable("Cannot bind retained viewer"))?;
    let address = listener
        .local_addr()
        .map_err(|_| LabError::internal("Cannot read viewer address"))?;
    manifest.control_address = Some(address);
    let stop = CancellationToken::new();
    let force = CancellationToken::new();
    let v = Viewer {
        manifest,
        path,
        sessions: Sessions::default(),
        queries: query::Engine::default(),
        sockets: Default::default(),
    };
    let ticket = v
        .sessions
        .ticket()
        .map_err(|_| LabError::internal("Cannot create viewer ticket"))?;
    let url = with_selection(
        &format!("http://{address}/ui/{}/#ticket={ticket}", v.manifest.lab_id),
        selection,
    );
    let api = Router::new()
        .route("/ui/{id}/socket", get(websocket))
        .layer(axum::middleware::from_fn_with_state(v.clone(), authorize));
    let router = Router::new()
        .merge(api)
        .route("/ui/{id}/", get(index))
        .route("/ui/{id}/assets/{*path}", get(file))
        .route(
            "/ui/{id}/session",
            post(exchange).layer(axum::extract::DefaultBodyLimit::max(256)),
        )
        .with_state(v.clone());
    let envelope = serde_json::json!({"contract_version":1,"request_id":format!("viewer-{}",std::process::id()),"lab_id":v.manifest.lab_id,"result":{"state":"ready","read_only":true,"launch_url":url,"expires_in_seconds":null,"expires_on":LaunchExpiry::ViewerStop}});
    let ready = if json {
        serde_json::to_string(&envelope).unwrap()
    } else {
        format!("Read-only viewer · {url}")
    };
    writeln!(std::io::stdout().lock(), "{ready}")
        .and_then(|_| std::io::stdout().flush())
        .map_err(|_| LabError::unavailable("Viewer readiness output closed"))?;
    let server = crate::network::serve(
        listener,
        crate::network::Handler::Control(router),
        stop.clone(),
        force.clone(),
    );
    tokio::pin!(server);
    tokio::select! {result=&mut server=>{v.sockets.shutdown().await;return result.map_err(|_|LabError::internal("Retained viewer listener failed"));},_=tokio::signal::ctrl_c()=>{},_=crate::query::cli::stdout_closed()=>{}}
    v.sockets.shutdown().await;
    stop.cancel();
    match tokio::time::timeout(Duration::from_secs(3), &mut server).await {
        Ok(result) => result.map_err(|_| LabError::internal("Retained viewer shutdown failed"))?,
        Err(_) => {
            force.cancel();
            server
                .await
                .map_err(|_| LabError::internal("Retained viewer force shutdown failed"))?;
        }
    }
    Ok(())
}
