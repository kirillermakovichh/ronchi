//! Read-only evaluation models. Stored definitions, not current configuration, own rules.
use super::{
    Scope,
    rules::{EVALUATION_VERSION, Requirement},
};
use crate::query::EventRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub check_id: String,
    pub definition_version: u32,
    pub definition_digest: Option<String>,
    pub scope: Scope,
}
impl Selection {
    pub fn normalize(&mut self) -> std::result::Result<(), crate::query::Error> {
        self.scope
            .normalize()
            .map_err(|_| crate::query::Error::invalid("Invalid Check scope"))?;
        if self.scope.intent_ids.len() + self.scope.signatures.len() != 1 {
            return Err(crate::query::Error::capability(
                "Initial Check evaluation requires exactly one intent or signature scope",
            ));
        }
        if !crate::workspace::identifier(&self.check_id)
            || self.definition_version == 0
            || self
                .definition_digest
                .as_ref()
                .is_some_and(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(crate::query::Error::invalid(
                "Invalid pinned Check definition",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Fail,
    Inconclusive,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RuleResult {
    pub rule_id: String,
    pub version: u32,
    pub verdict: Verdict,
    pub reason: String,
    pub supporting: Vec<EventRef>,
    pub gaps: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Result {
    pub check_id: String,
    pub definition_version: u32,
    pub definition_digest: String,
    pub evaluation_version: u32,
    pub lab_id: String,
    pub snapshot_seq: u64,
    pub scope: Scope,
    pub source: String,
    pub source_authority: String,
    pub source_schema_version: u32,
    pub verdict: Option<Verdict>,
    pub requirements: Vec<RuleResult>,
    pub limitations: Vec<String>,
}
impl Result {
    pub(crate) fn new(
        selection: &Selection,
        digest: String,
        lab_id: &str,
        snapshot: u64,
        source: String,
        schema: u32,
        requirements: &[Requirement],
    ) -> Self {
        Self {
            check_id: selection.check_id.clone(),
            definition_version: selection.definition_version,
            definition_digest: digest,
            evaluation_version: EVALUATION_VERSION,
            lab_id: lab_id.into(),
            snapshot_seq: snapshot,
            scope: selection.scope.clone(),
            source,
            source_authority: "application".into(),
            source_schema_version: schema,
            verdict: None,
            requirements: requirements
                .iter()
                .map(|r| RuleResult {
                    rule_id: r.rule_id.clone(),
                    version: r.version,
                    verdict: Verdict::Inconclusive,
                    reason: "No sufficient authoritative observation".into(),
                    supporting: vec![],
                    gaps: vec!["missing_observation".into()],
                })
                .collect(),
            limitations: vec![
                "Application facts retain their configured source authority".into(),
                "Simulated finality does not establish economic execution".into(),
                "Two prepared identities do not establish two transfers".into(),
            ],
        }
    }
    pub(crate) fn aggregate(&mut self) {
        self.verdict = if self.requirements.is_empty() {
            None
        } else if self.requirements.iter().any(|r| r.verdict == Verdict::Fail) {
            Some(Verdict::Fail)
        } else if self
            .requirements
            .iter()
            .any(|r| r.verdict == Verdict::Inconclusive)
        {
            Some(Verdict::Inconclusive)
        } else {
            Some(Verdict::Pass)
        };
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Aggregate {
    pub verdict: Option<Verdict>,
    pub checks: Vec<Result>,
}
/// Aggregate only the caller's declared required Checks; retain every gap/result.
pub fn aggregate(checks: Vec<Result>) -> std::result::Result<Aggregate, crate::query::Error> {
    if let Some(first) = checks.first() {
        let mut ids = std::collections::BTreeSet::new();
        if checks.iter().any(|check| {
            check.lab_id != first.lab_id
                || check.snapshot_seq != first.snapshot_seq
                || check.scope != first.scope
                || !ids.insert(&check.check_id)
        }) {
            return Err(crate::query::Error::invalid(
                "Aggregate requires unique Checks from the same Lab, scope, and snapshot",
            ));
        }
    }
    let verdict = if checks.is_empty() {
        None
    } else if checks.iter().any(|c| c.verdict == Some(Verdict::Fail)) {
        Some(Verdict::Fail)
    } else if checks.iter().any(|c| c.verdict != Some(Verdict::Pass)) {
        Some(Verdict::Inconclusive)
    } else {
        Some(Verdict::Pass)
    };
    Ok(Aggregate { verdict, checks })
}
