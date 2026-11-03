//! Immutable policies and exchange-owned response withholding. History lives in Journal.
use crate::rpc::RpcError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

pub mod confirmation;
pub mod status;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Arm {
    pub case_id: String,
    #[serde(default = "default_hold")]
    pub hold_ms: u64,
    #[serde(default = "default_capacity")]
    pub max_held: u16,
    #[serde(default)]
    pub signatures: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<confirmation::Delay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<status::Settings>,
}
fn default_hold() -> u64 {
    20_000
}
fn default_capacity() -> u16 {
    16
}
impl Default for Arm {
    fn default() -> Self {
        Self {
            case_id: "tx-response-lost".into(),
            hold_ms: default_hold(),
            max_held: default_capacity(),
            signatures: vec![],
            confirmation: None,
            status: None,
        }
    }
}
impl Arm {
    pub fn validate(&self) -> Result<(), RpcError> {
        if ![
            "tx-response-lost",
            "tx-confirmation-stalls",
            "finality-delayed",
            "signature-status-null",
            "stale-transaction-read-after-submission",
        ]
        .contains(&self.case_id.as_str())
        {
            return Err(RpcError::unsupported("Unknown or unavailable Case"));
        }
        if !(1..=20_000).contains(&self.hold_ms)
            || !(1..=16).contains(&self.max_held)
            || self.signatures.len() > 16
        {
            return Err(RpcError::params(
                "Case settings exceed hold/signature limits",
            ));
        }
        for signature in &self.signatures {
            crate::protocol::signature(&serde_json::json!(signature))?;
        }
        if let Some(delay) = &self.confirmation {
            if !self.is_confirmation() {
                return Err(RpcError::params(
                    "Confirmation settings require a confirmation Case",
                ));
            }
            delay.validate()?;
        }
        if let Some(status) = &self.status {
            if !matches!(
                self.case_id.as_str(),
                "signature-status-null" | "stale-transaction-read-after-submission"
            ) {
                return Err(RpcError::params("Status settings require a status Case"));
            }
            status.validate()?;
        }
        Ok(())
    }
    pub fn is_confirmation(&self) -> bool {
        matches!(
            self.case_id.as_str(),
            "tx-confirmation-stalls" | "finality-delayed"
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Policy {
    pub revision: u64,
    #[serde(default)]
    pub activated_elapsed_ns: u64,
    #[serde(default)]
    pub activated_simulated_ns: u64,
    #[serde(flatten)]
    pub settings: Arm,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Binding {
    pub registration_seq: u64,
    pub policy: Option<Policy>,
}
/// Activation state follows registered exchanges, independently of later replacements.
#[derive(Debug, Default)]
pub(crate) struct Activation {
    pub status: Mutex<status::Cache>,
}
pub(crate) struct Registration {
    pub binding: Binding,
    pub activation: Option<Arc<Activation>>,
    pub elapsed_ns: u64,
    pub simulated_ns: u64,
}
impl Binding {
    pub fn selects(&self, signature: &str, responds: bool) -> bool {
        responds
            && self.policy.as_ref().is_some_and(|p| {
                p.settings.case_id == "tx-response-lost"
                    && (p.settings.signatures.is_empty()
                        || p.settings.signatures.iter().any(|s| s == signature))
            })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct HeldEffect {
    pub registration_seq: u64,
    pub policy_revision: u64,
    pub remaining_ms: u64,
}
pub(crate) struct HoldPool {
    slots: Arc<Semaphore>,
    entries: Mutex<std::collections::BTreeMap<u64, (u64, std::sync::Weak<Hold>)>>,
}
impl Default for HoldPool {
    fn default() -> Self {
        Self {
            slots: Arc::new(Semaphore::new(16)),
            entries: Default::default(),
        }
    }
}
impl HoldPool {
    pub fn snapshot(&self) -> Vec<HeldEffect> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, (_, hold)| {
            hold.upgrade()
                .is_some_and(|hold| !hold.abort.is_cancelled())
        });
        entries
            .iter()
            .filter_map(|(seq, (revision, hold))| {
                hold.upgrade().and_then(|hold| {
                    hold.deadline().map(|deadline| HeldEffect {
                        registration_seq: *seq,
                        policy_revision: *revision,
                        remaining_ms: deadline
                            .saturating_duration_since(tokio::time::Instant::now())
                            .as_millis()
                            .min(u64::MAX as u128) as u64,
                    })
                })
            })
            .collect()
    }
}
#[derive(Default)]
pub(crate) struct Hold {
    permit: Mutex<Option<OwnedSemaphorePermit>>,
    pub abort: CancellationToken,
    pub started: CancellationToken,
    pub observed_committed: CancellationToken,
    credits: Mutex<Vec<crate::journal::TransitionReservation>>,
    deadline: Mutex<Option<tokio::time::Instant>>,
}
impl Hold {
    pub fn reserve(
        self: &Arc<Self>,
        pool: &Arc<HoldPool>,
        maximum: u16,
        registration: u64,
        revision: u64,
    ) -> Result<(), RpcError> {
        if self.abort.is_cancelled() {
            return Err(RpcError::params(
                "Transport closed before new fault admission",
            ));
        }
        let mut permit = self.permit.lock().unwrap_or_else(|e| e.into_inner());
        if permit.is_some() {
            return Ok(());
        }
        if 16 - pool.slots.available_permits() >= usize::from(maximum) {
            return Err(capacity());
        }
        *permit = Some(
            pool.slots
                .clone()
                .try_acquire_owned()
                .map_err(|_| capacity())?,
        );
        let mut entries = pool.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, (_, hold)| {
            hold.upgrade()
                .is_some_and(|hold| !hold.abort.is_cancelled())
        });
        entries.insert(registration, (revision, Arc::downgrade(self)));
        Ok(())
    }
    pub async fn reserve_evidence(
        self: &Arc<Self>,
        writer: crate::journal::WriterHandle,
        count: usize,
    ) -> Result<(), RpcError> {
        if self.deadline().is_some() {
            return Ok(());
        }
        let credits = tokio::task::spawn_blocking(move || {
            (0..2 * count + 1)
                .map(|_| writer.reserve_transition())
                .collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|_| RpcError::internal())?
        .map_err(|_| RpcError::internal())?;
        self.credits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(credits);
        Ok(())
    }
    pub async fn record(
        &self,
        writer: &crate::journal::WriterHandle,
        started: std::time::Instant,
        kind: &str,
        items: Vec<serde_json::Value>,
    ) -> Result<(), RpcError> {
        for data in items {
            let credit = self
                .credits
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .pop()
                .ok_or_else(RpcError::internal)?;
            let event = crate::lab::event(kind, started, data);
            let writer = writer.clone();
            let receipt =
                tokio::task::spawn_blocking(move || writer.try_append_reserved(&event, credit))
                    .await
                    .map_err(|_| RpcError::internal())?
                    .map_err(|_| RpcError::internal())?;
            receipt
                .committed()
                .await
                .map_err(|_| RpcError::internal())?;
        }
        Ok(())
    }
    pub async fn start(
        &self,
        writer: &crate::journal::WriterHandle,
        started: std::time::Instant,
        hold_ms: u64,
        data: serde_json::Value,
    ) -> Result<(), RpcError> {
        let credit = {
            let mut deadline = self.deadline.lock().unwrap_or_else(|e| e.into_inner());
            if self.abort.is_cancelled() || deadline.is_some() {
                return Ok(());
            }
            let credit = self
                .credits
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .pop()
                .ok_or_else(RpcError::internal)?;
            *deadline = Some(tokio::time::Instant::now() + Duration::from_millis(hold_ms));
            self.started.cancel();
            credit
        };
        let event = crate::lab::event("fault.withholding_started", started, data);
        let writer = writer.clone();
        let result = async {
            let receipt =
                tokio::task::spawn_blocking(move || writer.try_append_reserved(&event, credit))
                    .await
                    .map_err(|_| RpcError::internal())?
                    .map_err(|_| RpcError::internal())?;
            receipt
                .committed()
                .await
                .map_err(|_| RpcError::internal())?;
            Ok(())
        }
        .await;
        self.observed_committed.cancel();
        result
    }
    pub fn close(&self) -> bool {
        let deadline = self.deadline.lock().unwrap_or_else(|e| e.into_inner());
        self.abort.cancel();
        deadline.is_some()
    }
    pub fn deadline(&self) -> Option<tokio::time::Instant> {
        *self.deadline.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn release(&self) {
        self.permit.lock().unwrap_or_else(|e| e.into_inner()).take();
        self.credits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
}
fn capacity() -> RpcError {
    RpcError {
        code: -32000,
        message: "Case held-request capacity is exhausted".into(),
        data: Some(serde_json::json!({"code":"case_capacity_exceeded"})),
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CatalogEntry {
    pub case_id: String,
    pub modes: Vec<String>,
    pub default_hold_ms: u64,
    pub max_held: u16,
}
pub fn catalog() -> Vec<CatalogEntry> {
    [
        "tx-response-lost",
        "tx-confirmation-stalls",
        "finality-delayed",
        "signature-status-null",
        "stale-transaction-read-after-submission",
    ]
    .into_iter()
    .map(|case_id| CatalogEntry {
        case_id: case_id.into(),
        modes: if matches!(
            case_id,
            "tx-response-lost"
                | "signature-status-null"
                | "stale-transaction-read-after-submission"
        ) {
            vec!["simulator".into(), "surfpool".into()]
        } else {
            vec!["simulator".into()]
        },
        default_hold_ms: 20_000,
        max_held: 16,
    })
    .collect()
}
pub fn lab_error(error: RpcError) -> crate::lab::LabError {
    if error.data.as_ref().is_some_and(|data| {
        data["code"] == "capacity_exceeded" || data["code"] == "case_capacity_exceeded"
    }) {
        crate::lab::LabError::conflict(error.message)
    } else if error.code == -32602 {
        crate::lab::LabError::invalid(error.message)
    } else if error
        .data
        .as_ref()
        .is_some_and(|data| data["code"] == "unsupported_capability")
    {
        crate::lab::LabError::unsupported(error.message)
    } else {
        crate::lab::LabError::internal(error.message)
    }
}
