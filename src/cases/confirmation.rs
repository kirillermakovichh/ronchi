//! Acceptance assigns one durable simulator schedule; reads never advance its deadlines.
use crate::rpc::RpcError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HoldAt {
    Processed,
    Confirmed,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delay {
    pub processing_ms: u64,
    pub confirmation_interval_ms: u64,
    pub extra_delay_ms: u64,
    pub hold_at: HoldAt,
}
impl Default for Delay {
    fn default() -> Self {
        Self {
            processing_ms: 2_000,
            confirmation_interval_ms: 2_000,
            extra_delay_ms: 30_000,
            hold_at: HoldAt::Processed,
        }
    }
}
impl Delay {
    pub fn validate(&self) -> Result<(), RpcError> {
        if [
            self.processing_ms,
            self.confirmation_interval_ms,
            self.extra_delay_ms,
        ]
        .iter()
        .any(|n| !(1..=120_000).contains(n))
        {
            return Err(RpcError::params(
                "Confirmation intervals must be between 1 and 120000 ms",
            ));
        }
        Ok(())
    }
    pub fn deadlines(&self, accepted_ns: u64) -> Result<[u64; 3], RpcError> {
        self.validate()?;
        let processing = self.processing_ms;
        let confirmed = processing
            + self.confirmation_interval_ms
            + if self.hold_at == HoldAt::Processed {
                self.extra_delay_ms
            } else {
                0
            };
        let finalized = processing + 2 * self.confirmation_interval_ms + self.extra_delay_ms;
        let mut result = [0; 3];
        for (target, millis) in result.iter_mut().zip([processing, confirmed, finalized]) {
            *target = accepted_ns
                .checked_add(millis * 1_000_000)
                .filter(|n| *n < i64::MAX as u64)
                .ok_or_else(|| {
                    RpcError::unsupported("Simulator time exceeds the persistence range")
                })?;
        }
        Ok(result)
    }
}

pub(crate) fn selected(
    binding: &crate::cases::Binding,
    signature: &str,
    responds: bool,
) -> Option<Delay> {
    let policy = binding.policy.as_ref()?;
    let settings = &policy.settings;
    if !responds
        || !settings.is_confirmation()
        || !(settings.signatures.is_empty() || settings.signatures.iter().any(|s| s == signature))
    {
        return None;
    }
    Some(settings.confirmation.clone().unwrap_or_else(|| Delay {
        hold_at: if settings.case_id == "finality-delayed" {
            HoldAt::Confirmed
        } else {
            HoldAt::Processed
        },
        ..Default::default()
    }))
}
