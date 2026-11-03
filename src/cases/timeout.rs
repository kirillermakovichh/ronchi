//! A request's frozen registration time selects the outage, independently of dispatch time.
use crate::{cases::Binding, rpc::RpcError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub(crate) mod validation;
const DEFAULT_METHODS: [&str; 4] = [
    "sendTransaction",
    "getLatestBlockhash",
    "getSignatureStatuses",
    "getTransaction",
];
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    #[schemars(range(min = 1, max = 120000))]
    pub outage_ms: u64,
    #[schemars(range(min = 1, max = 120000))]
    pub recovery_ms: u64,
    #[schemars(length(max = 32))]
    pub methods: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            outage_ms: 5000,
            recovery_ms: 10000,
            methods: vec![],
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), RpcError> {
        if !(1..=120000).contains(&self.outage_ms)
            || !(1..=120000).contains(&self.recovery_ms)
            || self.methods.len() > 32
            || self
                .methods
                .iter()
                .any(|m| !crate::backend::METHODS.contains(&m.as_str()))
        {
            return Err(RpcError::params(
                "Timeout intervals must be 1..120000 ms; method filters require at most 32 supported methods",
            ));
        }
        Ok(())
    }
    /// Right boundaries belong to the next interval; this never reads wall time.
    pub fn position(&self, elapsed_ns: u64) -> (u64, u64, bool) {
        let cycle = (self.outage_ms + self.recovery_ms) * 1_000_000;
        let position = elapsed_ns % cycle;
        (
            elapsed_ns / cycle,
            position,
            position < self.outage_ms * 1_000_000,
        )
    }
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Selection {
    pub cycle: u64,
    pub position_ns: u64,
    pub outage_ms: u64,
    pub recovery_ms: u64,
}
pub(crate) fn candidate(
    binding: &Binding,
    reference: &Value,
    method: &str,
    notification: bool,
) -> Option<Selection> {
    let p = binding.policy.as_ref()?;
    if p.settings.case_id != "provider-timeout-burst" || notification {
        return None;
    }
    let defaults = Settings::default();
    let settings = p.settings.timeout.as_ref().unwrap_or(&defaults);
    let selected = if settings.methods.is_empty() {
        DEFAULT_METHODS.contains(&method)
    } else {
        settings.methods.iter().any(|m| m == method)
    };
    if !selected {
        return None;
    }
    if p.custom.is_some() {
        return Some(Selection {
            cycle: 0,
            position_ns: 0,
            outage_ms: settings.outage_ms,
            recovery_ms: settings.recovery_ms,
        });
    }
    let elapsed = reference["registered_elapsed_ns"]
        .as_u64()?
        .checked_sub(p.activated_elapsed_ns)?;
    let (cycle, position_ns, outage) = settings.position(elapsed);
    outage.then_some(Selection {
        cycle,
        position_ns,
        outage_ms: settings.outage_ms,
        recovery_ms: settings.recovery_ms,
    })
}
pub(crate) fn matches(binding: &Binding, signatures: &[String]) -> bool {
    binding.policy.as_ref().is_some_and(|p| {
        p.settings.signatures.is_empty()
            || signatures.iter().any(|s| p.settings.signatures.contains(s))
    })
}
pub fn finding_definition() -> Value {
    serde_json::json!({"rule_id":"provider-timeout-burst.later-rpc-attempt","version":1,"required_facts":["rpc.dispatch_prevented","fault.withholding_started","rpc.boundary","rpc.registered:later_same_method"],"correlation_keys":["lab_id","request_id","registration_seq","rpc_method"],"conclusion":"Ronchi prevented dispatch and observed a later same-method RPC attempt; no claim about a client timeout or causal retry"})
}
