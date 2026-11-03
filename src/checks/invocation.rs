use super::{
    Definition, Error, Scope, SourceRequest,
    transport::{self, Runtime},
};
use crate::{
    config::Mode,
    journal::{Event, TransitionReservation, WriterHandle},
    query::EventRef,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Instant;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
pub struct Context {
    pub writer: WriterHandle,
    pub lab_id: String,
    pub mode: Mode,
    pub started: Instant,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvocationOutcome {
    Observed,
    ObservedWithGaps,
    SourceGap,
    Cancelled,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InvocationResult {
    pub invocation_id: String,
    pub start_ref: EventRef,
    pub completion_ref: EventRef,
    pub snapshot_seq: u64,
    pub outcome: InvocationOutcome,
    pub evidence_gap: Option<String>,
}
impl Context {
    pub(super) fn event(&self, kind: &str, source: &str, data: Value) -> Event {
        let mut event = crate::lab::event(kind, self.started, data);
        event.execution_mode = self.mode;
        event.source = source.into();
        event
    }
    pub(super) async fn reserve(&self) -> Result<TransitionReservation, Error> {
        let writer = self.writer.clone();
        tokio::task::spawn_blocking(move || writer.reserve_transition())
            .await
            .map_err(|_| Error::journal())?
            .map_err(|_| Error::journal())
    }
    pub(super) async fn append(
        &self,
        event: Event,
        reserved: Option<TransitionReservation>,
    ) -> Result<u64, Error> {
        let writer = self.writer.clone();
        let failure = writer.clone();
        let result = async {
            let receipt = tokio::task::spawn_blocking(move || match reserved {
                Some(reservation) => writer.try_append_reserved(&event, reservation),
                None => writer.try_append(&[event]),
            })
            .await
            .map_err(|_| Error::journal())?
            .map_err(|_| Error::journal())?;
            receipt.committed().await.map_err(|_| Error::journal())
        }
        .await;
        if result.is_err() {
            failure.fail_fatal();
        }
        result
    }
    fn reference(&self, seq: u64) -> EventRef {
        EventRef {
            lab_id: self.lab_id.clone(),
            event_seq: seq,
        }
    }
}
pub async fn collect_once(
    context: &Context,
    definition: Definition,
    mut scope: Scope,
    collection_id: &str,
    cancel: CancellationToken,
) -> Result<InvocationResult, Error> {
    scope.normalize().map_err(|_| Error::invalid())?;
    let runtime = Runtime::new(definition)?;
    invoke(
        context,
        &runtime,
        scope,
        collection_id,
        tokio::time::Instant::now()
            + std::time::Duration::from_millis(u64::from(runtime.definition.limits.timeout_ms)),
        cancel,
    )
    .await
}
pub(super) async fn invoke(
    context: &Context,
    runtime: &Runtime,
    scope: Scope,
    collection_id: &str,
    deadline: tokio::time::Instant,
    cancel: CancellationToken,
) -> Result<InvocationResult, Error> {
    if cancel.is_cancelled() {
        return Err(Error::new("cancelled", "none"));
    }
    let invocation_id = format!(
        "I{}",
        crate::workspace::nonce().map_err(|_| Error::journal())?
    );
    let reservation = context.reserve().await?;
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    let request = SourceRequest {
        contract_version: 1,
        invocation_id: invocation_id.clone(),
        check_id: runtime.definition.check_id.clone(),
        lab_id: context.lab_id.clone(),
        snapshot_seq: context.writer.status().snapshot_seq,
        scope: scope.clone(),
        facts: runtime.definition.facts.keys().cloned().collect(),
        deadline: (time::OffsetDateTime::now_utc()
            + time::Duration::try_from(remaining).map_err(|_| Error::invalid())?)
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_| Error::invalid())?,
    };
    let start=context.append(context.event("check.invocation_started","ronchi",json!({"request_id":invocation_id,"collection_id":collection_id,"source_request":request,"definition":runtime.definition.public_definition()})),None).await?;
    let response = if deadline <= tokio::time::Instant::now() {
        Err("deadline_before_http")
    } else {
        transport::post(runtime, &request, deadline, &cancel).await
    }
    .and_then(|response| super::validate(&runtime.definition, &request, response));
    let (outcome, gap, payload) = match response {
        Ok(mut response) => {
            for fact in response.facts.values_mut() {
                if contains_secret(fact, &runtime.secrets) {
                    *fact = json!({"availability":"redacted","reason":"runtime_credential"});
                }
            }
            if response.revision.as_ref().is_some_and(|revision| {
                runtime
                    .secrets
                    .iter()
                    .any(|secret| revision.contains(secret))
            }) {
                response.revision = None;
            }
            let gaps = response
                .facts
                .values()
                .any(|value| value["availability"] != "available" || nested_gap(value));
            (
                if gaps {
                    InvocationOutcome::ObservedWithGaps
                } else {
                    InvocationOutcome::Observed
                },
                gaps.then(|| "fact_unavailable_or_redacted".to_string()),
                Some(serde_json::to_value(response).map_err(|_| Error::journal())?),
            )
        }
        Err(code) => (
            if code == "cancelled" {
                InvocationOutcome::Cancelled
            } else {
                InvocationOutcome::SourceGap
            },
            Some(code.into()),
            None,
        ),
    };
    let mut data = json!({"request_id":invocation_id,"collection_id":collection_id,"check_id":runtime.definition.check_id,"version":runtime.definition.version,"source_id":runtime.definition.source,"source_schema_version":runtime.definition.schema_version,"scope":scope,"snapshot_seq":request.snapshot_seq,"cause_refs":[context.reference(start)],"outcome":outcome,"evidence_gap":gap,"retention":if payload.is_some(){"complete_validated_selection"}else{"no_validated_observation"}});
    if let Some(payload) = payload {
        data["payload"] = payload;
    }
    let observed = data.get("payload").is_some();
    let completed = context
        .append(
            context.event(
                if observed {
                    "check.observation"
                } else {
                    "check.invocation_error"
                },
                if observed { "application" } else { "ronchi" },
                data,
            ),
            Some(reservation),
        )
        .await?;
    Ok(InvocationResult {
        invocation_id,
        start_ref: context.reference(start),
        completion_ref: context.reference(completed),
        snapshot_seq: context.writer.status().snapshot_seq.max(completed),
        outcome,
        evidence_gap: gap,
    })
}
fn contains_secret(value: &Value, secrets: &[String]) -> bool {
    match value {
        Value::String(s) => secrets.iter().any(|secret| s.contains(secret)),
        Value::Object(object) => object.iter().any(|(key, v)| {
            secrets.iter().any(|secret| key.contains(secret)) || contains_secret(v, secrets)
        }),
        Value::Array(a) => a.iter().any(|v| contains_secret(v, secrets)),
        _ => false,
    }
}
fn nested_gap(value: &Value) -> bool {
    match value {
        Value::Object(o) => {
            o.get("availability").is_some_and(|v| v != "available") || o.values().any(nested_gap)
        }
        Value::Array(a) => a.iter().any(nested_gap),
        _ => false,
    }
}
