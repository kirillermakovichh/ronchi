use crate::config::Mode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub artifact: Artifact,
    pub digest: String,
    pub resolved_primitives: Vec<String>,
    pub resolved_templates: Vec<String>,
    /// Parsed input before defaults. This is the authority for the canonical digest.
    pub canonical_input: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub schema: String,
    pub id: String,
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub requires: Requires,
    pub modes: Vec<Mode>,
    pub filter: Filter,
    pub state: StateSpec,
    pub sequence: Vec<Step>,
    #[serde(default)]
    pub findings: Vec<FindingRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<Repeat>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    pub primitives: Vec<String>,
    #[serde(default)]
    pub finding_templates: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub signatures: Vec<String>,
    #[serde(default)]
    pub commitments: Vec<String>,
    pub transaction_origin: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateSpec {
    pub key: String,
    pub scope: String,
    pub max_keys: u16,
    pub on_exhausted: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub using: String,
    #[serde(default)]
    pub with: Value,
    pub until: Bound,
    pub next: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bound {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matches: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Repeat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycles: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub while_active: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindingRule {
    pub id: String,
    pub template: String,
    pub facts: BTreeMap<String, FactSelector>,
    pub same: String,
    pub order: Vec<String>,
    pub within_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FactSelector {
    pub kind: String,
    pub step: String,
}
