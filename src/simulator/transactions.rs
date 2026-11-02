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
    Submit(SignedTransaction, Value),
    Statuses(Vec<String>, bool),
    Lookup(String, LookupConfig),
}
struct Job {
    operation: Operation,
    reply: oneshot::Sender<Result<Value, RpcError>>,
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
    credits: BTreeMap<String, Vec<TransitionReservation>>,
}
impl Transactions {
    pub(crate) fn channel(
        clock: Clock,
        writer: WriterHandle,
        started: Instant,
        state: crate::protocol::state::ProtocolState,
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
                credits: BTreeMap::new(),
            },
        )
    }
    pub async fn execute(
        &self,
        clock: &Clock,
        method: &str,
        params: &Value,
        reference: Value,
    ) -> Result<Value, RpcError> {
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
                Operation::Submit(signed, reference)
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
    pub(crate) async fn initialize(&mut self) -> Result<(), JournalError> {
        let writer = self.writer.clone();
        let reader = tokio::task::spawn_blocking(move || writer.reader())
            .await
            .map_err(|_| JournalError::WriterFailed)??;
        self.reader = Some(reader);
        if self.read(|reader| reader.has_transactions()).await? {
            return Err(JournalError::Closed);
        }
        if !self.state.lookup_tables.is_empty() {
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
    async fn append(&mut self, events: Vec<crate::journal::Event>) -> Result<(), JournalError> {
        let writer = self.writer.clone();
        let receipt = tokio::task::spawn_blocking(move || writer.try_append(&events))
            .await
            .map_err(|_| JournalError::WriterFailed)??;
        receipt.committed().await?;
        Ok(())
    }
    async fn submit(
        &mut self,
        signed: SignedTransaction,
        reference: Value,
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
            let attempt = crate::lab::event(
                "transaction.submission",
                self.started,
                json!({"signature":signature,"digest":signed.digest,"admission":"duplicate","request":reference,"acceptance_ns":existing.accepted_ns}),
            );
            self.append(vec![attempt])
                .await
                .map_err(|_| RpcError::internal())?;
            return Ok(json!(signature));
        }
        let loaded_addresses = crate::protocol::resolve_signed(
            &signed.wire_base64,
            &self.state,
            self.clock.view(Commitment::Processed).slot,
        )?;
        if self.credits.len() >= MAX_ACTIVE {
            return Err(RpcError {
                code: -32000,
                message: "Pending lifecycle capacity is exhausted".into(),
                data: Some(json!({"code":"capacity_exceeded"})),
            });
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
        let hash_height = self.clock.local_hash_height(&signed.blockhash);
        let last_valid_height =
            hash_height.and_then(|h| h.checked_add(self.clock.profile.validity_window_blocks));
        let expiry_at_ns = last_valid_height
            .and_then(|h| h.checked_sub(self.clock.profile.initial_block_height))
            .and_then(|h| h.checked_add(1))
            .and_then(|h| h.checked_mul(self.clock.profile.slot_duration_ms * 1_000_000));
        let process_at_ns = accepted + 2_000_000_000;
        let next_due_ns = expiry_at_ns
            .map(|at| at.max(accepted).min(process_at_ns))
            .unwrap_or(process_at_ns);
        let admission_view = self
            .clock
            .view_at(Duration::from_nanos(accepted), Commitment::Processed);
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
            confirmed_at_ns: accepted + 4_000_000_000,
            finalized_at_ns: accepted + 6_000_000_000,
            last_valid_height,
            expiry_at_ns,
            next_due_ns: Some(next_due_ns),
            stage: Phase::Pending,
            processed_slot: None,
            processed_height: None,
            loaded_addresses,
            admission_request: reference.clone(),
        };
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
        self.append(vec![attempt, accepted_fact])
            .await
            .map_err(|_| RpcError::internal())?;
        self.credits.insert(signature.clone(), credits);
        Ok(json!(signature))
    }
    async fn advance_due(
        &mut self,
        until: Duration,
        force: &CancellationToken,
    ) -> Result<Option<u64>, JournalError> {
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
            match record.stage {
                Phase::Pending => {
                    let view = self
                        .clock
                        .view_at(Duration::from_nanos(due), Commitment::Processed);
                    if record
                        .last_valid_height
                        .is_some_and(|height| view.block_height > height)
                    {
                        record.stage = Phase::Expired;
                        record.next_due_ns = None;
                        reason = Some("recent_blockhash_expired");
                    } else if !record.hash_height.is_some_and(|h| h <= view.block_height) {
                        record.stage = Phase::Unprocessed;
                        record.next_due_ns = None;
                        reason = Some("local_blockhash_unavailable");
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
            let data = json!({"signature":record.signature,"stage":record.stage,"processed_slot":record.processed_slot,"processed_height":record.processed_height,"next_due_ns":record.next_due_ns,"scheduled_transition_ns":due,"observed_simulated_ns":self.clock.elapsed().as_nanos().min(i64::MAX as u128) as u64,"reason":reason});
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
            receipt.committed().await?;
            if record.next_due_ns.is_none() {
                self.credits.remove(&record.signature);
            }
        }
    }
    async fn query(&mut self, operation: Operation, at: Duration) -> Result<Value, RpcError> {
        match operation {
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
                let value = self
                    .clock
                    .hash_height(&message.blockhash(), view)
                    .map(|_| fee);
                Ok(json!({"context":{"slot":view.slot},"value":value}))
            }
            Operation::Submit(signed, reference) => self.submit(signed, reference).await,
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
                    return Ok(Value::Null);
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
        }
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
                        let fatal=result.as_ref().is_err_and(|error|error.code==-32603);
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
