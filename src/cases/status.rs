//! Activation-scoped lookup views. Authoritative transaction state never lives here.
use crate::rpc::RpcError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const WINDOW_NS: u64 = 10_000_000_000;
const RESERVED_KEY_BYTES: usize = 1024;
const MAX_SNAPSHOT_BYTES: usize = 16 * 1024;
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub max_keys: u16,
    pub max_snapshot_bytes: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            max_keys: 1024,
            max_snapshot_bytes: 4 * 1024 * 1024,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), RpcError> {
        if self.max_keys == 0
            || self.max_keys > 1024
            || !(RESERVED_KEY_BYTES..=4 * 1024 * 1024).contains(&self.max_snapshot_bytes)
        {
            return Err(RpcError::params(
                "Status Case count or byte bounds exceed their limits",
            ));
        }
        Ok(())
    }
}
pub fn selected(binding: &super::Binding) -> bool {
    binding.policy.as_ref().is_some_and(|p| {
        matches!(
            p.settings.case_id.as_str(),
            "signature-status-null" | "stale-transaction-read-after-submission"
        )
    })
}
pub fn stale(binding: &super::Binding) -> bool {
    binding
        .policy
        .as_ref()
        .is_some_and(|p| p.settings.case_id == "stale-transaction-read-after-submission")
}
pub fn matches(binding: &super::Binding, signature: &str) -> bool {
    selected(binding)
        && binding.policy.as_ref().is_some_and(|p| {
            p.settings.signatures.is_empty() || p.settings.signatures.iter().any(|s| s == signature)
        })
}
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub value: Value,
    pub captured_ns: u64,
    pub source_seq: u64,
    pub event_seq: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub accepted_ns: u64,
    pub admission_request: Value,
    pub snapshot: Option<Snapshot>,
}
#[derive(Debug, Default)]
pub(crate) struct Cache {
    entries: BTreeMap<String, Entry>,
    bytes: usize,
    settings: Settings,
}
impl Cache {
    pub fn admit(
        &mut self,
        signature: &str,
        accepted_ns: u64,
        request: &Value,
        settings: &Settings,
    ) -> Result<(), RpcError> {
        if self.entries.contains_key(signature) {
            return Ok(());
        }
        if self.entries.len() >= usize::from(settings.max_keys)
            || self.bytes.saturating_add(RESERVED_KEY_BYTES) > settings.max_snapshot_bytes
        {
            return Err(capacity());
        }
        self.settings = settings.clone();
        self.bytes += RESERVED_KEY_BYTES;
        self.entries.insert(
            signature.into(),
            Entry {
                accepted_ns,
                admission_request: json!({"request_id":request["request_id"],"registration_seq":request["registration_seq"]}),
                snapshot: None,
            },
        );
        Ok(())
    }
    pub fn accepted(&mut self, signature: &str, at: u64) {
        if let Some(entry) = self.entries.get_mut(signature) {
            entry.accepted_ns = at;
        }
    }
    pub fn remove(&mut self, signature: &str) {
        if let Some(entry) = self.entries.remove(signature) {
            self.bytes = self.bytes.saturating_sub(
                RESERVED_KEY_BYTES
                    + entry.snapshot.as_ref().map_or(0, |s| {
                        s.value
                            .to_string()
                            .len()
                            .saturating_sub(RESERVED_KEY_BYTES / 2)
                    }),
            );
        }
    }
    pub fn entry(&self, signature: &str) -> Option<&Entry> {
        self.entries.get(signature)
    }
    pub fn capture(
        &mut self,
        signature: &str,
        value: &Value,
        now: u64,
        source_seq: u64,
    ) -> Result<Option<Snapshot>, RpcError> {
        let Some(entry) = self.entries.get_mut(signature) else {
            return Ok(None);
        };
        if entry.snapshot.is_some() {
            return Ok(None);
        };
        if !value.is_object()
            || !matches!(
                value["confirmationStatus"].as_str(),
                Some("processed" | "confirmed" | "finalized")
            )
            || !value["slot"].is_u64()
        {
            return Ok(None);
        }
        let size = value.to_string().len();
        let extra = size.saturating_sub(RESERVED_KEY_BYTES / 2);
        if size > MAX_SNAPSHOT_BYTES
            || self.bytes.saturating_add(extra) > self.settings.max_snapshot_bytes
        {
            return Err(capacity());
        }
        let snapshot = Snapshot {
            value: value.clone(),
            captured_ns: now,
            source_seq,
            event_seq: 0,
        };
        entry.snapshot = Some(snapshot.clone());
        self.bytes += extra;
        Ok(Some(snapshot))
    }
    pub fn captured(&mut self, signature: &str, seq: u64) {
        if let Some(snapshot) = self
            .entries
            .get_mut(signature)
            .and_then(|e| e.snapshot.as_mut())
        {
            snapshot.event_seq = seq;
        }
    }
}
pub(crate) fn capacity() -> RpcError {
    RpcError {
        code: -32000,
        message: "Status Case transient state capacity is exhausted".into(),
        data: Some(json!({"code":"case_capacity_exceeded"})),
    }
}
pub fn finding_definition() -> Value {
    json!({"rule_id":"rpc.lookup-view.changed","version":1,"modes":["simulator","surfpool"],"required_facts":["transaction.accepted","rpc.lookup_source","fault.lookup_changed","rpc.response_prepared","rpc.boundary"],"correlation":["signature","request_id","policy_revision","source_event_seq"],"limits":{"incidents":256},"conclusion":"Ronchi changed an available authoritative lookup view; client receipt and business outcome remain unknown"})
}
