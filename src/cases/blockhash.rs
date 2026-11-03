//! One genuine tuple per activation. Expiry requires a matching source height, never a slot.
use crate::{rpc::RpcError, simulator::clock::Commitment};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub const WINDOW_NS: u64 = 10_000_000_000;
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub commitment: Commitment,
}
#[derive(Clone, Debug)]
struct Tuple {
    wire: Value,
    hash: String,
    last_valid: u64,
    slot: u64,
    source: String,
    commitment: Commitment,
    capture_request: String,
    cycle: String,
    expired_at: Option<u64>,
    observed_height: Option<u64>,
}
impl Tuple {
    fn parse(wire: &Value, source: &str, commitment: Commitment, request: &Value) -> Option<Self> {
        if serde_json::to_vec(wire).ok()?.len() > 16 * 1024 {
            return None;
        }
        let hash = wire["value"]["blockhash"].as_str()?;
        let mut bytes = [0u8; 32];
        if hash.len() > 44 || bs58::decode(hash).onto(&mut bytes).ok() != Some(32) {
            return None;
        }
        let request = request["request_id"].as_str()?.to_string();
        Some(Self {
            wire: wire.clone(),
            hash: hash.into(),
            last_valid: wire["value"]["lastValidBlockHeight"].as_u64()?,
            slot: wire["context"]["slot"].as_u64()?,
            source: source.into(),
            commitment,
            capture_request: request.clone(),
            cycle: format!("b1-{request}"),
            expired_at: None,
            observed_height: None,
        })
    }
    fn facts(&self) -> Value {
        json!({"cycle_id":self.cycle,"capture_request_id":self.capture_request,"source_identity":self.source,"commitment":self.commitment,"blockhash":self.hash,"last_valid_block_height":self.last_valid,"context_slot":self.slot,"observed_block_height":self.observed_height,"expiry_observed_elapsed_ns":self.expired_at})
    }
}
pub(crate) type Facts = Vec<(&'static str, Value)>;
#[derive(Debug, Default)]
pub(crate) struct Cache {
    tuple: Option<Tuple>,
    recovery_until: Option<u64>,
}
impl Cache {
    fn advance(&mut self, now: u64) -> Facts {
        let mut facts = vec![];
        if self.recovery_until.is_none()
            && let Some(tuple) = &self.tuple
            && let Some(expired) = tuple.expired_at
            && now >= expired.saturating_add(WINDOW_NS)
        {
            self.recovery_until = Some(expired.saturating_add(2 * WINDOW_NS));
            let mut fact = tuple.facts();
            fact["expired_submission_observed"] = json!(false);
            fact["window_end_elapsed_ns"] = json!(expired.saturating_add(WINDOW_NS));
            fact["recovery_until_elapsed_ns"] = json!(self.recovery_until);
            facts.push(("fault.blockhash_window_elapsed", fact));
        }
        if self.recovery_until.is_some_and(|until| now >= until) {
            self.tuple = None;
            self.recovery_until = None;
        }
        facts
    }
    pub(crate) fn height(
        &mut self,
        height: u64,
        source: &str,
        commitment: Commitment,
        now: u64,
    ) -> Facts {
        let mut facts = self.advance(now);
        if self.recovery_until.is_none()
            && let Some(tuple) = &mut self.tuple
            && tuple.source == source
            && tuple.commitment == commitment
        {
            tuple.observed_height = Some(height);
            if height > tuple.last_valid && tuple.expired_at.is_none() {
                tuple.expired_at = Some(now);
                facts.push(("fault.blockhash_expired", tuple.facts()));
            }
        }
        facts
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "One source response binds authority, commitment, context constraint and observation time"
    )]
    pub(crate) fn read(
        &mut self,
        fresh: &Value,
        source: &str,
        commitment: Commitment,
        min_context: Option<u64>,
        height: Option<u64>,
        request: &Value,
        now: u64,
    ) -> (Value, Facts) {
        let mut facts = self.advance(now);
        if self.recovery_until.is_some() {
            let mut fact = json!({"reason":"recovery_interval","recovery_until_elapsed_ns":self.recovery_until});
            fact["source_identity"] = json!(source);
            facts.push(("fault.blockhash_skipped", fact));
            return (fresh.clone(), facts);
        }
        if self.tuple.is_none() {
            let Some(tuple) = Tuple::parse(fresh, source, commitment, request) else {
                facts.push((
                    "fault.blockhash_skipped",
                    json!({"reason":"genuine_tuple_unavailable","source_identity":source}),
                ));
                return (fresh.clone(), facts);
            };
            facts.push(("fault.blockhash_captured", tuple.facts()));
            self.tuple = Some(tuple);
            if let Some(height) = height {
                facts.extend(self.height(height, source, commitment, now));
            }
            return (fresh.clone(), facts);
        }
        if let Some(height) = height {
            facts.extend(self.height(height, source, commitment, now));
        }
        let tuple = self.tuple.as_ref().expect("tuple was captured");
        let reason = if tuple.source != source || tuple.commitment != commitment {
            Some("source_or_commitment_changed")
        } else if min_context.is_some_and(|slot| slot > tuple.slot) {
            Some("cached_context_excluded")
        } else if height.is_none() {
            Some("source_height_unavailable")
        } else {
            None
        };
        if let Some(reason) = reason {
            let mut fact = tuple.facts();
            fact["reason"] = json!(reason);
            facts.push(("fault.blockhash_skipped", fact));
            return (fresh.clone(), facts);
        }
        facts.push(("fault.blockhash_returned", tuple.facts()));
        (tuple.wire.clone(), facts)
    }
    pub(crate) fn submission(
        &mut self,
        signature: &str,
        hash: &str,
        lifetime: &str,
        eligible: bool,
        now: u64,
    ) -> Facts {
        let mut facts = self.advance(now);
        let Some(tuple) = &self.tuple else {
            return facts;
        };
        if tuple.hash != hash {
            return facts;
        }
        if lifetime != "recent_blockhash" || !eligible {
            let mut fact = tuple.facts();
            fact["signature"] = json!(signature);
            fact["reason"] = json!(if lifetime != "recent_blockhash" {
                "nonce_or_unresolved_lifetime"
            } else {
                "identity_or_filter_excluded"
            });
            facts.push(("fault.blockhash_skipped", fact));
            return facts;
        }
        if self.recovery_until.is_none() && tuple.expired_at.is_some() {
            let mut fact = tuple.facts();
            fact["signature"] = json!(signature);
            self.recovery_until = Some(now.saturating_add(WINDOW_NS));
            fact["recovery_until_elapsed_ns"] = json!(self.recovery_until);
            facts.push(("fault.blockhash_submission", fact));
        }
        facts
    }
}
pub(crate) fn query(params: &Value) -> Result<(Commitment, Option<u64>), RpcError> {
    let args = crate::rpc::methods::positional(params)?;
    if args.len() > 1 {
        return Err(RpcError::params(
            "Invalid blockhash context parameter count",
        ));
    }
    let cfg: crate::rpc::methods::ContextConfig = crate::rpc::methods::config(args.first())?;
    Ok((cfg.commitment.unwrap_or_default(), cfg.min_context_slot))
}
pub(crate) fn matches(
    binding: &crate::cases::Binding,
    commitment: Commitment,
    responds: bool,
) -> bool {
    responds
        && binding.policy.as_ref().is_some_and(|policy| {
            policy.settings.case_id == "tx-blockhash-expired"
                && policy
                    .settings
                    .blockhash
                    .as_ref()
                    .map(|settings| settings.commitment)
                    .unwrap_or_default()
                    == commitment
        })
}
pub fn finding_definition() -> Value {
    json!({"rule_id":"recent-blockhash.expired-submission","version":1,"execution_modes":["simulator","surfpool"],"required_facts":["rpc.blockhash_source","fault.blockhash_returned","rpc.blockhash_height","transaction.signed_attempt","rpc.admission_result","rpc.boundary:stale_read","rpc.boundary:submission"],"mode_requirements":{"simulator":["transaction.accepted","transaction.processing_prevented"],"surfpool":["rpc.blockhash_status"]},"correlation_keys":["lab_id","source_identity","commitment","blockhash","signature","request_id"],"expiry_rule":"same-authority same-commitment blockHeight > lastValidBlockHeight","conclusion":"observed submission uses a proven expired recent hash; no inferred client or economic outcome"})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tuple(height: u8) -> Value {
        json!({"context":{"slot":height},"value":{"blockhash":bs58::encode([height;32]).into_string(),"lastValidBlockHeight":u64::from(height)+2}})
    }
    #[test]
    fn wall_clock_windows_recover_without_an_expired_submission_and_capture_again() {
        let mut cache = Cache::default();
        let first = tuple(1);
        let fresh = tuple(9);
        let reference = json!({"request_id":"r1"});
        assert_eq!(
            cache
                .read(
                    &first,
                    "authority",
                    Commitment::Finalized,
                    None,
                    Some(1),
                    &reference,
                    0
                )
                .0,
            first
        );
        assert_eq!(
            cache
                .read(
                    &fresh,
                    "authority",
                    Commitment::Finalized,
                    None,
                    Some(4),
                    &reference,
                    100
                )
                .0,
            first
        );
        assert_eq!(
            cache
                .read(
                    &fresh,
                    "authority",
                    Commitment::Finalized,
                    None,
                    Some(5),
                    &reference,
                    100 + WINDOW_NS - 1
                )
                .0,
            first
        );
        let (recovered, facts) = cache.read(
            &fresh,
            "authority",
            Commitment::Finalized,
            None,
            Some(5),
            &reference,
            100 + WINDOW_NS,
        );
        assert_eq!(recovered, fresh);
        assert!(
            facts
                .iter()
                .any(|(kind, _)| *kind == "fault.blockhash_window_elapsed")
        );
        let next = tuple(10);
        assert_eq!(
            cache
                .read(
                    &next,
                    "authority",
                    Commitment::Finalized,
                    None,
                    Some(10),
                    &json!({"request_id":"r2"}),
                    100 + 2 * WINDOW_NS
                )
                .0,
            next
        );
        assert_eq!(
            cache
                .read(
                    &tuple(11),
                    "authority",
                    Commitment::Finalized,
                    None,
                    Some(11),
                    &reference,
                    101 + 2 * WINDOW_NS
                )
                .0,
            next
        );
    }
}
