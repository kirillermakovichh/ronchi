//! One owner serializes durable identity and time-driven transitions.
//! Only bounded pending liabilities are retained in memory; history stays in SQLite.
use crate::{
    journal::{
        JournalError, JournalReader, TransactionPhase as Phase, TransactionRecord,
        TransitionReservation, WriterHandle,
    },
    protocol::SignedTransaction,
    rpc::RpcError,
    simulator::clock::{Clock, Commitment},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub const TRANSACTION_METHODS: [&str; 4] = [
    "sendTransaction",
    "getSignatureStatuses",
    "getTransaction",
    "getFeeForMessage",
];
const MAX_ACTIVE: usize = 128;
mod custom;
mod status;
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SendConfig {
    #[serde(default)]
    skip_preflight: bool,
    preflight_commitment: Option<Commitment>,
    encoding: Option<String>,
    max_retries: Option<u64>,
    min_context_slot: Option<u64>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StatusConfig {
    #[serde(default)]
    search_transaction_history: bool,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LookupConfig {
    encoding: Option<String>,
    commitment: Option<Commitment>,
    max_supported_transaction_version: Option<u8>,
}

enum Operation {
    Fee(
        crate::protocol::fees::FeeMessage,
        crate::rpc::methods::ContextConfig,
    ),
    Submit(
        SignedTransaction,
        Value,
        std::sync::Arc<crate::cases::Hold>,
        Option<std::sync::Arc<crate::cases::Activation>>,
    ),
    StatusView(Box<status::Read>),
    CustomLookup(Box<status::Read>, Vec<crate::cases::sequence::Selection>),
    ValidateSubmit(SignedTransaction),
    ValidateFee(
        crate::protocol::fees::FeeMessage,
        crate::rpc::methods::ContextConfig,
    ),
    BackendSubmit(
        crate::backend::Client,
        crate::backend::Prepared,
        Value,
        std::sync::Arc<crate::cases::Hold>,
        Option<std::sync::Arc<crate::cases::Activation>>,
    ),
    Register(Vec<Value>, std::sync::Arc<crate::cases::Hold>),
    Change(Option<crate::cases::Arm>),
    Statuses(Vec<String>, bool),
    Lookup(String, LookupConfig),
}
struct Job {
    operation: Operation,
    reply: oneshot::Sender<Result<Reply, RpcError>>,
}
enum Reply {
    Value(Value),
    Registered(Box<crate::cases::Registration>),
}
#[derive(Clone)]
pub struct Transactions {
    sender: mpsc::Sender<Job>,
}
pub(crate) struct Owner {
    receiver: mpsc::Receiver<Job>,
    reader: Option<JournalReader>,
    writer: WriterHandle,
    clock: Clock,
    started: Instant,
    state: crate::protocol::state::ProtocolState,
    binding: crate::cases::Binding,
    activation: Option<std::sync::Arc<crate::cases::Activation>>,
    policy_status: tokio::sync::watch::Sender<crate::cases::Binding>,
    holds: std::sync::Arc<crate::cases::HoldPool>,
    credits: BTreeMap<String, Vec<TransitionReservation>>,
    backend: bool,
    upstream_enabled: bool,
    status_activations: BTreeMap<String, std::sync::Weak<crate::cases::Activation>>,
}
impl Transactions {
    pub(crate) fn channel(
        clock: Clock,
        writer: WriterHandle,
        started: Instant,
        state: crate::protocol::state::ProtocolState,
        policy_status: tokio::sync::watch::Sender<crate::cases::Binding>,
        holds: std::sync::Arc<crate::cases::HoldPool>,
    ) -> (Self, Owner) {
        let (sender, receiver) = mpsc::channel(64);
        (
            Self { sender },
            Owner {
                receiver,
                reader: None,
                writer,
                clock,
                started,
                state,
                binding: Default::default(),
                activation: None,
                policy_status,
                holds,
                credits: BTreeMap::new(),
                backend: false,
                upstream_enabled: false,
                status_activations: BTreeMap::new(),
            },
        )
    }
    pub(crate) async fn execute(
        &self,
        clock: &Clock,
        method: &str,
        params: &Value,
        reference: Value,
        hold: std::sync::Arc<crate::cases::Hold>,
        activation: Option<std::sync::Arc<crate::cases::Activation>>,
    ) -> Result<Value, RpcError> {
        let operation = Self::prepare(clock, method, params, reference, hold, activation).await?;
        self.job(operation).await
    }
    async fn prepare(
        clock: &Clock,
        method: &str,
        params: &Value,
        reference: Value,
        hold: std::sync::Arc<crate::cases::Hold>,
        activation: Option<std::sync::Arc<crate::cases::Activation>>,
    ) -> Result<Operation, RpcError> {
        let args = crate::rpc::methods::positional(params)?;
        let operation = match method {
            "getFeeForMessage" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(RpcError::params(
                        "Expected a base64 message and optional context configuration",
                    ));
                }
                let encoded = args[0]
                    .as_str()
                    .ok_or_else(|| RpcError::params("Expected a base64 message"))?
                    .to_owned();
                let cfg = crate::rpc::methods::config(args.get(1))?;
                let message = tokio::task::spawn_blocking(move || {
                    crate::protocol::fees::FeeMessage::decode(&encoded)
                })
                .await
                .map_err(|_| RpcError::internal())??;
                Operation::Fee(message, cfg)
            }
            "sendTransaction" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(RpcError::params(
                        "Expected a signed transaction and optional configuration",
                    ));
                }
                let encoded = args[0]
                    .as_str()
                    .ok_or_else(|| RpcError::params("Expected an encoded signed transaction"))?;
                let cfg: SendConfig = crate::rpc::methods::config(args.get(1))?;
                if !cfg.skip_preflight {
                    return Err(RpcError::unsupported(
                        "Full preflight requires local execution; configure skipPreflight:true or use Surfpool",
                    ));
                }
                if cfg.max_retries.is_some_and(|n| n > 0) {
                    return Err(RpcError::unsupported(
                        "Positive maxRetries requires a declared leader-rebroadcast model; use zero or omit it",
                    ));
                }
                // Agave uses the processed bank when preflight is skipped.
                let _validated_preflight_commitment = cfg.preflight_commitment;
                let context = clock.view(Commitment::Processed);
                if cfg.min_context_slot.is_some_and(|slot| slot > context.slot) {
                    return Err(RpcError {
                        code: -32016,
                        message: "Minimum context slot has not been reached".into(),
                        data: Some(json!({"contextSlot":context.slot})),
                    });
                }
                let encoded = encoded.to_string();
                let encoding = cfg.encoding.unwrap_or_else(|| "base58".into());
                let signed = tokio::task::spawn_blocking(move || {
                    crate::protocol::decode(&encoded, &encoding)
                })
                .await
                .map_err(|_| RpcError::internal())??;
                Operation::Submit(signed, reference, hold, activation)
            }
            "getSignatureStatuses" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(RpcError::params(
                        "Expected signatures and optional status configuration",
                    ));
                }
                let signatures = args[0]
                    .as_array()
                    .ok_or_else(|| RpcError::params("Expected an array of signatures"))?;
                if signatures.len() > 256 {
                    return Err(RpcError::params(
                        "At most 256 signatures are supported per status query",
                    ));
                }
                let signatures = signatures
                    .iter()
                    .map(crate::protocol::signature)
                    .collect::<Result<Vec<_>, _>>()?;
                let cfg: StatusConfig = crate::rpc::methods::config(args.get(1))?;
                Operation::Statuses(signatures, cfg.search_transaction_history)
            }
            "getTransaction" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(RpcError::params(
                        "Expected a signature and optional transaction configuration",
                    ));
                }
                let signature = crate::protocol::signature(&args[0])?;
                let cfg: LookupConfig = if args.get(1).is_some_and(Value::is_string) {
                    LookupConfig {
                        encoding: args[1].as_str().map(String::from),
                        ..Default::default()
                    }
                } else {
                    crate::rpc::methods::config(args.get(1))?
                };
                if cfg.commitment == Some(Commitment::Processed) {
                    return Err(RpcError::params(
                        "getTransaction requires confirmed or finalized commitment",
                    ));
                }
                if cfg.encoding.as_ref().is_some_and(|s| {
                    !["json", "jsonParsed", "base58", "base64"].contains(&s.as_str())
                }) {
                    return Err(RpcError::params("Unknown transaction encoding"));
                }
                Operation::Lookup(signature, cfg)
            }
            _ => return Err(RpcError::unsupported("Unknown transaction operation")),
        };
        Ok(operation)
    }
    pub(crate) async fn validate_timeout(
        &self,
        clock: &Clock,
        method: &str,
        params: &Value,
    ) -> Result<Vec<String>, RpcError> {
        let prepared =
            Self::prepare(clock, method, params, Value::Null, Default::default(), None).await?;
        match prepared {
            Operation::Submit(signed, _, _, _) => {
                let signature = signed.signature.clone();
                self.job(Operation::ValidateSubmit(signed)).await?;
                Ok(vec![signature])
            }
            Operation::Statuses(signatures, _) => Ok(signatures),
            Operation::Lookup(signature, _) => Ok(vec![signature]),
            Operation::Fee(message, cfg) => {
                self.job(Operation::ValidateFee(message, cfg)).await?;
                Ok(vec![])
            }
            _ => Err(RpcError::internal()),
        }
    }
    pub(crate) async fn backend_submit(
        &self,
        client: crate::backend::Client,
        prepared: crate::backend::Prepared,
        reference: Value,
        hold: std::sync::Arc<crate::cases::Hold>,
        activation: Option<std::sync::Arc<crate::cases::Activation>>,
    ) -> Result<Value, RpcError> {
        self.job(Operation::BackendSubmit(
            client, prepared, reference, hold, activation,
        ))
        .await
    }
    pub(crate) async fn status_view(
        &self,
        method: &str,
        params: &Value,
        source: Value,
        reference: Value,
        activation: std::sync::Arc<crate::cases::Activation>,
        backend: Option<crate::backend::Client>,
    ) -> Result<Value, RpcError> {
        self.job(Operation::StatusView(Box::new(status::Read {
            method: method.into(),
            params: params.clone(),
            source,
            reference,
            activation,
            backend,
        })))
        .await
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Extends the existing source-attributed lookup seam with immutable custom selections"
    )]
    pub(crate) async fn custom_lookup(
        &self,
        method: &str,
        params: &Value,
        source: Value,
        reference: Value,
        activation: std::sync::Arc<crate::cases::Activation>,
        backend: Option<crate::backend::Client>,
        selections: Vec<crate::cases::sequence::Selection>,
    ) -> Result<Value, RpcError> {
        self.job(Operation::CustomLookup(
            Box::new(status::Read {
                method: method.into(),
                params: params.clone(),
                source,
                reference,
                activation,
                backend,
            }),
            selections,
        ))
        .await
    }
    pub(crate) async fn register(
        &self,
        items: Vec<Value>,
        hold: std::sync::Arc<crate::cases::Hold>,
    ) -> Result<crate::cases::Registration, RpcError> {
        match self.ask(Operation::Register(items, hold)).await? {
            Reply::Registered(registration) => Ok(*registration),
            Reply::Value(_) => Err(RpcError::internal()),
        }
    }
    pub(crate) async fn change(
        &self,
        mut args: Option<crate::cases::Arm>,
    ) -> Result<crate::cases::Binding, RpcError> {
        if let Some(args) = &mut args {
            if args.case_id == "provider-timeout-burst" && args.timeout.is_none() {
                args.timeout = Some(Default::default());
            }
            args.validate()?;
        }
        serde_json::from_value(self.job(Operation::Change(args)).await?)
            .map_err(|_| RpcError::internal())
    }
    async fn job(&self, operation: Operation) -> Result<Value, RpcError> {
        match self.ask(operation).await? {
            Reply::Value(value) => Ok(value),
            Reply::Registered(_) => Err(RpcError::internal()),
        }
    }
    async fn ask(&self, operation: Operation) -> Result<Reply, RpcError> {
        let (reply, result) = oneshot::channel();
        self.sender
            .try_send(Job { operation, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => RpcError {
                    code: -32000,
                    message: "Transaction admission queue is full".into(),
                    data: Some(json!({"code":"capacity_exceeded"})),
                },
                mpsc::error::TrySendError::Closed(_) => RpcError::internal(),
            })?;
        result.await.map_err(|_| RpcError::internal())?
    }
}
impl Owner {
    pub(crate) fn enable_upstream(&mut self) {
        self.upstream_enabled = true;
    }
    pub(crate) fn backend_mode(&mut self) {
        self.backend = true;
    }
    pub(crate) async fn initialize(&mut self) -> Result<(), JournalError> {
        let writer = self.writer.clone();
        let reader = tokio::task::spawn_blocking(move || writer.reader())
            .await
            .map_err(|_| JournalError::WriterFailed)??;
        self.reader = Some(reader);
        if self.read(|reader| reader.has_transactions()).await? {
            return Err(JournalError::Closed);
        }
        if !self.backend
            && (!self.state.lookup_tables.is_empty() || !self.state.nonce_accounts.is_empty())
        {
            let fact = crate::lab::event(
                "protocol.fixture",
                self.started,
                serde_json::to_value(&self.state)?,
            );
            self.append(vec![fact]).await?;
        }
        Ok(())
    }
    async fn read<T: Send + 'static>(
        &mut self,
        query: impl FnOnce(&JournalReader) -> Result<T, JournalError> + Send + 'static,
    ) -> Result<T, JournalError> {
        let reader = self.reader.take().ok_or(JournalError::Closed)?;
        let (reader, result) = tokio::task::spawn_blocking(move || {
            let result = query(&reader);
            (reader, result)
        })
        .await
        .map_err(|_| JournalError::WriterFailed)?;
        self.reader = Some(reader);
        result
    }
    async fn append(&mut self, mut events: Vec<crate::journal::Event>) -> Result<(), JournalError> {
        if self.backend {
            for event in &mut events {
                event.execution_mode = crate::config::Mode::Surfpool;
            }
        }
        let writer = self.writer.clone();
        let receipt = tokio::task::spawn_blocking(move || writer.try_append(&events))
            .await
            .map_err(|_| JournalError::WriterFailed)??;
        receipt.committed().await?;
        Ok(())
    }
    async fn blockhash_attempt(
        &mut self,
        signed: &SignedTransaction,
        reference: &Value,
        lifetime: &str,
        eligible: bool,
        source: &str,
        activation: Option<&std::sync::Arc<crate::cases::Activation>>,
    ) -> Result<(), RpcError> {
        let binding: crate::cases::Binding = serde_json::from_value(reference["binding"].clone())
            .map_err(|_| RpcError::internal())?;
        let filtered = binding.policy.as_ref().is_none_or(|policy| {
            policy.settings.signatures.is_empty()
                || policy.settings.signatures.contains(&signed.signature)
        });
        let eligible = eligible && filtered;
        let mut facts = vec![crate::lab::event(
            "transaction.signed_attempt",
            self.started,
            json!({"request":reference,"signature":signed.signature,"blockhash":signed.blockhash,"lifetime":lifetime,"expiry_claim_eligible":eligible,"source_identity":source}),
        )];
        if let Some(active) = activation {
            let now = self.started.elapsed().as_nanos().min(i64::MAX as u128) as u64;
            let responds = eligible && reference["responds"].as_bool().unwrap_or(false);
            let selected = if active.custom.is_some() {
                active
                    .custom_blockhashes
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .values_mut()
                    .flat_map(|cache| {
                        cache.submission(
                            &signed.signature,
                            &signed.blockhash,
                            lifetime,
                            responds,
                            now,
                        )
                    })
                    .collect()
            } else {
                active
                    .blockhash
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .submission(
                        &signed.signature,
                        &signed.blockhash,
                        lifetime,
                        responds,
                        now,
                    )
            };
            for (kind, mut data) in selected {
                data["request"] = reference.clone();
                facts.push(crate::lab::event(kind, self.started, data));
            }
        }
        self.append(facts).await.map_err(|_| RpcError::internal())
    }
    async fn submit(
        &mut self,
        signed: SignedTransaction,
        reference: Value,
        hold: std::sync::Arc<crate::cases::Hold>,
        activation: Option<std::sync::Arc<crate::cases::Activation>>,
    ) -> Result<Value, RpcError> {
        let signature = signed.signature.clone();
        let key = signature.clone();
        let existing = self
            .read(move |reader| reader.transaction(&key))
            .await
            .map_err(|_| RpcError::internal())?;
        if let Some(existing) = existing {
            if existing.digest != signed.digest || existing.wire_base64 != signed.wire_base64 {
                return Err(RpcError::params(
                    "Signed identity conflicts with previously retained content",
                ));
            }
            let source = format!("rpc-simulator:{}", self.clock.genesis_hash());
            self.blockhash_attempt(
                &signed,
                &reference,
                if existing.nonce_context.is_some() {
                    "durable_nonce"
                } else {
                    "recent_blockhash"
                },
                !matches!(
                    existing.stage,
                    Phase::Processed | Phase::Confirmed | Phase::Finalized
                ),
                &source,
                activation.as_ref(),
            )
            .await?;
            let attempt = crate::lab::event(
                "transaction.submission",
                self.started,
                json!({"signature":signature,"digest":signed.digest,"admission":"duplicate","request":reference,"acceptance_ns":existing.accepted_ns}),
            );
            let mut admission = crate::lab::event(
                "rpc.admission_result",
                self.started,
                json!({"request":reference,"signature":signature,"blockhash":signed.blockhash,"source_identity":source,"admission":"signature_returned","processing":"retained_unchanged"}),
            );
            admission.source = "rpc_simulator".into();
            self.append(vec![attempt, admission])
                .await
                .map_err(|_| RpcError::internal())?;
            return Ok(json!(signature));
        }
        let (loaded_addresses, nonce_context) = crate::protocol::resolve_signed(
            &signed.wire_base64,
            &self.state,
            self.clock.view(Commitment::Processed).slot,
        )?;
        if let Some(context) = &nonce_context {
            let key = context.address.clone();
            let account = self
                .read(move |reader| reader.nonce(&key, None))
                .await
                .map_err(|_| RpcError::internal())?
                .ok_or_else(|| {
                    crate::protocol::state::unavailable(
                        "Required nonce account is absent from simulator_state",
                    )
                })?;
            if account.authority != context.authority {
                return Err(RpcError::params(
                    "Nonce authority does not match the signed instruction",
                ));
            }
        }
        if self.credits.len() >= MAX_ACTIVE {
            return Err(RpcError {
                code: -32000,
                message: "Pending lifecycle capacity is exhausted".into(),
                data: Some(json!({"code":"capacity_exceeded"})),
            });
        }
        let binding: crate::cases::Binding = serde_json::from_value(reference["binding"].clone())
            .map_err(|_| RpcError::internal())?;
        let selected =
            binding.selects(&signature, reference["responds"].as_bool().unwrap_or(false));
        if selected {
            hold.reserve(
                &self.holds,
                binding
                    .policy
                    .as_ref()
                    .expect("selected policy")
                    .settings
                    .max_held,
                binding.registration_seq,
                binding.policy.as_ref().expect("selected policy").revision,
            )?;
            hold.reserve_evidence(
                self.writer.clone(),
                reference["batch_size"].as_u64().unwrap_or(1) as usize,
            )
            .await?;
        }
        let writer = self.writer.clone();
        let credits = tokio::task::spawn_blocking(move || {
            (0..3)
                .map(|_| writer.reserve_transition())
                .collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|_| RpcError::internal())?
        .map_err(|_| RpcError::internal())?;
        let accepted = u64::try_from(self.clock.elapsed().as_nanos())
            .ok()
            .filter(|n| *n < i64::MAX as u64 - 6_000_000_000)
            .ok_or_else(|| RpcError::unsupported("Simulator time exceeds the persistence range"))?;
        let hash_height = if nonce_context.is_some() {
            None
        } else {
            self.clock.local_hash_height(&signed.blockhash)
        };
        let last_valid_height =
            hash_height.and_then(|h| h.checked_add(self.clock.profile.validity_window_blocks));
        let expiry_at_ns = last_valid_height
            .and_then(|h| h.checked_sub(self.clock.profile.initial_block_height))
            .and_then(|h| h.checked_add(1))
            .and_then(|h| h.checked_mul(self.clock.profile.slot_duration_ms * 1_000_000));
        let delay = crate::cases::confirmation::selected(
            &binding,
            &signature,
            reference["responds"].as_bool().unwrap_or(false),
        );
        let [process_at_ns, confirmed_at_ns, finalized_at_ns] = if let Some(delay) = &delay {
            delay.deadlines(accepted)?
        } else {
            [
                accepted + 2_000_000_000,
                accepted + 4_000_000_000,
                accepted + 6_000_000_000,
            ]
        };
        let next_due_ns = expiry_at_ns
            .map(|at| at.max(accepted).min(process_at_ns))
            .unwrap_or(process_at_ns);
        let admission_view = self
            .clock
            .view_at(Duration::from_nanos(accepted), Commitment::Processed);
        let pre_expired =
            last_valid_height.is_some_and(|height| admission_view.block_height > height);
        let (process_at_ns, confirmed_at_ns, finalized_at_ns) = if pre_expired {
            (0, 0, 0)
        } else {
            (process_at_ns, confirmed_at_ns, finalized_at_ns)
        };
        let record = TransactionRecord {
            signature: signature.clone(),
            wire_base64: signed.wire_base64,
            digest: signed.digest.clone(),
            version: signed.version,
            blockhash: signed.blockhash,
            hash_height,
            accepted_ns: accepted,
            admission_slot: admission_view.slot,
            admission_height: admission_view.block_height,
            process_at_ns,
            confirmed_at_ns,
            finalized_at_ns,
            last_valid_height,
            expiry_at_ns,
            next_due_ns: if pre_expired { None } else { Some(next_due_ns) },
            stage: if pre_expired {
                Phase::Expired
            } else {
                Phase::Pending
            },
            processed_slot: None,
            processed_height: None,
            loaded_addresses,
            nonce_context,
            admission_request: reference.clone(),
        };
        let status_stale = self.status_admit(&activation, &signature, &reference, accepted)?;
        let source = format!("rpc-simulator:{}", self.clock.genesis_hash());
        let signed_for_attempt = crate::protocol::SignedTransaction {
            signature: signature.clone(),
            wire_base64: record.wire_base64.clone(),
            digest: record.digest.clone(),
            version: record.version,
            blockhash: record.blockhash.clone(),
        };
        self.blockhash_attempt(
            &signed_for_attempt,
            &reference,
            if record.nonce_context.is_some() {
                "durable_nonce"
            } else {
                "recent_blockhash"
            },
            true,
            &source,
            activation.as_ref(),
        )
        .await?;
        let mut data = serde_json::to_value(&record).map_err(|_| RpcError::internal())?;
        data["preflight"] = json!("skipped");
        data["execution"] = json!("not_performed");
        let mut accepted_fact = crate::lab::event("transaction.accepted", self.started, data);
        accepted_fact.source = "rpc_simulator".into();
        let attempt = crate::lab::event(
            "transaction.submission",
            self.started,
            json!({"signature":signature,"digest":signed.digest,"admission":"accepted","request":reference}),
        );
        let mut admission = crate::lab::event(
            "rpc.admission_result",
            self.started,
            json!({"request":reference,"signature":signature,"blockhash":record.blockhash,"source_identity":source,"admission":"signature_returned","processing":if pre_expired{"prevented_by_expiry"}else{"schedule_assigned"}}),
        );
        admission.source = "rpc_simulator".into();
        let mut facts = vec![attempt, accepted_fact, admission];
        if pre_expired {
            let mut prevented = crate::lab::event(
                "transaction.processing_prevented",
                self.started,
                json!({"signature":signature,"request":reference,"reason":"recent_blockhash_expired","blockhash":record.blockhash,"last_valid_height":last_valid_height,"observed_block_height":admission_view.block_height,"authority":"rpc_simulator"}),
            );
            prevented.source = "rpc_simulator".into();
            facts.push(prevented);
        }
        if let Some(delay) = delay.filter(|_| !pre_expired) {
            facts.push(crate::lab::event("fault.confirmation_schedule_assigned", self.started, json!({"signature":signature,"request":reference,"policy":binding.policy,"schedule":delay,"process_at_ns":process_at_ns,"confirmed_at_ns":confirmed_at_ns,"finalized_at_ns":finalized_at_ns,"authority":"rpc_simulator"})));
        }
        if selected {
            facts.push(crate::lab::event("fault.response_loss_selected",self.started,json!({"signature":signature,"request":reference,"policy":binding.policy,"consumed":true})));
        }
        self.append(facts).await.map_err(|_| RpcError::internal())?;
        if !pre_expired {
            if status_stale && let Some(activation) = activation {
                self.status_activations
                    .insert(signature.clone(), std::sync::Arc::downgrade(&activation));
            }
            self.credits.insert(signature.clone(), credits);
        }
        if selected && hold.deadline().is_none() {
            let hold_ms = binding
                .policy
                .as_ref()
                .expect("selected policy")
                .settings
                .hold_ms;
            hold.start(
                &self.writer,
                self.started,
                hold_ms,
                json!({"request":reference,"signature":signature,"hold_ms":hold_ms}),
            )
            .await?;
        }
        Ok(json!(signature))
    }
    async fn backend_submit(
        &mut self,
        client: crate::backend::Client,
        prepared: crate::backend::Prepared,
        reference: Value,
        hold: std::sync::Arc<crate::cases::Hold>,
        activation: Option<std::sync::Arc<crate::cases::Activation>>,
    ) -> Result<Value, RpcError> {
        let signed = prepared.signed.ok_or_else(RpcError::internal)?;
        let signature = signed.signature.clone();
        let key = signature.clone();
        let existing = self
            .read(move |reader| reader.backend_transaction(&key))
            .await
            .map_err(|_| RpcError::internal())?;
        if existing.as_ref().is_some_and(|value| {
            value["digest"] != signed.digest || value["wire_base64"] != signed.wire_base64
        }) {
            return Err(RpcError::params(
                "Signed identity conflicts with retained backend content",
            ));
        }
        let binding: crate::cases::Binding = serde_json::from_value(reference["binding"].clone())
            .map_err(|_| RpcError::internal())?;
        let selected = existing.is_none()
            && binding.selects(&signature, reference["responds"].as_bool().unwrap_or(false));
        if selected {
            let policy = binding.policy.as_ref().ok_or_else(RpcError::internal)?;
            hold.reserve(
                &self.holds,
                policy.settings.max_held,
                binding.registration_seq,
                policy.revision,
            )?;
            hold.reserve_evidence(
                self.writer.clone(),
                reference["batch_size"].as_u64().unwrap_or(1) as usize,
            )
            .await?;
        }
        let status_reserved = if existing.is_none() {
            self.status_admit(
                &activation,
                &signature,
                &reference,
                self.started.elapsed().as_nanos().min(i64::MAX as u128) as u64,
            )?;
            crate::cases::status::matches(&binding, &signature)
        } else {
            false
        };
        let lifetime = crate::protocol::nonce::backend_lifetime(&signed)?;
        self.blockhash_attempt(
            &signed,
            &reference,
            lifetime,
            existing.is_none(),
            client.authority(),
            activation.as_ref(),
        )
        .await?;
        let mut dispatch = crate::lab::event(
            "rpc.backend_started",
            self.started,
            json!({"request":reference,"method":"sendTransaction","signature":signature,"blockhash":signed.blockhash,"authority":"execution_backend","source_identity":client.authority(),"submission_effects":"unknown_until_source_result"}),
        );
        dispatch.execution_mode = crate::config::Mode::Surfpool;
        self.append(vec![dispatch])
            .await
            .map_err(|_| RpcError::internal())?;
        let result = client.call("sendTransaction", &prepared.params).await;
        let mut source = Vec::new();
        for mut data in client.source_facts("sendTransaction", &result, self.started) {
            data["request"] = reference.clone();
            let mut event = crate::lab::event("rpc.backend", self.started, data);
            event.source = "surfpool".into();
            event.execution_mode = crate::config::Mode::Surfpool;
            source.push(event);
        }
        self.append(source)
            .await
            .map_err(|_| RpcError::internal())?;
        let mut admission = crate::lab::event(
            "rpc.admission_result",
            self.started,
            json!({"request":reference,"signature":signature,"blockhash":signed.blockhash,"source_identity":client.authority(),"admission":if result.as_ref().is_ok_and(|value|value==&signature){"signature_returned"}else if result.is_ok(){"invalid_signature_result"}else if result.as_ref().err().and_then(|error|error.data.as_ref()).and_then(|data|data.get("code")).and_then(Value::as_str)==Some("backend_rpc_error"){"backend_rpc_error"}else{"outcome_unknown"},"processing":"backend_status_required","error_code":result.as_ref().err().map(|error|error.code)}),
        );
        admission.source = "surfpool".into();
        self.append(vec![admission])
            .await
            .map_err(|_| RpcError::internal())?;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if status_reserved && let Some(activation) = &activation {
                    activation
                        .status
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&signature);
                }
                return Err(error);
            }
        };
        if result != signature {
            self.writer.fail_fatal();
            return Err(RpcError::internal());
        }
        let accepted_elapsed = self.started.elapsed().as_nanos().min(i64::MAX as u128) as u64;
        if status_reserved && let Some(activation) = &activation {
            activation
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .accepted(&signature, accepted_elapsed);
        }
        let mut attempt = crate::lab::event(
            "transaction.submission",
            self.started,
            json!({"signature":signature,"digest":signed.digest,"admission":if existing.is_some(){"backend_duplicate_response"}else{"backend_accepted"},"request":reference,"authority":"execution_backend"}),
        );
        attempt.execution_mode = crate::config::Mode::Surfpool;
        let mut facts = vec![attempt];
        if existing.is_none() {
            let data = json!({"signature":signature,"digest":signed.digest,"wire_base64":signed.wire_base64,"version":signed.version,"blockhash":signed.blockhash,"admission_request":reference,"accepted_elapsed_ns":accepted_elapsed,"authority":"execution_backend","source_identity":client.authority(),"next_due_ns":null,"execution":"backend_acceptance_only"});
            let mut fact = crate::lab::event("transaction.accepted", self.started, data);
            fact.execution_mode = crate::config::Mode::Surfpool;
            fact.source = "surfpool".into();
            facts.push(fact);
        }
        if selected {
            let mut fact = crate::lab::event(
                "fault.response_loss_selected",
                self.started,
                json!({"signature":signature,"request":reference,"policy":binding.policy,"consumed":true,"authority":"execution_backend"}),
            );
            fact.execution_mode = crate::config::Mode::Surfpool;
            facts.push(fact);
        }
        self.append(facts).await.map_err(|_| RpcError::internal())?;
        if selected && hold.deadline().is_none() {
            hold.start(&self.writer,self.started,binding.policy.as_ref().ok_or_else(RpcError::internal)?.settings.hold_ms,json!({"request":reference,"signature":signature,"authority":"execution_backend","execution_mode":"surfpool"})).await?;
        }
        Ok(result)
    }
    async fn advance_due(
        &mut self,
        until: Duration,
        force: &CancellationToken,
    ) -> Result<Option<u64>, JournalError> {
        if self.backend {
            return Ok(None);
        }
        loop {
            if force.is_cancelled() {
                return Ok(None);
            }
            let Some(mut record) = self.read(|reader| reader.next_transaction()).await? else {
                return Ok(None);
            };
            if force.is_cancelled() {
                return Ok(None);
            }
            let due = record.next_due_ns.ok_or(JournalError::InvalidInput)?;
            if u128::from(due) > until.as_nanos() {
                return Ok(Some(due));
            }
            let mut reason = None;
            let mut nonce_advance = Value::Null;
            match record.stage {
                Phase::Pending => {
                    let view = self
                        .clock
                        .view_at(Duration::from_nanos(due), Commitment::Processed);
                    if let Some(context) = &record.nonce_context {
                        let key = context.address.clone();
                        let account = self.read(move |reader| reader.nonce(&key, None)).await?;
                        let next = crate::protocol::nonce::next_value(
                            &self.clock.blockhash(view.block_height),
                        )
                        .map_err(|_| JournalError::InvalidInput)?;
                        if let Some(account) = account.filter(|account| {
                            account.nonce == record.blockhash
                                && account.authority == context.authority
                                && account.nonce != next
                        }) {
                            nonce_advance = json!({"address":account.address,"authority":account.authority,"previous_nonce":account.nonce,"nonce":next,"slot":view.slot});
                        } else {
                            reason = Some("durable_nonce_invalid_or_not_advanceable");
                        }
                    } else if record
                        .last_valid_height
                        .is_some_and(|height| view.block_height > height)
                    {
                        reason = Some("recent_blockhash_expired");
                    } else if !record.hash_height.is_some_and(|h| h <= view.block_height) {
                        reason = Some("local_blockhash_unavailable");
                    }
                    if let Some(reason) = reason {
                        record.stage = if reason == "recent_blockhash_expired" {
                            Phase::Expired
                        } else {
                            Phase::Unprocessed
                        };
                        record.next_due_ns = None;
                    } else {
                        record.stage = Phase::Processed;
                        record.processed_slot = Some(view.slot);
                        record.processed_height = Some(view.block_height);
                        record.next_due_ns = Some(record.confirmed_at_ns);
                    }
                }
                Phase::Processed => {
                    record.stage = Phase::Confirmed;
                    record.next_due_ns = Some(record.finalized_at_ns);
                }
                Phase::Confirmed => {
                    record.stage = Phase::Finalized;
                    record.next_due_ns = None;
                }
                _ => return Err(JournalError::InvalidInput),
            }
            let mut data = json!({"signature":record.signature,"stage":record.stage,"processed_slot":record.processed_slot,"processed_height":record.processed_height,"next_due_ns":record.next_due_ns,"scheduled_transition_ns":due,"observed_simulated_ns":self.clock.elapsed().as_nanos().min(i64::MAX as u128) as u64,"reason":reason,"nonce_advance":nonce_advance});
            if record.stage == Phase::Processed
                && self.status_activations.contains_key(&record.signature)
            {
                let observed = data["observed_simulated_ns"].as_u64().unwrap_or(0);
                let view = self
                    .clock
                    .view_at(Duration::from_nanos(observed), Commitment::Processed);
                data["source_identity"] = json!(format!("simulator:{}", self.clock.genesis_hash()));
                data["status_view"] = json!({"slot":record.processed_slot,"confirmations":view.block_height.saturating_sub(record.processed_height.unwrap_or(view.block_height)),"err":null,"status":{"Ok":null},"confirmationStatus":"processed"});
            }
            let observation = data.clone();
            let mut fact = crate::lab::event("transaction.transition", self.started, data);
            fact.source = "rpc_simulator".into();
            let reservation = self
                .credits
                .get_mut(&record.signature)
                .and_then(Vec::pop)
                .ok_or(JournalError::InvalidInput)?;
            let writer = self.writer.clone();
            let receipt = tokio::task::spawn_blocking(move || {
                writer.try_append_transition(&fact, reservation)
            })
            .await
            .map_err(|_| JournalError::WriterFailed)??;
            let transition_seq = receipt.committed().await?;
            self.status_transition(&record, transition_seq, &observation)
                .map_err(|_| JournalError::WriterFailed)?;
            if record.next_due_ns.is_none() {
                self.credits.remove(&record.signature);
            }
        }
    }
    async fn query(&mut self, operation: Operation, at: Duration) -> Result<Reply, RpcError> {
        let result = match operation {
            Operation::StatusView(read) => self.status_view(*read).await,
            Operation::BackendSubmit(client, prepared, reference, hold, activation) => {
                self.backend_submit(client, prepared, reference, hold, activation)
                    .await
            }
            Operation::Fee(message, cfg) => {
                let view = self.clock.view_at(at, cfg.commitment.unwrap_or_default());
                if cfg.min_context_slot.is_some_and(|slot| slot > view.slot) {
                    return Err(RpcError {
                        code: -32016,
                        message: "Minimum context slot has not been reached".into(),
                        data: Some(json!({"contextSlot":view.slot})),
                    });
                }
                let fee = message.quote(&self.state, view.slot)?;
                let valid = if let Some(context) = message.nonce_use(&self.state, view.slot)? {
                    let key = context.address;
                    let nonce = self
                        .read(move |reader| reader.nonce(&key, Some(view.slot)))
                        .await
                        .map_err(|_| RpcError::internal())?
                        .ok_or_else(|| {
                            crate::protocol::state::unavailable(
                                "Required nonce account is absent from simulator_state",
                            )
                        })?;
                    nonce.nonce == message.blockhash()
                } else {
                    self.clock.hash_height(&message.blockhash(), view).is_some()
                };
                let value = valid.then_some(fee);
                Ok(json!({"context":{"slot":view.slot},"value":value}))
            }
            Operation::ValidateFee(message, cfg) => {
                let view = crate::rpc::methods::check_context(&self.clock, &cfg)?;
                message.quote(&self.state, view.slot)?;
                if let Some(context) = message.nonce_use(&self.state, view.slot)? {
                    let key = context.address;
                    self.read(move |reader| reader.nonce(&key, Some(view.slot)))
                        .await
                        .map_err(|_| RpcError::internal())?
                        .ok_or_else(|| {
                            crate::protocol::state::unavailable(
                                "Required nonce account is absent from simulator_state",
                            )
                        })?;
                }
                Ok(Value::Null)
            }
            Operation::ValidateSubmit(signed) => {
                let key = signed.signature.clone();
                let existing = self
                    .read(move |reader| reader.transaction(&key))
                    .await
                    .map_err(|_| RpcError::internal())?;
                if let Some(existing) = existing {
                    if existing.digest != signed.digest
                        || existing.wire_base64 != signed.wire_base64
                    {
                        return Err(RpcError::params(
                            "Signed identity conflicts with previously retained content",
                        ));
                    }
                } else {
                    let (_, nonce) = crate::protocol::resolve_signed(
                        &signed.wire_base64,
                        &self.state,
                        self.clock.view(Commitment::Processed).slot,
                    )?;
                    if let Some(context) = nonce {
                        let key = context.address.clone();
                        let account = self
                            .read(move |reader| reader.nonce(&key, None))
                            .await
                            .map_err(|_| RpcError::internal())?
                            .ok_or_else(|| {
                                crate::protocol::state::unavailable(
                                    "Required nonce account is absent from simulator_state",
                                )
                            })?;
                        if account.authority != context.authority {
                            return Err(RpcError::params(
                                "Nonce authority does not match the signed instruction",
                            ));
                        }
                    }
                }
                Ok(Value::Null)
            }
            Operation::Submit(signed, reference, hold, activation) => {
                self.submit(signed, reference, hold, activation).await
            }
            Operation::CustomLookup(read, selections) => {
                self.custom_lookup(*read, &selections).await
            }
            Operation::Register(items, hold) => {
                self.binding.registration_seq = self
                    .binding
                    .registration_seq
                    .checked_add(1)
                    .ok_or_else(RpcError::internal)?;
                let real = self.started.elapsed().as_nanos().min(i64::MAX as u128) as u64;
                let sim = at.as_nanos().min(i64::MAX as u128) as u64;
                let custom = self.custom_register(&items, real, sim, &hold).await;
                return Ok(Reply::Registered(Box::new(crate::cases::Registration {
                    custom,
                    binding: self.binding.clone(),
                    activation: self.activation.clone(),
                    elapsed_ns: self.started.elapsed().as_nanos().min(i64::MAX as u128) as u64,
                    simulated_ns: at.as_nanos().min(i64::MAX as u128) as u64,
                })));
            }
            Operation::Change(mut args) => {
                let custom = if let Some(yaml) = args.as_mut().and_then(|a| a.custom_yaml.take()) {
                    let mode = if self.backend {
                        crate::config::Mode::Surfpool
                    } else {
                        crate::config::Mode::Simulator
                    };
                    let plan = tokio::task::spawn_blocking(move || {
                        crate::cases::custom::validate(&yaml, mode)
                    })
                    .await
                    .map_err(|_| RpcError::internal())?
                    .map_err(|error| error.to_rpc())?;
                    let runtime = crate::cases::sequence::Runtime::new(plan);
                    if let Some(args) = &mut args {
                        args.case_id = runtime.artifact["id"]
                            .as_str()
                            .expect("validated id")
                            .into();
                    }
                    Some(runtime)
                } else {
                    None
                };
                let seq = self
                    .binding
                    .registration_seq
                    .checked_add(1)
                    .ok_or_else(RpcError::internal)?;
                let binding = crate::cases::Binding {
                    registration_seq: seq,
                    policy: args.map(|settings| crate::cases::Policy {
                        revision: seq,
                        activated_at: time::OffsetDateTime::now_utc()
                            .format(&time::format_description::well_known::Rfc3339)
                            .expect("current UTC time formats"),
                        custom: custom.as_ref().map(|r| r.provenance()),
                        activated_elapsed_ns: self
                            .started
                            .elapsed()
                            .as_nanos()
                            .min(i64::MAX as u128)
                            as u64,
                        activated_simulated_ns: at.as_nanos().min(i64::MAX as u128) as u64,
                        settings,
                    }),
                };
                let mut facts = vec![];
                let plan_ref = if let Some(runtime) = &custom {
                    use base64::Engine as _;
                    let bytes =
                        serde_json::to_vec(&runtime.plan).map_err(|_| RpcError::internal())?;
                    let count = bytes.len().div_ceil(16 * 1024);
                    for (index, chunk) in bytes.chunks(16 * 1024).enumerate() {
                        facts.push(crate::lab::event("case.plan",self.started,json!({"policy_revision":seq,"digest":runtime.plan.digest,"chunk":index,"chunks":count,"encoding":"base64-json","payload":base64::engine::general_purpose::STANDARD.encode(chunk)})));
                    }
                    Some(
                        json!({"digest":runtime.plan.digest,"kind":"case.plan","chunks":count,"policy_revision":seq}),
                    )
                } else {
                    None
                };
                facts.push(crate::lab::event(
                    "case.changed",
                    self.started,
                    json!({"binding":binding,"resolved_plan":plan_ref}),
                ));
                self.append(facts).await.map_err(|_| RpcError::internal())?;
                self.binding = binding.clone();
                self.activation = binding.policy.as_ref().map(|_| {
                    std::sync::Arc::new(crate::cases::Activation {
                        custom,
                        ..Default::default()
                    })
                });
                self.policy_status.send_replace(binding.clone());
                serde_json::to_value(binding).map_err(|_| RpcError::internal())
            }
            Operation::Statuses(signatures, history) => {
                let processed = self.clock.view_at(at, Commitment::Processed);
                let finalized = self.clock.view_at(at, Commitment::Finalized);
                self.read(move|reader|{
                    let mut values=Vec::with_capacity(signatures.len());
                    for signature in signatures {
                        let value=reader.transaction(&signature)?.filter(|record|matches!(record.stage,Phase::Processed|Phase::Confirmed|Phase::Finalized)).filter(|r|history||r.stage!=Phase::Finalized||r.processed_slot.is_some_and(|slot|slot>=finalized.slot.saturating_sub(300))).map(|r|{
                            let finalized=r.stage==Phase::Finalized;let confirmations=if finalized{None}else{Some(processed.block_height.saturating_sub(r.processed_height.unwrap_or(processed.block_height)))};
                            json!({"slot":r.processed_slot,"confirmations":confirmations,"err":null,"status":{"Ok":null},"confirmationStatus":r.stage})
                        }).unwrap_or(Value::Null);values.push(value);
                    }
                    Ok(json!({"context":{"slot":processed.slot},"value":values}))
                }).await.map_err(|_|RpcError::internal())
            }
            Operation::Lookup(signature, cfg) => {
                let record = self
                    .read(move |reader| reader.transaction(&signature))
                    .await
                    .map_err(|_| RpcError::internal())?;
                let Some(record) = record.filter(|r| match cfg.commitment.unwrap_or_default() {
                    Commitment::Confirmed => matches!(r.stage, Phase::Confirmed | Phase::Finalized),
                    _ => r.stage == Phase::Finalized,
                }) else {
                    return Ok(Reply::Value(Value::Null));
                };
                let version = crate::protocol::negotiate_version(
                    record.version,
                    cfg.max_supported_transaction_version,
                )?;
                let wire = record.wire_base64.clone();
                let encoding = cfg.encoding.unwrap_or_else(|| "json".into());
                let transaction = tokio::task::spawn_blocking(move || {
                    crate::protocol::encode_lookup(&wire, &encoding, &record.loaded_addresses)
                })
                .await
                .map_err(|_| RpcError::internal())??;
                let mut result = json!({"slot":record.processed_slot,"blockTime":null,"meta":null,"transaction":transaction});
                if let Some(version) = version {
                    result["version"] = version;
                }
                Ok(result)
            }
        };
        result.map(Reply::Value)
    }
    pub(crate) async fn run(
        mut self,
        closing: CancellationToken,
        force: CancellationToken,
    ) -> Result<(), JournalError> {
        let mut stopping = false;
        loop {
            if force.is_cancelled() {
                self.receiver.close();
                break;
            }
            let due = self.advance_due(self.clock.elapsed(), &force).await?;
            if stopping && self.credits.is_empty() && self.receiver.is_empty() {
                break;
            }
            tokio::select! {biased;
                _=force.cancelled()=>{self.receiver.close();break;},
                _=closing.cancelled(),if !stopping=>{stopping=true;self.receiver.close();},
                job=self.receiver.recv(),if !self.receiver.is_closed()||!self.receiver.is_empty()=>{
                    if let Some(job)=job{
                        let at=self.clock.elapsed();self.advance_due(at,&force).await?;
                        if force.is_cancelled(){let _=job.reply.send(Err(RpcError::unsupported("Lab stopped before admission")));self.receiver.close();break;}
                        let result=self.query(job.operation,at).await;
                        let fatal=result.as_ref().is_err_and(|error|error.code==-32603 && error.data.as_ref().and_then(|data|data.get("code")).and_then(Value::as_str)!=Some("backend_rpc_error"));
                        let _=job.reply.send(result);
                        if fatal{self.writer.fail_fatal();return Err(JournalError::WriterFailed);}
                    }else{stopping=true;}
                },
                _=async{if let Some(at)=due{self.clock.wait_until(Duration::from_nanos(at)).await}else{std::future::pending::<()>().await}}=>(),
            }
        }
        while let Ok(job) = self.receiver.try_recv() {
            let _ = job.reply.send(Err(RpcError::unsupported(
                "Lab stopped before queued work was admitted",
            )));
        }
        Ok(())
    }
}
