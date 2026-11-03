//! Versioned interpretations of retained facts. This module does no I/O.
//! Callers select the earliest eligible later fact for each incident and rule.
use crate::{config::Mode, query::EventRef};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    LookupAfterWithholding,
    SubmissionAfterWithholding,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Submission,
    Acceptance,
    Selection,
    Withholding,
    StatusRequest,
    TransactionRequest,
}
#[derive(Clone, Copy, Debug, Serialize, JsonSchema)]
pub struct Requirement {
    pub kind: FactKind,
    pub source: &'static str,
    pub role: &'static str,
}
/// Small maintained declarative patterns, not an expression interpreter.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct Definition {
    pub rule_id: &'static str,
    pub version: u32,
    pub execution_mode: Mode,
    pub required_facts: [Requirement; 4],
    pub later_source: &'static str,
    pub later_role: &'static str,
    pub guidance: &'static str,
    pub limitations: &'static [&'static str],
    pub later_kinds: &'static [FactKind],
    pub correlation_keys: &'static [&'static str],
    pub order: &'static str,
    pub incident_key: &'static str,
}
impl Rule {
    pub fn definition(self) -> Definition {
        Definition {
            rule_id: match self {
                Self::LookupAfterWithholding => "tx-response-lost.lookup-after-withholding",
                Self::SubmissionAfterWithholding => "tx-response-lost.submission-after-withholding",
            },
            version: 1,
            execution_mode: Mode::Simulator,
            required_facts: [
                Requirement {
                    kind: FactKind::Submission,
                    source: "ronchi",
                    role: "submission",
                },
                Requirement {
                    kind: FactKind::Acceptance,
                    source: "rpc_simulator",
                    role: "acceptance",
                },
                Requirement {
                    kind: FactKind::Selection,
                    source: "ronchi",
                    role: "fault_selection",
                },
                Requirement {
                    kind: FactKind::Withholding,
                    source: "ronchi",
                    role: "observed_withholding",
                },
            ],
            later_source: "ronchi",
            later_role: "later_action",
            guidance: match self {
                Self::LookupAfterWithholding => {
                    "Inspect how the application reconciles the original signature and completes its operation after this lookup."
                }
                Self::SubmissionAfterWithholding => {
                    "Inspect retry and reconciliation behavior for the original transaction identity. Another attempt does not establish a second transaction or transfer."
                }
            },
            limitations: &[
                "RPC-only evidence does not establish a client timeout, operation completion, or business intent.",
                "Simulator acceptance does not establish instruction execution, balance changes, or an economic transfer.",
                "Event and registration order establish observation order, not application causality.",
            ],
            later_kinds: match self {
                Self::LookupAfterWithholding => {
                    &[FactKind::StatusRequest, FactKind::TransactionRequest]
                }
                Self::SubmissionAfterWithholding => &[FactKind::Submission],
            },
            correlation_keys: &["lab_id", "signature", "request_id", "registration_seq"],
            order: "submission < acceptance < selection < withholding < later; later registration > incident registration",
            incident_key: "Lab + signature + accepted request + acceptance event; earliest eligible later event per rule",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Fact {
    pub event_ref: EventRef,
    pub schema_version: u32,
    pub execution_mode: Mode,
    pub source: String,
    pub kind: FactKind,
    pub request_id: String,
    pub registration_seq: u64,
    pub signature: String,
    pub digest: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Evidence {
    pub lab_id: String,
    pub snapshot_seq: u64,
    pub submission: Option<Fact>,
    pub acceptance: Option<Fact>,
    pub selection: Option<Fact>,
    pub withholding: Option<Fact>,
    pub later: Option<Fact>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Matched,
    NoMatch,
    InsufficientEvidence,
    Unsupported,
    OperationalLimit,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Support {
    pub role: String,
    pub event_ref: EventRef,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct FindingScope {
    pub signature: String,
    pub accepted_request_id: String,
    pub from_seq: u64,
    pub to_seq: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Finding {
    pub finding_id: String,
    pub rule_id: String,
    pub version: u32,
    pub lab_id: String,
    pub scope: FindingScope,
    pub supporting_events: Vec<Support>,
    pub statement: String,
    pub guidance: String,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Evaluation {
    pub rule: Rule,
    pub version: u32,
    pub state: State,
    pub finding: Option<Finding>,
    pub evidence_gaps: Vec<String>,
}
impl Evaluation {
    fn empty(rule: Rule, state: State, gap: Option<&str>) -> Self {
        Self {
            rule,
            version: rule.definition().version,
            state,
            finding: None,
            evidence_gaps: gap.into_iter().map(String::from).collect(),
        }
    }
}
/// Evaluate one candidate incident using only facts at or below its committed watermark.
/// Missing facts do not establish negative application behavior.
pub fn evaluate(rule: Rule, evidence: &Evidence) -> Evaluation {
    let definition = rule.definition();
    let facts = [
        evidence.submission.as_ref(),
        evidence.acceptance.as_ref(),
        evidence.selection.as_ref(),
        evidence.withholding.as_ref(),
        evidence.later.as_ref(),
    ];
    if facts
        .iter()
        .flatten()
        .any(|f| f.schema_version != 1 || f.execution_mode != definition.execution_mode)
    {
        return Evaluation::empty(
            rule,
            State::Unsupported,
            Some("Required fact schema or execution mode is unsupported"),
        );
    }
    if facts.iter().flatten().any(|f| {
        f.event_ref.lab_id != evidence.lab_id
            || f.event_ref.event_seq == 0
            || f.event_ref.event_seq > evidence.snapshot_seq
    }) {
        return Evaluation::empty(
            rule,
            State::InsufficientEvidence,
            Some("Required facts are outside the selected Lab or snapshot"),
        );
    }
    let [
        Some(submission),
        Some(acceptance),
        Some(selection),
        Some(withholding),
        later,
    ] = facts
    else {
        return Evaluation::empty(
            rule,
            State::InsufficientEvidence,
            Some("Submission, acceptance, selected Fault, and observed withholding are required"),
        );
    };
    for (fact, expected) in [submission, acceptance, selection, withholding]
        .into_iter()
        .zip(definition.required_facts)
    {
        if fact.kind != expected.kind
            || fact.source != expected.source
            || fact.request_id.is_empty()
            || fact.registration_seq == 0
            || fact.signature.is_empty()
        {
            return Evaluation::empty(
                rule,
                State::InsufficientEvidence,
                Some("Required typed facts lack authoritative source or correlation keys"),
            );
        }
        if fact.signature != acceptance.signature
            || fact.request_id != acceptance.request_id
            || fact.registration_seq != acceptance.registration_seq
        {
            return Evaluation::empty(rule, State::NoMatch, None);
        }
    }
    if ![submission, acceptance, selection, withholding]
        .windows(2)
        .all(|p| p[0].event_ref.event_seq < p[1].event_ref.event_seq)
    {
        return Evaluation::empty(rule, State::NoMatch, None);
    }
    let Some(later) = later else {
        return Evaluation::empty(rule, State::NoMatch, None);
    };
    if later.source != definition.later_source
        || later.request_id.is_empty()
        || later.registration_seq == 0
        || later.signature.is_empty()
    {
        return Evaluation::empty(
            rule,
            State::InsufficientEvidence,
            Some("Later action lacks authoritative request correlation"),
        );
    }
    if !definition.later_kinds.contains(&later.kind)
        || later.signature != acceptance.signature
        || later.request_id == acceptance.request_id
        || later.registration_seq <= acceptance.registration_seq
        || later.event_ref.event_seq <= withholding.event_ref.event_seq
    {
        return Evaluation::empty(rule, State::NoMatch, None);
    }
    let same_bytes = rule == Rule::SubmissionAfterWithholding
        && submission.digest.as_ref().filter(|d| !d.is_empty())
            == acceptance.digest.as_ref().filter(|d| !d.is_empty())
        && acceptance.digest.as_ref().filter(|d| !d.is_empty())
            == later.digest.as_ref().filter(|d| !d.is_empty())
        && acceptance.digest.as_ref().is_some_and(|d| !d.is_empty());
    let mut limitations: Vec<String> = definition.limitations.iter().map(|s| (*s).into()).collect();
    let signature = &acceptance.signature;
    let statement = match rule {
        Rule::LookupAfterWithholding => format!(
            "Ronchi received a status or transaction lookup for {signature} after observing response withholding for its accepted submission."
        ),
        Rule::SubmissionAfterWithholding => {
            if !same_bytes {
                limitations.push("Signed-byte equality is unavailable without matching retained content digests.".into());
            }
            format!(
                "Ronchi recorded another submission attempt for transaction identity {signature} after observing response withholding{}.",
                if same_bytes {
                    "; the retained content digests match"
                } else {
                    ""
                }
            )
        }
    };
    let supporting_events: Vec<_> = [
        definition.required_facts[0].role,
        definition.required_facts[1].role,
        definition.required_facts[2].role,
        definition.required_facts[3].role,
        definition.later_role,
    ]
    .into_iter()
    .zip([submission, acceptance, selection, withholding, later])
    .map(|(role, fact)| Support {
        role: role.into(),
        event_ref: fact.event_ref.clone(),
    })
    .collect();
    let scope = FindingScope {
        signature: signature.clone(),
        accepted_request_id: acceptance.request_id.clone(),
        from_seq: submission.event_ref.event_seq,
        to_seq: later.event_ref.event_seq,
    };
    // Exclude evaluation watermark and presentation from semantic identity.
    let identity = serde_json::to_vec(&(
        definition.rule_id,
        definition.version,
        &evidence.lab_id,
        &scope,
        &supporting_events,
    ))
    .expect("typed Finding identity serializes");
    let finding = Finding {
        finding_id: format!("f1-{}", crate::query::digest(&identity)),
        rule_id: definition.rule_id.into(),
        version: definition.version,
        lab_id: evidence.lab_id.clone(),
        scope,
        supporting_events,
        statement,
        guidance: definition.guidance.into(),
        limitations,
    };
    let mut result = Evaluation::empty(rule, State::Matched, None);
    if rule == Rule::SubmissionAfterWithholding && !same_bytes {
        result.evidence_gaps.push("Matching signed content digests are unavailable; only transaction identity is established".into());
    }
    result.finding = Some(finding);
    result
}
