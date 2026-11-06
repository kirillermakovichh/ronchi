//! Constant-size telemetry derived only from committed effect records.
use super::Event;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CaseActivity {
    pub policy_revision: u64,
    pub applied_records: u64,
}
pub(super) enum Update {
    Reset(Option<u64>),
    Applied(u64),
}
pub(super) fn updates(events: &[Event]) -> Vec<Update> {
    events
        .iter()
        .filter_map(|event| {
            if event.source != "ronchi" || event.schema_version != 1 {
                return None;
            }
            if event.kind == "case.changed" {
                return Some(Update::Reset(
                    event.data["binding"]["policy"]["revision"].as_u64(),
                ));
            }
            if !matches!(
                event.kind.as_str(),
                "fault.withholding_started"
                    | "fault.lookup_changed"
                    | "fault.custom_lookup_changed"
                    | "fault.confirmation_schedule_assigned"
                    | "fault.blockhash_returned"
            ) {
                return None;
            }
            if event.kind == "fault.blockhash_returned"
                && event.data["expiry_observed_elapsed_ns"].is_null()
            {
                return None;
            }
            event.data["policy"]["revision"]
                .as_u64()
                .or_else(|| event.data["request"]["binding"]["policy"]["revision"].as_u64())
                .map(Update::Applied)
        })
        .collect()
}
pub(super) fn apply(activity: &mut Option<CaseActivity>, updates: Vec<Update>) {
    for update in updates {
        match update {
            Update::Reset(revision) => {
                *activity = revision.map(|policy_revision| CaseActivity {
                    policy_revision,
                    applied_records: 0,
                })
            }
            Update::Applied(revision) => {
                if let Some(current) = activity
                    && current.policy_revision == revision
                {
                    current.applied_records = current.applied_records.saturating_add(1);
                }
            }
        }
    }
}
