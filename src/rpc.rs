//! Bounded JSON-RPC. RPC IDs remain wire values; Journal identities are independent.
pub(crate) mod methods;
use crate::{
    journal::{Event, WriterHandle, WriterHealth},
    simulator::clock::Clock,
};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{Request, Response, body::Incoming};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_BATCH: usize = 64;
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;
pub const BOOTSTRAP_METHODS: [&str; 7] = [
    "getHealth",
    "getVersion",
    "getGenesisHash",
    "getSlot",
    "getBlockHeight",
    "getLatestBlockhash",
    "isBlockhashValid",
];
#[derive(Clone, Debug, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}
impl RpcError {
    pub fn params(message: &str) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }
    pub fn unsupported(message: &str) -> Self {
        Self {
            code: -32000,
            message: message.into(),
            data: Some(json!({"code":"unsupported_capability"})),
        }
    }
    pub(crate) fn internal() -> Self {
        Self {
            code: -32603,
            message: "Journal admission or persistence failed".into(),
            data: Some(json!({"code":"journal_failed"})),
        }
    }
}
fn error(id: Value, error: RpcError) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":error})
}
fn invalid() -> RpcError {
    RpcError {
        code: -32600,
        message: "Invalid Request".into(),
        data: None,
    }
}
/// In-process scheduling seam for deterministic HTTP histories. No remote control path.
#[doc(hidden)]
pub struct RegistrationPause(Arc<PauseState>);
struct PauseState {
    entered: CancellationToken,
    released: CancellationToken,
}
impl RegistrationPause {
    pub async fn registered(&self) {
        self.0.entered.cancelled().await;
    }
    pub fn resume(&self) {
        self.0.released.cancel();
    }
}
impl Drop for RegistrationPause {
    fn drop(&mut self) {
        self.resume();
    }
}
#[derive(Clone)]
pub struct Rpc(Arc<State>);
struct State {
    pub clock: Clock,
    writer: WriterHandle,
    ready: AtomicBool,
    pause: Mutex<Option<Arc<PauseState>>>,
    holds: Arc<crate::cases::HoldPool>,
    policy_status: tokio::sync::watch::Sender<crate::cases::Binding>,
    started: Instant,
    transactions: crate::simulator::transactions::Transactions,
    owner: Mutex<Option<crate::simulator::transactions::Owner>>,
    initialized: tokio::sync::watch::Sender<Option<bool>>,
}
#[derive(Clone)]
pub(crate) struct Exchange {
    rpc: Rpc,
    boundaries: Arc<Mutex<Vec<Value>>>,
    pub(crate) hold: Arc<crate::cases::Hold>,
}
impl Rpc {
    pub fn new(clock: Clock, writer: WriterHandle, started: Instant) -> Self {
        Self::with_state(clock, writer, started, Default::default())
            .expect("empty protocol state is valid")
    }
    pub fn with_state(
        clock: Clock,
        writer: WriterHandle,
        started: Instant,
        state: crate::protocol::state::ProtocolState,
    ) -> Result<Self, String> {
        state.validate()?;
        let (policy_status, _) = tokio::sync::watch::channel(crate::cases::Binding::default());
        let holds = Arc::new(crate::cases::HoldPool::default());
        let (transactions, owner) = crate::simulator::transactions::Transactions::channel(
            clock.clone(),
            writer.clone(),
            started,
            state,
            policy_status.clone(),
            holds.clone(),
        );
        let (initialized, _) = tokio::sync::watch::channel(None);
        Ok(Self(Arc::new(State {
            transactions,
            owner: Mutex::new(Some(owner)),
            initialized,
            clock,
            writer,
            ready: AtomicBool::new(false),
            pause: Default::default(),
            policy_status,
            holds,
            started,
        })))
    }
    pub async fn arm_case(
        &self,
        args: crate::cases::Arm,
    ) -> Result<crate::cases::Binding, RpcError> {
        self.0.transactions.change(Some(args)).await
    }
    pub async fn disarm_case(&self) -> Result<crate::cases::Binding, RpcError> {
        self.0.transactions.change(None).await
    }
    pub fn held_effects(&self) -> Vec<crate::cases::HeldEffect> {
        self.0.holds.snapshot()
    }
    pub fn case_status(&self) -> crate::cases::Binding {
        self.0.policy_status.borrow().clone()
    }
    #[doc(hidden)]
    pub fn pause_next_registration(&self) -> RegistrationPause {
        let state = Arc::new(PauseState {
            entered: CancellationToken::new(),
            released: CancellationToken::new(),
        });
        if let Some(previous) = self
            .0
            .pause
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(state.clone())
        {
            previous.released.cancel();
        }
        RegistrationPause(state)
    }
    pub fn set_ready(&self, ready: bool) {
        self.0.ready.store(ready, Ordering::SeqCst);
    }
    pub fn clock(&self) -> &Clock {
        &self.0.clock
    }
    pub async fn wait_initialized(&self) -> std::io::Result<()> {
        let mut receiver = self.0.initialized.subscribe();
        match receiver.wait_for(|value| value.is_some()).await {
            Ok(value) if *value == Some(true) => Ok(()),
            _ => Err(std::io::Error::other("Simulator initialization failed")),
        }
    }
    pub async fn serve(
        self,
        listener: tokio::net::TcpListener,
        stop: CancellationToken,
        force: CancellationToken,
    ) -> std::io::Result<()> {
        let mut owner = self
            .0
            .owner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .ok_or_else(|| std::io::Error::other("Simulator already has an owner"))?;
        if let Err(error) = owner.initialize().await {
            self.0.initialized.send_replace(Some(false));
            self.0.writer.fail_fatal();
            return Err(std::io::Error::other(error));
        }
        self.0.initialized.send_replace(Some(true));
        let closing = CancellationToken::new();
        let actor_closing = closing.clone();
        let actor_force = force.clone();
        let writer = self.0.writer.clone();
        let mut actor = tokio::spawn(async move {
            let result = owner.run(actor_closing, actor_force).await;
            if result.is_err() {
                writer.fail_fatal();
            }
            result
        });
        let network = crate::network::serve(
            listener,
            crate::network::Handler::Rpc(self.clone()),
            stop.clone(),
            force.clone(),
        );
        tokio::pin!(network);
        let (result, actor_result) = tokio::select! {
            result=&mut network=>(result,None),
            result=&mut actor=>{stop.cancel();force.cancel();(network.await,Some(result))},
        };
        closing.cancel();
        let actor_result = match actor_result {
            Some(result) => result,
            None => actor.await,
        };
        actor_result
            .map_err(std::io::Error::other)?
            .map_err(std::io::Error::other)?;
        result
    }
    pub(crate) fn exchange(&self) -> Exchange {
        Exchange {
            rpc: self.clone(),
            boundaries: Arc::new(Mutex::new(Vec::new())),
            hold: Default::default(),
        }
    }
    async fn record(&self, kind: &str, items: Vec<Value>) -> Result<(), RpcError> {
        if items.is_empty() {
            return Ok(());
        }
        let facts: Vec<Event> = items
            .into_iter()
            .map(|data| crate::lab::event(kind, self.0.started, data))
            .collect();
        let writer = self.0.writer.clone();
        let receipt = tokio::task::spawn_blocking(move || writer.try_append(&facts))
            .await
            .map_err(|_| RpcError::internal())?
            .map_err(|_| RpcError::internal())?;
        receipt
            .committed()
            .await
            .map_err(|_| RpcError::internal())?;
        Ok(())
    }
}
impl Exchange {
    pub(crate) async fn finish(&self, transport: &str) {
        let held = self.hold.close();
        if held {
            self.hold.observed_committed.cancelled().await;
        }
        let values =
            std::mem::take(&mut *self.boundaries.lock().unwrap_or_else(|e| e.into_inner()));
        let values = values
            .into_iter()
            .map(|mut value| {
                value["transport"] = json!(transport);
                if held {
                    value["shared_withholding"] = json!(true);
                    value["prepared_response"] = value["rpc_response"].clone();
                    if value["rpc_response"] != "notification_no_response" {
                        value["rpc_response"] = json!("no_transport_response");
                    }
                }
                value["client_receipt"] = json!("unknown");
                value
            })
            .collect();
        if held {
            let result = self
                .hold
                .record(
                    &self.rpc.0.writer,
                    self.rpc.0.started,
                    "rpc.boundary",
                    values,
                )
                .await;
            if result.is_err() {
                self.rpc.0.writer.fail_fatal();
            }
        } else {
            let _ = self.rpc.record("rpc.boundary", values).await;
        }
        self.hold.release();
    }
    pub(crate) async fn handle(
        &self,
        request: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        let response =
            match tokio::time::timeout(Duration::from_secs(30), self.process(request)).await {
                Ok(response) => response,
                Err(_) => http_response(
                    408,
                    Some(error(
                        Value::Null,
                        RpcError {
                            code: -32000,
                            message: "RPC request deadline exceeded".into(),
                            data: Some(json!({"code":"deadline_exceeded"})),
                        },
                    )),
                ),
            };
        if self.hold.deadline().is_some() {
            std::future::pending::<()>().await;
        }
        Ok(response)
    }
    async fn process(&self, request: Request<Incoming>) -> Response<Full<Bytes>> {
        if request.method() != hyper::Method::POST || request.uri().path() != "/" {
            return http_response(405, Some(error(Value::Null, invalid())));
        }
        if request.headers().get("content-type").is_some_and(|v| {
            !v.to_str()
                .unwrap_or("")
                .split(';')
                .next()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
        }) {
            return http_response(415, Some(error(Value::Null, invalid())));
        }
        let body = match Limited::new(request.into_body(), MAX_REQUEST_BYTES)
            .collect()
            .await
        {
            Ok(body) => body.to_bytes(),
            Err(_) => {
                return http_response(
                    413,
                    Some(error(
                        Value::Null,
                        RpcError {
                            code: -32600,
                            message: "Request body is unavailable or exceeds 64 KiB".into(),
                            data: None,
                        },
                    )),
                );
            }
        };
        let parsed = serde_json::from_slice::<Value>(&body);
        let (batch, items, parse_error) = match parsed {
            Ok(Value::Array(items)) if items.is_empty() || items.len() > MAX_BATCH => {
                return http_response(200, Some(error(Value::Null, invalid())));
            }
            Ok(Value::Array(items)) => (true, items, false),
            Ok(value) => (false, vec![value], false),
            Err(_) => (false, vec![Value::Null], true),
        };
        let binding = match self.rpc.0.transactions.register().await {
            Ok(binding) => binding,
            Err(failure) => return http_response(503, Some(error(Value::Null, failure))),
        };
        let registration = binding.registration_seq;
        let batch_size = items.len();
        let mut prepared = Vec::new();
        let mut responses = Vec::new();
        let mut received = Vec::new();
        for (index, item) in items.into_iter().enumerate() {
            let reference = json!({"registration_seq":registration,"batch_id":if batch{Some(registration)}else{None},"request_id":format!("r{registration}:{index}"),"binding":binding,"responds":item.get("id").is_some(),"batch_size":batch_size});
            let valid = item.is_object()
                && item.get("jsonrpc") == Some(&json!("2.0"))
                && item.get("method").is_some_and(Value::is_string)
                && item
                    .get("id")
                    .is_none_or(|id| id.is_null() || id.is_string() || id.is_number())
                && item
                    .get("params")
                    .is_none_or(|p| p.is_array() || p.is_object());
            let mut registration_fact = reference.clone();
            let method = item.get("method").and_then(Value::as_str).unwrap_or("");
            registration_fact["method"] = json!(if BOOTSTRAP_METHODS.contains(&method)
                || crate::simulator::transactions::TRANSACTION_METHODS.contains(&method)
            {
                method
            } else {
                "unsupported_or_invalid"
            });
            received.push(registration_fact);
            prepared.push((reference, item, valid));
        }
        *self.boundaries.lock().unwrap_or_else(|e| e.into_inner()) = prepared
            .iter()
            .map(|(reference, _, _)| {
                let mut value = reference.clone();
                value["rpc_response"] = json!("not_prepared");
                value
            })
            .collect();
        if let Err(failure) = self.rpc.record("rpc.registered", received).await {
            return failures(&prepared, batch, failure);
        }
        let pause = self
            .rpc
            .0
            .pause
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(pause) = pause {
            pause.entered.cancel();
            pause.released.cancelled().await;
        }
        let mut decisions = Vec::new();
        let mut boundaries = Vec::new();
        for (reference, item, valid) in prepared {
            let id = item.get("id").cloned().unwrap_or(Value::Null);
            let notification = valid && item.get("id").is_none();
            let result = if parse_error {
                Err(RpcError {
                    code: -32700,
                    message: "Parse error".into(),
                    data: None,
                })
            } else if !valid {
                Err(invalid())
            } else if !self.rpc.0.ready.load(Ordering::SeqCst)
                || self.rpc.0.writer.status().health != WriterHealth::Healthy
            {
                Err(RpcError {
                    code: -32005,
                    message: "Local Lab is not ready".into(),
                    data: None,
                })
            } else if crate::simulator::transactions::TRANSACTION_METHODS
                .contains(&item["method"].as_str().unwrap_or(""))
            {
                self.rpc
                    .0
                    .transactions
                    .execute(
                        &self.rpc.0.clock,
                        item["method"].as_str().unwrap_or(""),
                        item.get("params").unwrap_or(&Value::Null),
                        reference.clone(),
                        self.hold.clone(),
                    )
                    .await
            } else {
                methods::execute(
                    &self.rpc.0.clock,
                    item["method"].as_str().unwrap_or(""),
                    item.get("params").unwrap_or(&Value::Null),
                )
            };
            let mut fact = reference.clone();
            let view = self
                .rpc
                .0
                .clock
                .view(crate::simulator::clock::Commitment::Processed);
            fact["simulated_time_ns"] = json!(view.simulated_time_ns);
            fact["slot"] = json!(view.slot);
            fact["block_height"] = json!(view.block_height);
            let mut boundary = reference;
            boundary["rpc_response"] = json!(if notification {
                "notification_no_response"
            } else if result.is_ok() {
                "result"
            } else {
                "error"
            });
            match result {
                Ok(result) => {
                    fact["result"] = result.clone();
                    if !notification {
                        responses.push(json!({"jsonrpc":"2.0","id":id,"result":result}));
                    }
                }
                Err(failure) => {
                    fact["error_code"] = json!(failure.code);
                    if !notification {
                        responses.push(error(if valid { id } else { Value::Null }, failure));
                    }
                }
            }
            decisions.push(fact);
            boundaries.push(boundary);
        }
        if serde_json::to_vec(&responses)
            .expect("JSON values serialize")
            .len()
            > MAX_RESPONSE_BYTES
        {
            let failure = RpcError {
                code: -32000,
                message:
                    "Response exceeds its byte budget; select a smaller batch or binary encoding"
                        .into(),
                data: Some(json!({"code":"result_too_large"})),
            };
            for response in &mut responses {
                *response = error(response["id"].clone(), failure.clone());
            }
            for fact in &mut decisions {
                if let Some(result) = fact
                    .as_object_mut()
                    .and_then(|object| object.remove("result"))
                {
                    fact["source_result"] = result;
                }
                fact["prepared_error_code"] = json!(-32000);
            }
            for boundary in &mut boundaries {
                if boundary["rpc_response"] != "notification_no_response" {
                    boundary["rpc_response"] = json!("error");
                }
            }
        }
        let recorded = if self.hold.deadline().is_some() {
            self.hold
                .record(
                    &self.rpc.0.writer,
                    self.rpc.0.started,
                    "rpc.response_prepared",
                    decisions,
                )
                .await
        } else {
            self.rpc.record("rpc.response_prepared", decisions).await
        };
        if let Err(failure) = recorded {
            self.rpc.0.writer.fail_fatal();
            let responses: Vec<_> = responses
                .into_iter()
                .map(|value| error(value["id"].clone(), failure.clone()))
                .collect();
            return if responses.is_empty() {
                http_response(204, None)
            } else if batch {
                http_response(503, Some(json!(responses)))
            } else {
                http_response(503, responses.into_iter().next())
            };
        }
        *self.boundaries.lock().unwrap_or_else(|e| e.into_inner()) = boundaries;
        if responses.is_empty() {
            http_response(204, None)
        } else if batch {
            http_response(200, Some(Value::Array(responses)))
        } else {
            http_response(200, responses.pop())
        }
    }
}
fn failures(
    prepared: &[(Value, Value, bool)],
    batch: bool,
    failure: RpcError,
) -> Response<Full<Bytes>> {
    let responses: Vec<_> = prepared
        .iter()
        .filter_map(|(_, item, valid)| {
            if *valid && item.get("id").is_none() {
                None
            } else {
                Some(error(
                    if *valid {
                        item["id"].clone()
                    } else {
                        Value::Null
                    },
                    failure.clone(),
                ))
            }
        })
        .collect();
    if responses.is_empty() {
        http_response(204, None)
    } else if batch {
        http_response(503, Some(json!(responses)))
    } else {
        http_response(503, responses.into_iter().next())
    }
}
fn http_response(status: u16, value: Option<Value>) -> Response<Full<Bytes>> {
    let mut bytes = value
        .map(|v| serde_json::to_vec(&v).expect("JSON values serialize"))
        .unwrap_or_default();
    if bytes.len() > MAX_RESPONSE_BYTES {
        bytes = serde_json::to_vec(&error(
            Value::Null,
            RpcError {
                code: -32000,
                message: "Response exceeds its byte budget".into(),
                data: Some(json!({"code":"result_too_large"})),
            },
        ))
        .expect("Static error serializes");
    }
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .header("connection", "close")
        .body(Full::new(Bytes::from(bytes)))
        .expect("Static response headers are valid")
}
