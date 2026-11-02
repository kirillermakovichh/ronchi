//! Listener and connection tasks are owned, bounded, drained, and joined here.
use bytes::Bytes;
use http_body_util::Full;
use hyper::{
    Request, Response,
    body::{Body, Incoming},
    service::{Service, service_fn},
};
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use std::{convert::Infallible, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub enum Handler {
    Control(axum::Router),
    Rpc,
}
async fn unsupported(_: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    Ok(Response::builder().status(501).header("content-type","application/json").body(Full::new(Bytes::from_static(br#"{"error":{"code":"unsupported_capability","message":"RPC methods are not implemented in this build"}}"#))).expect("Static response is valid"))
}
async fn connection<S, B>(
    stream: TcpStream,
    service: S,
    stop: CancellationToken,
    force: CancellationToken,
) where
    S: Service<Request<Incoming>, Response = Response<B>> + 'static,
    S::Future: Send,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: Body<Data = Bytes> + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let mut builder = hyper::server::conn::http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .header_read_timeout(Duration::from_secs(30))
        .max_buf_size(32 * 1024)
        .max_headers(64);
    let future = builder.serve_connection(TokioIo::new(stream), service);
    tokio::pin!(future);
    tokio::select! { _=force.cancelled()=>(), _=&mut future=>(), _=stop.cancelled()=>{
        future.as_mut().graceful_shutdown();
        tokio::select! { _=force.cancelled()=>(), _=&mut future=>() }
    } }
}
pub async fn serve(
    listener: TcpListener,
    handler: Handler,
    stop: CancellationToken,
    force: CancellationToken,
) -> std::io::Result<()> {
    let mut tasks = JoinSet::new();
    let mut failure = None;
    loop {
        tokio::select! {
            _=stop.cancelled()=>break,
            joined=tasks.join_next(), if !tasks.is_empty()=>{ if joined.is_some_and(|r|r.is_err()) { failure=Some(std::io::Error::other("Connection task panicked")); break; } },
            accepted=listener.accept()=>{
                let (stream,_)=match accepted {Ok(pair)=>pair,Err(error)=>{failure=Some(error);break;}};
                if tasks.len()>=64 { drop(stream); continue; }
                let stop=stop.clone(); let force=force.clone(); let handler=handler.clone();
                tasks.spawn(async move { match handler {
                    Handler::Control(router)=>connection(stream,TowerToHyperService::new(router),stop,force).await,
                    Handler::Rpc=>connection(stream,service_fn(unsupported),stop,force).await,
                }});
            }
        }
    }
    drop(listener);
    if failure.is_some() {
        stop.cancel();
        force.cancel();
    }
    while let Some(joined) = tasks.join_next().await {
        if joined.is_err() {
            failure = Some(std::io::Error::other("Connection task panicked"));
            stop.cancel();
            force.cancel();
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }

    Ok(())
}
