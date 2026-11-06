//! Browser-only server-push transport. The Journal and shared core remain authoritative.
mod operations;
use crate::{control::Control, lab::Manifest};
use axum::{
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{Semaphore, watch},
    task::{AbortHandle, JoinSet},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
const MAX_SUBSCRIPTIONS: usize = 16;
const MAX_JOBS: usize = 2;
const MAX_CALLS: usize = 4;
const MAX_FRAME: usize = 401 * 1024;
const MAX_SEND: usize = 320 * 1024;
#[derive(Clone)]
pub(crate) struct Hub(Arc<HubState>);
struct HubState {
    slots: Arc<Semaphore>,
    readers: Arc<Semaphore>,
    changed: watch::Sender<u64>,
    stop: CancellationToken,
    tasks: TaskTracker,
}
impl Default for Hub {
    fn default() -> Self {
        let (changed, _) = watch::channel(0);
        Self(Arc::new(HubState {
            slots: Arc::new(Semaphore::new(8)),
            readers: Arc::new(Semaphore::new(2)),
            changed,
            stop: CancellationToken::new(),
            tasks: TaskTracker::new(),
        }))
    }
}
impl Hub {
    pub fn notify(&self) {
        self.0.changed.send_modify(|n| *n = n.wrapping_add(1));
    }
    pub async fn shutdown(&self) {
        self.0.stop.cancel();
        self.0.tasks.close();
        self.0.tasks.wait().await;
    }
}
#[derive(Clone)]
pub(crate) enum Source {
    Live(Control),
    Retained {
        manifest: Arc<Manifest>,
        path: PathBuf,
        queries: crate::query::Engine,
    },
}
impl Source {
    fn snapshot(&self) -> Manifest {
        match self {
            Self::Live(control) => control.snapshot(),
            Self::Retained { manifest, .. } => (**manifest).clone(),
        }
    }
    fn writer(&self) -> Option<watch::Receiver<crate::journal::WriterStatus>> {
        match self {
            Self::Live(c) => Some(c.writer.subscribe()),
            _ => None,
        }
    }
}
pub(crate) async fn live(
    State(control): State<Control>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    crate::control::authorize(&control, &id, &headers)?;
    let hub = control.sockets.clone();
    upgrade(ws, Source::Live(control), hub)
}
pub(crate) fn upgrade(
    ws: WebSocketUpgrade,
    source: Source,
    hub: Hub,
) -> Result<Response, StatusCode> {
    let permit = hub
        .0
        .slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if hub.0.stop.is_cancelled() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    // Track the upgrade callback before returning the handshake. Shutdown also
    // waits for an accepted upgrade whose callback has not started yet.
    let ownership = hub.0.tasks.token();
    Ok(ws
        .max_message_size(MAX_FRAME)
        .max_frame_size(MAX_FRAME)
        .write_buffer_size(16 * 1024)
        .max_write_buffer_size(512 * 1024)
        .on_upgrade(move |socket| async move {
            let _ownership = ownership;
            let _permit = permit;
            session(socket, source, hub).await;
        }))
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Incoming {
    Call {
        id: String,
        path: String,
        #[serde(default)]
        body: Value,
    },
    Subscribe {
        id: String,
        path: String,
        #[serde(default)]
        body: Value,
    },
    Unsubscribe {
        id: String,
    },
    Cancel {
        id: String,
    },
    Visibility {
        visible: bool,
    },
}
#[derive(Clone)]
struct Spec {
    path: String,
    body: Value,
}
struct Subscription {
    spec: Spec,
    epoch: u64,
    dirty: bool,
    last_run: u64,
    digest: Option<String>,
}
struct Running {
    abort: AbortHandle,
    cancel: CancellationToken,
    subscription: Option<u64>,
    mutation: bool,
    generation: u64,
}
struct ResultFrame {
    generation: u64,
    id: String,
    epoch: Option<u64>,
    status: u16,
    value: Value,
}
fn read_only(path: &str, _body: &Value) -> bool {
    path == "api/status" || path == "api/cases" || path == "api/investigate"
}
fn live_query(spec: &Spec) -> bool {
    spec.path != "api/investigate" || spec.body["arguments"]["snapshot_seq"].is_null()
}
async fn changed<T: Send + Sync>(receiver: &mut Option<watch::Receiver<T>>) {
    if let Some(r) = receiver {
        if r.changed().await.is_err() {
            *receiver = None;
        }
    } else {
        std::future::pending::<()>().await;
    }
}
async fn send(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    value: Value,
) -> bool {
    let Ok(text) = serde_json::to_string(&value) else {
        return false;
    };
    if text.len() > MAX_SEND {
        return false;
    }
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            sink.send(Message::Text(text.into()))
        )
        .await,
        Ok(Ok(()))
    )
}
async fn session(socket: WebSocket, source: Source, hub: Hub) {
    let (mut sink, mut stream) = socket.split();
    let mut writer = source.writer();
    let mut runtime = Some(hub.0.changed.subscribe());
    let mut subscriptions: BTreeMap<String, Subscription> = BTreeMap::new();
    let mut running: BTreeMap<String, Running> = BTreeMap::new();
    let mut pending: VecDeque<(String, Spec)> = VecDeque::new();
    let mut jobs: JoinSet<ResultFrame> = JoinSet::new();
    let mut epoch = 0u64;
    let mut clock = 0u64;
    let mut visible = true;
    if !send(
        &mut sink,
        json!({"type":"ready","contract_version":1,"lab_id":source.snapshot().lab_id}),
    )
    .await
    {
        return;
    }
    loop {
        while running.len() < MAX_JOBS && jobs.len() < MAX_JOBS {
            let work = if let Some((id, spec)) = pending.pop_front() {
                Some((id, spec, None))
            } else if visible {
                let next = subscriptions
                    .iter()
                    .filter(|(id, s)| s.dirty && !running.contains_key(*id))
                    .min_by_key(|(_, s)| s.last_run)
                    .map(|(id, _)| id.clone());
                next.map(|id| {
                    let s = subscriptions
                        .get_mut(&id)
                        .expect("selected subscription exists");
                    clock = clock.wrapping_add(1);
                    s.dirty = false;
                    s.last_run = clock;
                    (id, s.spec.clone(), Some(s.epoch))
                })
            } else {
                None
            };
            let Some((id, spec, version)) = work else {
                break;
            };
            let cancel = hub.0.stop.child_token();
            let guard = cancel.clone().drop_guard();
            let task_source = source.clone();
            let task_id = id.clone();
            let mutation = !read_only(&spec.path, &spec.body);
            let token = cancel.clone();
            clock = clock.wrapping_add(1);
            let generation = clock;
            let notifications = hub.clone();
            let readers = hub.0.readers.clone();
            let query = spec.path == "api/investigate";
            let abort = jobs.spawn(async move {
                let _guard = guard;
                let operation = async {
                    // Waiters are bounded by socket/job caps. Leave capacity in
                    // the shared core reader pool for agent requests.
                    let _permit = if query {Some(readers.acquire_owned().await.expect("reader semaphore remains open"))}else{None};
                    task_source.dispatch(spec, token).await
                };
                let (status, value) = match tokio::time::timeout(Duration::from_secs(30), operation).await {
                    Ok(reply) => reply,
                    Err(_) => (408, json!({"error":{"code":"deadline_exceeded","message":"Browser operation deadline exceeded","effects":if mutation {"unknown"}else{"none"}}})),
                };
                if mutation {notifications.notify();}
                ResultFrame {
                    generation,
                    id: task_id,
                    epoch: version,
                    status,
                    value,
                }
            });
            running.insert(
                id,
                Running {
                    abort,
                    cancel,
                    subscription: version,
                    mutation,
                    generation,
                },
            );
        }
        tokio::select! {
         _=hub.0.stop.cancelled()=>{
          let final_status=async {
           for(id,s)in &subscriptions {if s.spec.path=="api/status" && !send(&mut sink,json!({"type":"update","id":id,"status":200,"value":source.snapshot()})).await {break;}}
          };
          let _=tokio::time::timeout(Duration::from_secs(2),final_status).await;
          break;
         },
         input=stream.next()=>{
          let Some(Ok(message))=input else{break;};
          let Message::Text(text)=message else{if matches!(message,Message::Close(_)){break;}continue;};
          let Ok(input)=serde_json::from_str::<Incoming>(&text) else {if !send(&mut sink,json!({"type":"protocol_error","message":"Invalid WebSocket message"})).await{break;}continue;};
          let subscribe=matches!(&input,Incoming::Subscribe{..});
          match input {
           Incoming::Visibility{visible:value}=>{visible=value;if visible{for s in subscriptions.values_mut(){if live_query(&s.spec){s.dirty=true;}}}},
           Incoming::Cancel{id}|Incoming::Unsubscribe{id}=>{
            subscriptions.remove(&id);pending.retain(|(key,_)|key!=&id);
            if running.get(&id).is_some_and(|job|!job.mutation) && let Some(job)=running.remove(&id){job.cancel.cancel();job.abort.abort();}
           },
           Incoming::Call{id,path,body}|Incoming::Subscribe{id,path,body}=>{
            let valid=crate::workspace::identifier(&id)&&path.len()<=192&&read_size(&path,&body)&&!subscriptions.contains_key(&id)&&!running.contains_key(&id)&&!pending.iter().any(|(key,_)|key==&id);
            let capacity=if subscribe{subscriptions.len()<MAX_SUBSCRIPTIONS&&!subscriptions.contains_key(&id)&&read_only(&path,&body)&&body["arguments"]["include_payload"]!=true}else{pending.len()<MAX_CALLS};
            if !valid||!capacity {
             if !send(&mut sink,json!({"type":if subscribe{"update"}else{"reply"},"id":id,"status":400,"value":{"error":{"code":if valid{"capacity_exceeded"}else{"invalid_argument"},"effects":"none","message":"Invalid or over-capacity browser operation"}}})).await{break;}
             continue;
            }
            let spec=Spec{path,body};
            if subscribe{epoch=epoch.wrapping_add(1);subscriptions.insert(id,Subscription{spec,epoch,dirty:true,last_run:0,digest:None});}else{pending.push_back((id,spec));}
           }
          }
         },
         result=jobs.join_next(),if !jobs.is_empty()=>{
          match result {
           Some(Ok(frame))=>{
            if running.get(&frame.id).is_none_or(|r|r.subscription!=frame.epoch || r.generation!=frame.generation){continue;}
            running.remove(&frame.id);
            if let Some(version)=frame.epoch {
             let Some(s)=subscriptions.get_mut(&frame.id).filter(|s|s.epoch==version) else{continue;};
             let digest=crate::query::digest(&serde_json::to_vec(&(frame.status,&frame.value)).unwrap_or_default());
             if s.digest.as_ref()==Some(&digest){continue;}s.digest=Some(digest);
            }
            if !send(&mut sink,json!({"type":if frame.epoch.is_some(){"update"}else{"reply"},"id":frame.id,"status":frame.status,"value":frame.value})).await{break;}
           },
           Some(Err(error)) if error.is_cancelled()=>{},
           _=>break,
          }
         },
         _=changed(&mut writer)=>{if visible{for s in subscriptions.values_mut(){if live_query(&s.spec){s.dirty=true;}}}},
         _=changed(&mut runtime)=>{if visible{for s in subscriptions.values_mut(){if live_query(&s.spec){s.dirty=true;}}}},
        }
    }
    for job in running.values() {
        job.cancel.cancel();
    }
    jobs.abort_all();
    while jobs.join_next().await.is_some() {}
    let _ = tokio::time::timeout(Duration::from_millis(250), sink.send(Message::Close(None))).await;
}
fn read_size(path: &str, body: &Value) -> bool {
    let max = if matches!(path, "api/case/arm" | "api/case/validate") {
        400 * 1024
    } else {
        crate::query::MAX_REQUEST_BYTES
    };
    crate::query::encoded_len(body, max).is_ok()
}
