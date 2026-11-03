//! One Lab-owned supervisor owns all finite collection tasks and cancellation tokens.
use super::{
    Config, Context, Error, InvocationOutcome, InvocationResult, Scope, transport::Runtime,
};
use crate::query::EventRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{Semaphore, mpsc, oneshot, watch},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Arguments {
    pub collection_id: String,
    pub check_id: String,
    pub scope: Scope,
    #[serde(default)]
    pub poll: bool,
    pub horizon_seconds: Option<u32>,
}
impl Arguments {
    pub fn normalize(&mut self) -> Result<(), Error> {
        self.scope.normalize().map_err(|_| Error::invalid())?;
        if !crate::workspace::identifier(&self.collection_id)
            || !crate::workspace::identifier(&self.check_id)
            || self
                .horizon_seconds
                .is_some_and(|n| !self.poll || !(1..=300).contains(&n))
        {
            return Err(Error::invalid());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub contract_version: u32,
    pub operation: String,
    pub request_id: String,
    pub lab_id: String,
    pub arguments: Arguments,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub collection_id: String,
    pub check_id: String,
    pub version: u32,
    pub configuration_digest: String,
    pub lab_id: String,
    pub scope: Scope,
    pub status: String,
    pub invocations: u32,
    pub observations: u32,
    pub gaps: u32,
    pub skipped: u32,
    pub snapshot_seq: u64,
    pub start_ref: Option<EventRef>,
    pub completion_ref: Option<EventRef>,
    pub last_invocation: Option<InvocationResult>,
}
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Reply {
    pub contract_version: u32,
    pub request_id: String,
    pub lab_id: String,
    pub result: Report,
}
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Failure {
    pub contract_version: u32,
    pub request_id: String,
    pub lab_id: Option<String>,
    pub collection_id: Option<String>,
    pub error: Error,
}
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
pub enum Envelope {
    Success(Box<Reply>),
    Failure(Box<Failure>),
}
enum Command {
    Start(Arguments, oneshot::Sender<Result<Report, Error>>),
    Get(String, oneshot::Sender<Result<Report, Error>>),
    Cancel(String, oneshot::Sender<Result<Report, Error>>),
}
#[derive(Clone)]
pub struct Client {
    sender: mpsc::Sender<Command>,
    stop: CancellationToken,
    context: Context,
}
pub struct Owner {
    task: Option<tokio::task::JoinHandle<Result<(), Error>>>,
    stop: CancellationToken,
}
struct Entry {
    digest: String,
    key: String,
    cancel: CancellationToken,
    report: watch::Receiver<Report>,
}
impl Client {
    async fn ask(
        &self,
        f: impl FnOnce(oneshot::Sender<Result<Report, Error>>) -> Command,
    ) -> Result<Report, Error> {
        let (tx, rx) = oneshot::channel();
        self.sender.try_send(f(tx)).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => Error::new("capacity_exceeded", "none"),
            mpsc::error::TrySendError::Closed(_) => Error::journal(),
        })?;
        rx.await.map_err(|_| Error::journal())?
    }
    pub async fn start(&self, mut args: Arguments) -> Result<Report, Error> {
        args.normalize()?;
        if self.stop.is_cancelled() {
            return Err(Error::new("state_conflict", "none"));
        }
        self.ask(|reply| Command::Start(args, reply)).await
    }
    pub async fn status(&self, id: String) -> Result<Report, Error> {
        if self.sender.is_closed() {
            return retained(&self.context, id)
                .await?
                .map(|(_, report)| report)
                .ok_or_else(|| Error::new("collection_unavailable", "none"));
        }
        self.ask(|reply| Command::Get(id, reply)).await
    }
    pub async fn cancel(&self, id: String) -> Result<Report, Error> {
        if self.sender.is_closed() {
            return self.status(id).await;
        }
        self.ask(|reply| Command::Cancel(id, reply)).await
    }
    pub fn stop(&self) {
        self.stop.cancel();
    }
}
impl Owner {
    pub async fn settle(mut self, deadline: tokio::time::Instant) -> Result<(), Error> {
        self.stop.cancel();
        let mut task = self.task.take().ok_or_else(Error::journal)?;
        match tokio::time::timeout_at(deadline, &mut task).await {
            Ok(result) => result.map_err(|_| Error::journal())?,
            Err(_) => {
                task.abort();
                let _ = task.await;
                Err(Error::journal())
            }
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
struct FailureGuard {
    writer: crate::journal::WriterHandle,
    clean: bool,
}
impl Drop for FailureGuard {
    fn drop(&mut self) {
        if !self.clean {
            self.writer.fail_fatal();
        }
    }
}
pub fn spawn(context: Context, config: Config) -> Result<(Client, Owner), Error> {
    config.validate().map_err(|_| Error::invalid())?;
    let mut sources = BTreeMap::new();
    for definition in config.sources {
        sources.insert(
            definition.check_id.clone(),
            Arc::new(Runtime::new(definition)?),
        );
    }
    let (sender, receiver) = mpsc::channel(32);
    let stop = CancellationToken::new();
    let task_stop = stop.clone();
    let client_context = context.clone();
    let task = tokio::spawn(async move {
        let mut guard = FailureGuard {
            writer: context.writer.clone(),
            clean: false,
        };
        let result = supervise(context, sources, config.concurrency, receiver, task_stop).await;
        guard.clean = result.is_ok();
        result
    });
    Ok((
        Client {
            sender,
            stop: stop.clone(),
            context: client_context,
        },
        Owner {
            task: Some(task),
            stop,
        },
    ))
}
async fn retained(context: &Context, id: String) -> Result<Option<(String, Report)>, Error> {
    let writer = context.writer.clone();
    tokio::task::spawn_blocking(move || {
        writer
            .reader()
            .map_err(|_| Error::journal())?
            .check_collection(&id)
            .map_err(|_| Error::journal())
    })
    .await
    .map_err(|_| Error::journal())?
}
async fn supervise(
    context: Context,
    sources: BTreeMap<String, Arc<Runtime>>,
    concurrency: u8,
    mut commands: mpsc::Receiver<Command>,
    stop: CancellationToken,
) -> Result<(), Error> {
    let slots = Arc::new(Semaphore::new(usize::from(concurrency)));
    let mut tasks = JoinSet::new();
    let mut active: BTreeMap<String, Entry> = BTreeMap::new();
    let mut completed: VecDeque<(String, String, Report)> = VecDeque::new();
    loop {
        tokio::select! {
            biased;
            _=stop.cancelled()=>break,
            joined=tasks.join_next(),if !tasks.is_empty()=>{
                let Some(Ok((id,result)))=joined else{context.writer.fail_fatal();return Err(Error::journal());};
                let entry=active.remove(&id).ok_or_else(Error::journal)?;
                match result{Ok(report)=>{completed.push_back((id,entry.digest,report));if completed.len()>128{completed.pop_front();}},Err(error)=>{context.writer.fail_fatal();stop.cancel();while tasks.join_next().await.is_some(){}return Err(error);}}
            }
            command=commands.recv()=>{
                let Some(command)=command else{break;};
                match command{
                    Command::Get(id,reply)=>{
                        let result=if let Some(entry)=active.get(&id){Ok(entry.report.borrow().clone())}else if let Some((_,_,report))=completed.iter().find(|(key,_,_)|key==&id){Ok(report.clone())}else{retained(&context,id).await?.map(|(_,report)|report).ok_or_else(||Error::new("collection_unavailable","none"))};
                        let _=reply.send(result);
                    }
                    Command::Cancel(id,reply)=>{
                        if let Some(entry)=active.get(&id){entry.cancel.cancel();let _=reply.send(Ok(entry.report.borrow().clone()));}
                        else{let prior=completed.iter().find(|(key,_,_)|key==&id).map(|(_,_,report)|report.clone());let result=if prior.is_some(){prior}else{retained(&context,id).await?.map(|(_,report)|report)}.ok_or_else(||Error::new("collection_unavailable","none"));let _=reply.send(result);}
                    }
                    Command::Start(args,reply)=>{
                        let result=start(&context,&sources,&slots,&stop,StartState{active:&mut active,tasks:&mut tasks,completed:&completed},args).await;
                        let _=reply.send(result);
                    }
                }
            }
        }
    }
    stop.cancel();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((_, Ok(_))) => (),
            _ => {
                context.writer.fail_fatal();
                return Err(Error::journal());
            }
        }
    }
    Ok(())
}
struct StartState<'a> {
    active: &'a mut BTreeMap<String, Entry>,
    tasks: &'a mut JoinSet<(String, Result<Report, Error>)>,
    completed: &'a VecDeque<(String, String, Report)>,
}
async fn start(
    context: &Context,
    sources: &BTreeMap<String, Arc<Runtime>>,
    slots: &Arc<Semaphore>,
    stop: &CancellationToken,
    state: StartState<'_>,
    args: Arguments,
) -> Result<Report, Error> {
    let StartState {
        active,
        tasks,
        completed,
    } = state;
    if stop.is_cancelled() {
        return Err(Error::new("state_conflict", "none"));
    }
    let runtime = sources
        .get(&args.check_id)
        .ok_or_else(Error::invalid)?
        .clone();
    let digest = crate::query::digest(
        &serde_json::to_vec(&(&args, &runtime.definition)).map_err(|_| Error::invalid())?,
    );
    if let Some(entry) = active.get(&args.collection_id) {
        return if entry.digest == digest {
            Ok(entry.report.borrow().clone())
        } else {
            Err(Error::new("state_conflict", "none"))
        };
    }
    let prior = completed
        .iter()
        .find(|(id, _, _)| id == &args.collection_id)
        .map(|(_, digest, report)| (digest.clone(), report.clone()));
    if let Some((old, report)) = if prior.is_some() {
        prior
    } else {
        retained(context, args.collection_id.clone()).await?
    } {
        return if old == digest {
            Ok(report)
        } else {
            Err(Error::new("state_conflict", "none"))
        };
    }
    let key = crate::query::digest(
        &serde_json::to_vec(&(&args.check_id, &args.scope)).map_err(|_| Error::invalid())?,
    );
    if active.values().any(|entry| entry.key == key) {
        return Err(Error::new("state_conflict", "none"));
    }
    if active.len() >= 16 {
        return Err(Error::new("capacity_exceeded", "none"));
    }
    let initial_permit = if args.poll {
        None
    } else {
        Some(
            slots
                .clone()
                .try_acquire_owned()
                .map_err(|_| Error::new("capacity_exceeded", "none"))?,
        )
    };
    let reservation = context.reserve().await?;
    let mut report = Report {
        collection_id: args.collection_id.clone(),
        check_id: args.check_id.clone(),
        version: runtime.definition.version,
        configuration_digest: crate::query::digest(
            &serde_json::to_vec(&runtime.definition).map_err(|_| Error::invalid())?,
        ),
        lab_id: context.lab_id.clone(),
        scope: args.scope.clone(),
        status: "running".into(),
        invocations: 0,
        observations: 0,
        gaps: 0,
        skipped: 0,
        snapshot_seq: context.writer.status().snapshot_seq,
        start_ref: None,
        completion_ref: None,
        last_invocation: None,
    };
    let first=context.append(context.event("check.collection_started","ronchi",json!({"request_id":args.collection_id,"arguments_digest":digest,"arguments":args,"definition":runtime.definition.public_definition()})),None).await?;
    report.start_ref = Some(EventRef {
        lab_id: context.lab_id.clone(),
        event_seq: first,
    });
    report.snapshot_seq = first;
    let (tx, rx) = watch::channel(report.clone());
    let cancel = stop.child_token();
    active.insert(
        args.collection_id.clone(),
        Entry {
            digest: digest.clone(),
            key,
            cancel: cancel.clone(),
            report: rx,
        },
    );
    let context = context.clone();
    let slots = slots.clone();
    let initial = report.clone();
    tasks.spawn(async move {
        let id = args.collection_id.clone();
        let result = run(Collection {
            context,
            runtime,
            args,
            slots,
            cancel,
            updates: tx,
            report,
            reservation,
            digest,
            initial_permit,
        })
        .await;
        (id, result)
    });
    Ok(initial)
}
struct Collection {
    context: Context,
    runtime: Arc<Runtime>,
    args: Arguments,
    slots: Arc<Semaphore>,
    cancel: CancellationToken,
    updates: watch::Sender<Report>,
    report: Report,
    reservation: crate::journal::TransitionReservation,
    digest: String,
    initial_permit: Option<tokio::sync::OwnedSemaphorePermit>,
}
async fn run(collection: Collection) -> Result<Report, Error> {
    let Collection {
        context,
        runtime,
        args,
        slots,
        cancel,
        updates,
        mut report,
        reservation,
        digest,
        mut initial_permit,
    } = collection;
    let horizon = tokio::time::Instant::now()
        + Duration::from_secs(u64::from(
            args.horizon_seconds
                .unwrap_or(runtime.definition.limits.horizon_seconds),
        ));
    loop {
        if cancel.is_cancelled() {
            report.status = "cancelled".into();
            break;
        }
        if args.poll && tokio::time::Instant::now() >= horizon {
            break;
        }
        match if let Some(permit) = initial_permit.take() {
            Ok(permit)
        } else {
            slots.clone().try_acquire_owned()
        } {
            Ok(permit) => {
                let deadline = (tokio::time::Instant::now()
                    + Duration::from_millis(u64::from(runtime.definition.limits.timeout_ms)))
                .min(if args.poll {
                    horizon
                } else {
                    tokio::time::Instant::now() + Duration::from_secs(30)
                });
                let result = super::invocation::invoke(
                    &context,
                    &runtime,
                    args.scope.clone(),
                    &args.collection_id,
                    deadline,
                    cancel.clone(),
                )
                .await;
                drop(permit);
                match result {
                    Ok(result) => {
                        report.invocations += 1;
                        report.snapshot_seq = result.snapshot_seq;
                        if matches!(
                            result.outcome,
                            InvocationOutcome::Observed | InvocationOutcome::ObservedWithGaps
                        ) {
                            report.observations += 1;
                        }
                        if result.evidence_gap.is_some() {
                            report.gaps += 1;
                        }
                        if result.outcome == InvocationOutcome::Cancelled {
                            report.status = "cancelled".into();
                        }
                        report.last_invocation = Some(result);
                    }
                    Err(error) if error.code.as_ref() == "cancelled" => {
                        report.status = "cancelled".into();
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(_) => {
                report.skipped += 1;
            }
        }
        updates.send_replace(report.clone());
        if !args.poll || report.status == "cancelled" {
            break;
        }
        tokio::select! {_ =cancel.cancelled()=>{report.status="cancelled".into();break;},_=tokio::time::sleep_until((tokio::time::Instant::now()+Duration::from_millis(u64::from(runtime.definition.limits.poll_interval_ms))).min(horizon))=>{}}
    }
    if report.status != "cancelled" {
        report.status = if report.gaps > 0 || report.skipped > 0 {
            "completed_with_gaps"
        } else {
            "completed"
        }
        .into();
    }
    let last = context
        .append(
            context.event(
                "check.collection_completed",
                "ronchi",
                json!({"request_id":args.collection_id,"arguments_digest":digest,"report":report}),
            ),
            Some(reservation),
        )
        .await?;
    report.completion_ref = Some(EventRef {
        lab_id: context.lab_id.clone(),
        event_seq: last,
    });
    report.snapshot_seq = context.writer.status().snapshot_seq.max(last);
    updates.send_replace(report.clone());
    Ok(report)
}
