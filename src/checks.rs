//! Named HTTP observation sources. Collection never evaluates a Verdict.
mod validation;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
pub use validation::{SourceResponse, validate};
pub const CAPTURE_BYTES: usize = 32 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub concurrency: u8,
    pub sources: Vec<Definition>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            concurrency: 4,
            sources: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub check_id: String,
    pub version: u32,
    pub source: String,
    pub schema_version: u32,
    pub url: String,
    pub allowed_origin: Option<String>,
    pub authorization_env: Option<String>,
    pub facts: BTreeMap<String, ValueType>,
    #[serde(default)]
    pub limits: Limits,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub timeout_ms: u32,
    pub poll_interval_ms: u32,
    pub horizon_seconds: u32,
    pub response_bytes: usize,
    pub max_facts: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            poll_interval_ms: 1000,
            horizon_seconds: 60,
            response_bytes: 256 * 1024,
            max_facts: 1000,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ValueType {
    Boolean,
    Integer {
        unit: Option<String>,
    },
    Decimal {
        unit: Option<String>,
    },
    String {
        max_bytes: usize,
    },
    Enum {
        values: Vec<String>,
    },
    Array {
        max_items: usize,
        items: Box<ValueType>,
    },
    /// Only declared fields are retained. exact=false explicitly permits projected records.
    Record {
        fields: BTreeMap<String, ValueType>,
        #[serde(default)]
        exact: bool,
    },
    Nullable {
        value: Box<ValueType>,
    },
    Availability {
        value: Box<ValueType>,
    },
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=16).contains(&self.concurrency) || self.sources.len() > 16 {
            return Err("Check configuration exceeds concurrency/source limits".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for source in &self.sources {
            source.validate()?;
            if !ids.insert(&source.check_id) {
                return Err("Check IDs must be unique".into());
            }
        }
        if crate::query::encoded_len(self, 16 * 1024).is_err() {
            return Err("Check configuration exceeds 16 KiB".into());
        }
        Ok(())
    }
}
impl Definition {
    pub fn validate(&self) -> Result<(), String> {
        let id = crate::workspace::identifier;
        if !id(&self.check_id)
            || !id(&self.source)
            || self.version == 0
            || self.schema_version == 0
            || self.facts.is_empty()
            || self.facts.len() > 32
            || self.facts.keys().any(|k| !id(k))
        {
            return Err("Invalid Check identity or requested facts".into());
        }
        let url = url::Url::parse(&self.url).map_err(|_| "Invalid Check observation URL")?;
        if self.url.len() > 2048
            || url.scheme() != "http"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "Check URL must be HTTP without URL credentials, query, or fragment".into(),
            );
        }
        let loopback = match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            Some(url::Host::Domain(name)) => name == "localhost",
            _ => false,
        };
        if let Some(origin) = &self.allowed_origin {
            let origin = url::Url::parse(origin).map_err(|_| "Invalid Check origin")?;
            if origin.origin() != url.origin()
                || origin.path() != "/"
                || origin.query().is_some()
                || origin.fragment().is_some()
                || !origin.username().is_empty()
                || origin.password().is_some()
            {
                return Err("Check URL does not match the exact allowed origin".into());
            }
        } else if !loopback {
            return Err("Non-loopback Check sources require allowed_origin".into());
        }
        if self.authorization_env.as_ref().is_some_and(|v| {
            v.is_empty()
                || v.len() > 128
                || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        }) {
            return Err("Invalid runtime authorization variable name".into());
        }
        if let Some(name) = &self.authorization_env {
            let value =
                std::env::var(name).map_err(|_| "Check runtime Authorization is unavailable")?;
            if value.is_empty()
                || value.len() > 4096
                || hyper::header::HeaderValue::from_str(&value).is_err()
            {
                return Err("Invalid Check runtime Authorization".into());
            }
        }
        let l = &self.limits;
        if !(1..=30000).contains(&l.timeout_ms)
            || l.poll_interval_ms < 1000
            || l.poll_interval_ms > 300000
            || !(1..=300).contains(&l.horizon_seconds)
            || l.response_bytes == 0
            || l.response_bytes > 1024 * 1024
            || l.max_facts == 0
            || l.max_facts > 4000
        {
            return Err("Check invocation/collection limits exceed the core ceilings".into());
        }
        let mut nodes = 0;
        for schema in self.facts.values() {
            schema.check(0, &mut nodes)?;
        }
        if crate::query::encoded_len(&self.public_definition(), 16 * 1024).is_err() {
            return Err("Public Check definition exceeds 16 KiB".into());
        }
        Ok(())
    }
    pub fn effective_bytes(&self) -> usize {
        self.limits.response_bytes.min(CAPTURE_BYTES)
    }
    pub fn public_definition(&self) -> Value {
        serde_json::json!({"check_id":self.check_id,"version":self.version,"source":self.source,"schema_version":self.schema_version,"facts":self.facts,"limits":self.limits,"effective_response_bytes":self.effective_bytes()})
    }
}
impl ValueType {
    fn check(&self, depth: usize, nodes: &mut usize) -> Result<(), String> {
        *nodes += 1;
        if depth > 12 || *nodes > 128 {
            return Err("Check schema exceeds depth/node limits".into());
        }
        match self {
            Self::String { max_bytes } if *max_bytes == 0 || *max_bytes > CAPTURE_BYTES => {
                return Err("Invalid Check string bound".into());
            }
            Self::Integer { unit } | Self::Decimal { unit }
                if unit.as_ref().is_some_and(|s| s.is_empty() || s.len() > 64) =>
            {
                return Err("Invalid fact unit".into());
            }
            Self::Enum { values }
                if values.is_empty()
                    || values.len() > 32
                    || values.iter().any(|s| s.len() > 128) =>
            {
                return Err("Invalid enum schema".into());
            }
            Self::Array { max_items, items } => {
                if *max_items > 4000 {
                    return Err("Invalid array bound".into());
                }
                items.check(depth + 1, nodes)?;
            }
            Self::Record { fields, .. } => {
                if fields.len() > 32 || fields.keys().any(|k| !crate::workspace::identifier(k)) {
                    return Err("Invalid record fields".into());
                }
                for schema in fields.values() {
                    schema.check(depth + 1, nodes)?;
                }
            }
            Self::Nullable { value } | Self::Availability { value } => {
                value.check(depth + 1, nodes)?
            }
            _ => (),
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Scope {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub intent_ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub signatures: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
}
impl Scope {
    pub fn normalize(&mut self) -> Result<(), String> {
        if self.intent_ids.len() + self.signatures.len() > 32
            || self.intent_ids.is_empty() && self.signatures.is_empty()
            || self
                .intent_ids
                .iter()
                .any(|s| !crate::workspace::identifier(s))
            || self.signatures.iter().any(|s| s.is_empty() || s.len() > 88)
        {
            return Err("Check collection requires an explicit bounded scope".into());
        }
        self.intent_ids.sort();
        self.intent_ids.dedup();
        self.signatures.sort();
        self.signatures.dedup();
        for bound in [&mut self.since, &mut self.until].into_iter().flatten() {
            if bound.len() > 64 {
                return Err("Invalid Check scope time bound".into());
            }
            let parsed =
                time::OffsetDateTime::parse(bound, &time::format_description::well_known::Rfc3339)
                    .map_err(|_| "Invalid Check scope time bound")?
                    .to_offset(time::UtcOffset::UTC);
            *bound = parsed
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|_| "Invalid Check scope time bound")?;
        }
        if self
            .since
            .as_ref()
            .zip(self.until.as_ref())
            .is_some_and(|(a, b)| crate::query::time_value(a) > crate::query::time_value(b))
        {
            return Err("Invalid Check interval".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceRequest {
    pub contract_version: u32,
    pub invocation_id: String,
    pub check_id: String,
    pub lab_id: String,
    pub snapshot_seq: u64,
    pub scope: Scope,
    pub facts: Vec<String>,
    pub deadline: String,
}

mod invocation;
mod transport;
pub use invocation::{Context, InvocationOutcome, InvocationResult, collect_once};
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
    pub fn new(code: &str, effects: &str) -> Self {
        Self {
            code: code.into(),
            message: match code {
                "invalid_argument" => "Invalid Check request or runtime configuration",
                "state_conflict" => {
                    "Check/scope is occupied or collection conflicts with retained state"
                }
                "capacity_exceeded" => "Check collection capacity is occupied",
                "cancelled" => {
                    "Check collection was cancelled; retained observations remain available"
                }
                "lab_unavailable" => "Check collection requires an authenticated live Lab",
                _ => "Check Journal admission or commit failed; collection stopped",
            }
            .into(),
            phase: "check.observe".into(),
            retryable: false,
            effects: effects.into(),
            recovery:
                "Inspect the collection identity and retained Check events before another mutation"
                    .into(),
        }
    }
    pub fn invalid() -> Self {
        Self::new("invalid_argument", "none")
    }
    pub fn journal() -> Self {
        Self::new("internal_error", "unknown")
    }
    pub fn exit(&self) -> i32 {
        match self.code.as_ref() {
            "invalid_argument" => 2,
            "lab_unavailable" | "collection_unavailable" => 3,
            "state_conflict" | "capacity_exceeded" => 5,
            "cancelled" => 130,
            _ => 6,
        }
    }
}

mod collection;
pub use collection::{Arguments, Client, Envelope, Failure, Owner, Reply, Report, Request, spawn};

pub(crate) mod cli;
