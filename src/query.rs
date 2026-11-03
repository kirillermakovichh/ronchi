//! Finite read-only views of one existing Lab Journal.
pub(crate) mod cli;
use crate::journal::{JournalReader, JournalState};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum Operation {
    #[serde(rename = "investigate.summary")]
    Summary,
    #[serde(rename = "investigate.findings")]
    Findings,
    #[serde(rename = "investigate.timeline")]
    Timeline,
    #[serde(rename = "investigate.evidence")]
    Evidence,
}
impl Operation {
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "summary" | "investigate.summary" => Ok(Self::Summary),
            "findings" | "investigate.findings" => Ok(Self::Findings),
            "timeline" | "investigate.timeline" => Ok(Self::Timeline),
            "evidence" | "investigate.evidence" => Ok(Self::Evidence),
            _ => Err(Error::capability(
                "This Investigation operation is not implemented",
            )),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventRef {
    pub lab_id: String,
    pub event_seq: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Scope {
    pub signature: Vec<String>,
    pub rpc_method: Vec<String>,
    pub case_activation_id: Vec<u64>,
    pub kind: Vec<String>,
    pub event_refs: Vec<EventRef>,
    pub finding_id: Option<String>,
    pub from_seq: Option<u64>,
    pub to_seq: Option<u64>,
    pub since: Option<String>,
    pub until: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Arguments {
    pub scope: Scope,
    pub limit: u16,
    pub cursor: Option<String>,
    pub fields: Vec<String>,
    pub include_payload: bool,
    pub snapshot_seq: Option<u64>,
}
impl Default for Arguments {
    fn default() -> Self {
        Self {
            scope: Scope::default(),
            limit: 50,
            cursor: None,
            fields: vec![],
            include_payload: false,
            snapshot_seq: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub contract_version: u32,
    pub request_id: String,
    pub lab_id: String,
    pub operation: Operation,
    pub arguments: Arguments,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Gap {
    pub code: String,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct View {
    pub snapshot_seq: u64,
    pub scope: Scope,
    pub fields: Vec<String>,
    pub include_payload: bool,
    pub observation_boundary: Vec<String>,
    pub evidence_gaps: Vec<Gap>,
    pub journal_state: JournalState,
    pub selection_complete: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Page {
    pub next_cursor: Option<String>,
    pub truncated: bool,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Response {
    pub contract_version: u32,
    pub request_id: String,
    pub lab_id: String,
    pub selection_basis: String,
    pub result: Value,
    pub view: View,
    pub page: Page,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    pub code: Box<str>,
    pub message: Box<str>,
    pub phase: Box<str>,
    pub retryable: bool,
    pub effects: Box<str>,
    pub recovery: Box<str>,
}
impl Error {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            phase: "query".into(),
            retryable: false,
            effects: "none".into(),
            recovery: "Correct the selection or start a fresh bounded query".into(),
        }
    }
    pub fn invalid(message: &str) -> Self {
        Self::new("invalid_argument", message)
    }
    pub fn cursor() -> Self {
        Self::new(
            "invalid_cursor",
            "Cursor is damaged or does not bind this operation, Lab, snapshot, or selection",
        )
    }
    pub fn capability(message: &str) -> Self {
        Self::new("unsupported_capability", message)
    }
    pub fn storage() -> Self {
        Self::new("internal_error", "The retained Journal query failed")
    }
    pub fn exit(&self) -> i32 {
        match self.code.as_ref() {
            "invalid_argument" | "invalid_field" | "invalid_cursor" => 2,
            "journal_unavailable" | "lab_unavailable" | "finding_unavailable" => 3,
            "unsupported_capability" => 4,
            "capacity_exceeded" | "state_conflict" | "analysis_incomplete" => 5,
            "deadline_exceeded" => 124,
            "cancelled" => 130,
            _ => 6,
        }
    }
}
#[derive(Serialize, JsonSchema)]
pub struct Failure {
    pub contract_version: u32,
    pub request_id: String,
    pub lab_id: Option<String>,
    pub error: Error,
}
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
pub enum Envelope {
    Success(Box<Response>),
    Failure(Box<Failure>),
}
#[derive(Clone)]
pub struct Limits {
    pub response_bytes: usize,
    pub preview_bytes: usize,
    pub deadline: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            response_bytes: MAX_RESPONSE_BYTES,
            preview_bytes: 4096,
            deadline: Duration::from_secs(10),
        }
    }
}
#[derive(Clone)]
pub struct Engine {
    pub(crate) limits: Limits,
    readers: Arc<tokio::sync::Semaphore>,
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            readers: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
}
struct CancelGuard(CancellationToken);
impl Drop for CancelGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
impl Engine {
    pub fn with_limits(limits: Limits) -> Result<Self, Error> {
        if limits.response_bytes == 0
            || limits.response_bytes > MAX_RESPONSE_BYTES
            || limits.preview_bytes > 4096
            || limits.deadline > Duration::from_secs(10)
        {
            return Err(Error::invalid("Query limits exceed the core ceilings"));
        }
        Ok(Self {
            limits,
            ..Default::default()
        })
    }
    pub async fn execute(
        &self,
        path: PathBuf,
        owner_alive: bool,
        selection_basis: &str,
        mut request: Request,
        cancel: CancellationToken,
    ) -> Result<Response, Error> {
        request.normalize()?;
        let permit =
            self.readers.clone().try_acquire_owned().map_err(|_| {
                Error::new("capacity_exceeded", "All bounded query readers are busy")
            })?;
        let limits = self.limits.clone();
        let basis = selection_basis.to_string();
        if !["current", "latest", "explicit"].contains(&basis.as_str()) {
            return Err(Error::invalid("Invalid Lab selection basis"));
        }
        let deadline = Instant::now() + limits.deadline;
        let token = cancel.child_token();
        let guard = CancelGuard(token.clone());
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            check(&token, deadline)?;
            let mut reader = JournalReader::open(&path).map_err(|_| {
                Error::new(
                    "journal_unavailable",
                    "The selected Journal cannot be opened read-only",
                )
            })?;
            let progress = token.clone();
            reader
                .connection_for_query_progress(deadline, progress)
                .map_err(|_| Error::storage())?;
            let result = reader.investigate(request, owner_alive, basis, &limits, deadline, &token);
            if token.is_cancelled() || Instant::now() >= deadline {
                check(&token, deadline)?;
            }
            result
        });
        let result = task.await.map_err(|_| Error::storage())?;
        drop(guard);
        result
    }
}
pub(crate) fn check(cancel: &CancellationToken, deadline: Instant) -> Result<(), Error> {
    if cancel.is_cancelled() {
        Err(Error::new(
            "cancelled",
            "Investigation was cancelled; the Lab keeps running",
        ))
    } else if Instant::now() >= deadline {
        Err(Error::new(
            "deadline_exceeded",
            "Investigation exceeded its bounded deadline",
        ))
    } else {
        Ok(())
    }
}
const EVENT_FIELDS: &[&str] = &[
    "event_ref",
    "kind",
    "schema_version",
    "source",
    "recorded_at",
    "elapsed_ns",
    "execution_mode",
    "rpc_method",
    "transaction_signature",
    "transaction_signatures",
    "request_id",
    "batch_id",
    "registration_seq",
    "policy_ref",
    "cause_refs",
    "data",
    "simulated_time_ns",
    "slot",
    "block_height",
    "payload_status",
    "payload",
];
pub(crate) const REQUIRED: &[&str] = &[
    "schema_version",
    "execution_mode",
    "transaction_signature",
    "transaction_signatures",
    "request_id",
    "batch_id",
    "registration_seq",
    "policy_ref",
    "cause_refs",
    "payload_status",
];
impl Request {
    pub fn normalize(&mut self) -> Result<(), Error> {
        let a = &mut self.arguments;
        let s = &mut a.scope;
        if self.contract_version != 1
            || self.request_id.is_empty()
            || self.request_id.len() > 128
            || self.lab_id.is_empty()
            || self.lab_id.len() > 128
            || !(1..=200).contains(&a.limit)
            || a.fields.len() > 32
        {
            return Err(Error::invalid(
                "Invalid version, identity, page size, or projection size",
            ));
        }
        if s.finding_id.as_ref().is_some_and(|id| {
            id.len() != 67
                || !id.starts_with("f1-")
                || !id[3..].bytes().all(|c| c.is_ascii_hexdigit())
        }) {
            return Err(Error::invalid("Expected a stable f1- Finding ID"));
        }
        if s.finding_id.is_some()
            && (self.operation != Operation::Evidence || a.snapshot_seq.is_none())
        {
            return Err(Error::invalid(
                "Finding evidence requires investigate.evidence and explicit snapshot_seq",
            ));
        }
        if a.snapshot_seq.is_some_and(|n| n > i64::MAX as u64) {
            return Err(Error::invalid("Invalid snapshot watermark"));
        }
        let count = s.signature.len()
            + s.rpc_method.len()
            + s.case_activation_id.len()
            + s.kind.len()
            + s.event_refs.len()
            + usize::from(s.from_seq.is_some())
            + usize::from(s.to_seq.is_some())
            + usize::from(s.since.is_some())
            + usize::from(s.until.is_some());
        if count > 200 || a.cursor.as_ref().is_some_and(|s| s.len() > 4096) {
            return Err(Error::invalid("Query exceeds its filter/cursor budget"));
        }
        for values in [&mut s.signature, &mut s.rpc_method, &mut s.kind] {
            if values.iter().any(|s| s.is_empty() || s.len() > 256) {
                return Err(Error::invalid(
                    "Filter values must contain 1..256 UTF-8 bytes",
                ));
            }
            values.sort();
            values.dedup();
        }
        s.case_activation_id.sort();
        s.case_activation_id.dedup();
        s.event_refs
            .sort_by(|a, b| a.event_seq.cmp(&b.event_seq).then(a.lab_id.cmp(&b.lab_id)));
        s.event_refs.dedup();
        if s.event_refs
            .iter()
            .any(|r| r.lab_id != self.lab_id || r.event_seq == 0 || r.event_seq > i64::MAX as u64)
            || s.case_activation_id.iter().any(|n| *n > i64::MAX as u64)
            || [s.from_seq, s.to_seq]
                .into_iter()
                .flatten()
                .any(|n| n > i64::MAX as u64)
            || s.from_seq.zip(s.to_seq).is_some_and(|(a, b)| a > b)
        {
            return Err(Error::invalid(
                "Invalid event reference or inclusive sequence interval",
            ));
        }
        for value in [&mut s.since, &mut s.until].into_iter().flatten() {
            if value.len() > 64 {
                return Err(Error::invalid("Invalid RFC 3339 time bound"));
            }
            let parsed =
                time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                    .map_err(|_| Error::invalid("Expected an RFC 3339 time bound"))?
                    .to_offset(time::UtcOffset::UTC);
            *value = parsed
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|_| Error::invalid("Invalid time bound"))?;
        }
        if let (Some(a), Some(b)) = (&s.since, &s.until)
            && time_value(a) > time_value(b)
        {
            return Err(Error::invalid("since must not exceed until"));
        }
        if a.include_payload && self.operation != Operation::Evidence {
            return Err(Error::invalid(
                "Payload selection requires investigate.evidence",
            ));
        }
        let defaults = if self.operation == Operation::Findings {
            vec!["statement", "guidance", "limitations"]
        } else if self.operation == Operation::Summary {
            vec!["counts", "traffic_state"]
        } else {
            vec![
                "recorded_at",
                "elapsed_ns",
                "execution_mode",
                "rpc_method",
                "data",
                "simulated_time_ns",
                "slot",
                "block_height",
            ]
        };
        if a.fields.is_empty() {
            a.fields = defaults.into_iter().map(String::from).collect();
        }
        for field in &a.fields {
            let allowed = if self.operation == Operation::Findings {
                [
                    "finding_id",
                    "rule_id",
                    "version",
                    "lab_id",
                    "scope",
                    "supporting_events",
                    "statement",
                    "guidance",
                    "limitations",
                ]
                .contains(&field.as_str())
            } else if self.operation == Operation::Summary {
                [
                    "counts",
                    "traffic_state",
                    "analysis",
                    "findings",
                    "lab_context",
                ]
                .contains(&field.as_str())
            } else {
                EVENT_FIELDS.contains(&field.as_str())
            };
            if !allowed {
                return Err(Error::new("invalid_field", "Unknown typed projection path"));
            }
            if field == "payload" && !a.include_payload {
                return Err(Error::invalid(
                    "Select include_payload explicitly for payload fields",
                ));
            }
        }
        a.fields.sort();
        a.fields.dedup();
        if encoded_len(self, MAX_REQUEST_BYTES).is_err() {
            return Err(Error::invalid("Encoded query exceeds 64 KiB"));
        }
        Ok(())
    }
    pub(crate) fn fingerprint(&self) -> String {
        let mut copy = self.clone();
        copy.request_id.clear();
        copy.arguments.cursor = None;
        digest(
            &serde_json::to_vec(&(
                copy,
                [
                    crate::findings::Rule::LookupAfterWithholding.definition(),
                    crate::findings::Rule::SubmissionAfterWithholding.definition(),
                ],
            ))
            .expect("typed request serializes"),
        )
    }
}
pub(crate) fn time_value(value: &str) -> (i64, u32) {
    let parsed = time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .expect("normalized bound");
    (parsed.unix_timestamp(), parsed.nanosecond())
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|n| format!("{n:02x}"))
        .collect()
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cursor {
    pub v: u8,
    pub snapshot: u64,
    pub position: u64,
    pub fingerprint: String,
    pub state: JournalState,
}
impl Cursor {
    pub fn encode(&self) -> String {
        let body = serde_json::to_vec(self).expect("cursor serializes");
        format!("{}.{}", URL_SAFE_NO_PAD.encode(&body), digest(&body))
    }
    pub fn decode(value: &str) -> Result<Self, Error> {
        let (body, checksum) = value.split_once('.').ok_or_else(Error::cursor)?;
        let decoded = URL_SAFE_NO_PAD.decode(body).map_err(|_| Error::cursor())?;
        if digest(&decoded) != checksum {
            return Err(Error::cursor());
        }
        let value: Self = serde_json::from_slice(&decoded).map_err(|_| Error::cursor())?;
        if value.v != 1 || value.position > value.snapshot {
            return Err(Error::cursor());
        }
        Ok(value)
    }
}
pub(crate) fn encoded_len(value: &impl Serialize, max: usize) -> Result<usize, ()> {
    struct Count {
        bytes: usize,
        max: usize,
    }
    impl Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes = self.bytes.saturating_add(bytes.len());
            if self.bytes > self.max {
                return Err(std::io::Error::other("response budget"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count { bytes: 0, max };
    serde_json::to_writer(&mut count, value).map_err(|_| ())?;
    Ok(count.bytes)
}
