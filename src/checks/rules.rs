//! Bounded typed predicates over logical selected-fact paths, without I/O.
use super::ValueType;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};
pub const EVALUATION_VERSION: u32 = 1;
fn positive() -> bool {
    true
}
pub struct Context<'a> {
    pub finality: &'a dyn Fn(&str) -> PredicateResult,
    pub scope: Option<&'a super::Scope>,
    pub observed_at_ms: Option<i64>,
    pub operational: Option<&'a dyn Fn() -> bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CumulativeSet {
    pub path: String,
    pub key: String,
    pub complete_path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub rule_id: String,
    pub version: u32,
    pub pass: Predicate,
    pub fail: Predicate,
    pub complete: Predicate,
    #[serde(default)]
    pub immutable_paths: Vec<String>,
    #[serde(default)]
    pub revision_paths: Vec<String>,
    #[serde(default)]
    pub cumulative_sets: Vec<CumulativeSet>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
impl Relation {
    fn accepts(self, order: Ordering) -> bool {
        match self {
            Self::Eq => order == Ordering::Equal,
            Self::Ne => order != Ordering::Equal,
            Self::Lt => order == Ordering::Less,
            Self::Le => order != Ordering::Greater,
            Self::Gt => order == Ordering::Greater,
            Self::Ge => order != Ordering::Less,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Predicate {
    Compare {
        path: String,
        relation: Relation,
        value: Value,
    },
    CompareFields {
        left: String,
        right: String,
        relation: Relation,
    },
    In {
        path: String,
        values: Vec<Value>,
    },
    Count {
        path: String,
        distinct_by: Option<String>,
        relation: Relation,
        value: u32,
    },
    All {
        predicates: Vec<Predicate>,
    },
    Any {
        predicates: Vec<Predicate>,
    },
    JournalFinality {
        signature_path: String,
        #[serde(default = "positive")]
        expected: bool,
    },
    ScopeIntent {
        path: String,
    },
    CompareObservedAt {
        path: String,
        relation: Relation,
    },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Truth {
    True,
    False,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PredicateResult {
    pub truth: Truth,
    pub gaps: BTreeSet<String>,
}
fn result(value: bool) -> PredicateResult {
    PredicateResult {
        truth: if value { Truth::True } else { Truth::False },
        gaps: BTreeSet::new(),
    }
}
fn unknown(path: &str) -> PredicateResult {
    PredicateResult {
        truth: Truth::Unknown,
        gaps: BTreeSet::from([format!("unavailable:{path}")]),
    }
}
fn unwrapped(mut schema: &ValueType) -> &ValueType {
    while let ValueType::Availability { value } = schema {
        schema = value;
    }
    schema
}
fn path_type<'a>(
    fields: &'a BTreeMap<String, ValueType>,
    path: &str,
) -> Result<&'a ValueType, String> {
    if path.is_empty() || path.len() > 512 {
        return Err("Invalid requirement path".into());
    }
    let mut segments = path.split('.');
    let mut schema = fields
        .get(segments.next().unwrap_or(""))
        .ok_or("Unknown selected fact")?;
    for name in segments {
        schema = match unwrapped(schema) {
            ValueType::Record { fields, .. } => fields.get(name).ok_or("Unknown selected field")?,
            _ => return Err("Requirement path must traverse declared records".into()),
        };
    }
    Ok(unwrapped(schema))
}
fn scalar(schema: &ValueType, ordering: bool) -> bool {
    match schema {
        ValueType::Boolean | ValueType::String { .. } | ValueType::Enum { .. } => !ordering,
        ValueType::Integer { .. } | ValueType::Decimal { .. } => true,
        ValueType::Nullable { value } => !ordering && scalar(value, false),
        _ => false,
    }
}
fn numeric_order(relation: Relation) -> bool {
    !matches!(relation, Relation::Eq | Relation::Ne)
}
pub fn validate(
    requirements: &[Requirement],
    fields: &BTreeMap<String, ValueType>,
) -> Result<(), String> {
    if requirements.len() > 16 {
        return Err("At most 16 requirements are supported".into());
    }
    let mut ids = BTreeSet::new();
    let mut nodes = 0;
    for rule in requirements {
        if !crate::workspace::identifier(&rule.rule_id)
            || rule.version != 1
            || !ids.insert(&rule.rule_id)
        {
            return Err("Invalid or unsupported requirement identity/version".into());
        }
        if rule.immutable_paths.len() + rule.revision_paths.len() + rule.cumulative_sets.len() > 16
        {
            return Err("At most 16 consistency paths are supported per requirement".into());
        }
        for path in rule.immutable_paths.iter().chain(&rule.revision_paths) {
            path_type(fields, path)?;
        }
        for set in &rule.cumulative_sets {
            let ValueType::Array { items, .. } = path_type(fields, &set.path)? else {
                return Err("Cumulative inventory requires an array".into());
            };
            let ValueType::Record { fields: inner, .. } = unwrapped(items) else {
                return Err("Cumulative inventory requires record items".into());
            };
            if !scalar(path_type(inner, &set.key)?, false)
                || !matches!(path_type(fields, &set.complete_path)?, ValueType::Boolean)
            {
                return Err("Invalid cumulative inventory key/completeness schema".into());
            }
        }
        for predicate in [&rule.pass, &rule.fail, &rule.complete] {
            predicate.check(fields, 0, &mut nodes)?;
        }
    }
    Ok(())
}
impl Predicate {
    fn check(
        &self,
        fields: &BTreeMap<String, ValueType>,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<(), String> {
        *nodes += 1;
        if depth > 8 || *nodes > 128 {
            return Err("Requirement exceeds depth/node budget".into());
        }
        match self {
            Self::Compare {
                path,
                relation,
                value,
            } => {
                let schema = path_type(fields, path)?;
                if !scalar(schema, numeric_order(*relation))
                    || super::validation::typed(value, schema).is_err()
                {
                    return Err("Requirement literal/type mismatch".into());
                }
            }
            Self::CompareFields {
                left,
                right,
                relation,
            } => {
                let a = path_type(fields, left)?;
                let b = path_type(fields, right)?;
                if a != b || !scalar(a, numeric_order(*relation)) {
                    return Err("Requirement field types, units, or enums differ".into());
                }
            }
            Self::In { path, values } => {
                let schema = path_type(fields, path)?;
                if !scalar(schema, false)
                    || values.is_empty()
                    || values.len() > 32
                    || values
                        .iter()
                        .any(|v| super::validation::typed(v, schema).is_err())
                {
                    return Err("Invalid typed membership set".into());
                }
            }
            Self::Count {
                path,
                distinct_by,
                value,
                ..
            } => {
                let ValueType::Array { items, .. } = path_type(fields, path)? else {
                    return Err("Count requires a declared array".into());
                };
                if *value > 4000 {
                    return Err("Count literal exceeds array bound".into());
                }
                if let Some(key) = distinct_by {
                    let ValueType::Record { fields, .. } = unwrapped(items) else {
                        return Err("Distinct count requires record items".into());
                    };
                    if !scalar(path_type(fields, key)?, false) {
                        return Err("Distinct key must be scalar".into());
                    }
                }
            }
            Self::CompareObservedAt { path, .. } => {
                if !matches!(path_type(fields,path)?,ValueType::Integer{unit:Some(unit)} if unit=="unix_ms")
                {
                    return Err(
                        "Source timestamp comparison requires declared unix_ms integer".into(),
                    );
                }
            }
            Self::All { predicates } | Self::Any { predicates } => {
                if predicates.is_empty() || predicates.len() > 32 {
                    return Err("Composition requires 1..32 predicates".into());
                }
                for p in predicates {
                    p.check(fields, depth + 1, nodes)?;
                }
            }
            Self::JournalFinality { signature_path, .. }
            | Self::ScopeIntent {
                path: signature_path,
            } => {
                if !matches!(path_type(fields, signature_path)?, ValueType::String { .. }) {
                    return Err("Finality join requires a declared signature string".into());
                }
            }
        }
        Ok(())
    }
    /// Definitions and facts must have passed their typed validation first.
    /// `finality` supplies a retained same-Lab Journal join; it must perform no RPC.
    pub fn evaluate(
        &self,
        fields: &BTreeMap<String, ValueType>,
        facts: &BTreeMap<String, Value>,
        finality: &impl Fn(&str) -> PredicateResult,
    ) -> PredicateResult {
        self.evaluate_in(
            fields,
            facts,
            &Context {
                finality,
                scope: None,
                observed_at_ms: None,
                operational: None,
            },
        )
    }
    pub fn evaluate_in(
        &self,
        fields: &BTreeMap<String, ValueType>,
        facts: &BTreeMap<String, Value>,
        context: &Context<'_>,
    ) -> PredicateResult {
        if context.operational.is_some_and(|check| !check()) {
            return unknown("analysis_interrupted");
        }
        match self {
            Self::Compare {
                path,
                relation,
                value,
            } => match logical(fields, facts, path) {
                Some(a) => compare(path_type(fields, path).ok(), a, value)
                    .map_or_else(|| unknown(path), |o| result(relation.accepts(o))),
                None => unknown(path),
            },
            Self::CompareFields {
                left,
                right,
                relation,
            } => match (logical(fields, facts, left), logical(fields, facts, right)) {
                (Some(a), Some(b)) => compare(path_type(fields, left).ok(), a, b)
                    .map_or_else(|| unknown(left), |o| result(relation.accepts(o))),
                _ => unknown(&format!("{left},{right}")),
            },
            Self::In { path, values } => {
                match logical(fields, facts, path) {
                    Some(a) => result(values.iter().any(|b| {
                        compare(path_type(fields, path).ok(), a, b) == Some(Ordering::Equal)
                    })),
                    None => unknown(path),
                }
            }
            Self::Count {
                path,
                distinct_by,
                relation,
                value,
            } => {
                let Some(items) = logical(fields, facts, path).and_then(Value::as_array) else {
                    return unknown(path);
                };
                let count = if let Some(key) = distinct_by {
                    let Ok(ValueType::Array { items: schema, .. }) = path_type(fields, path) else {
                        return unknown(path);
                    };
                    let ValueType::Record { fields: inner, .. } = unwrapped(schema) else {
                        return unknown(path);
                    };
                    let mut distinct: Vec<&Value> = Vec::with_capacity(items.len());
                    for item in items {
                        let Some(value) = record_value(schema, item, key) else {
                            return unknown(path);
                        };
                        distinct.push(value);
                    }
                    let schema = path_type(inner, key).ok();
                    distinct
                        .sort_unstable_by(|a, b| compare(schema, a, b).unwrap_or(Ordering::Equal));
                    distinct.dedup_by(|a, b| compare(schema, a, b) == Some(Ordering::Equal));
                    distinct.len()
                } else {
                    items.len()
                };
                result(relation.accepts(count.cmp(&(*value as usize))))
            }
            Self::All { predicates } | Self::Any { predicates } => {
                let all = matches!(self, Self::All { .. });
                let mut gaps = BTreeSet::new();
                let mut positive = false;
                let mut negative = false;
                let mut missing = false;
                for p in predicates {
                    let r = p.evaluate_in(fields, facts, context);
                    gaps.extend(r.gaps);
                    match r.truth {
                        Truth::True => positive = true,
                        Truth::False => negative = true,
                        Truth::Unknown => missing = true,
                    }
                }
                let truth = if all && negative {
                    Truth::False
                } else if !all && positive {
                    Truth::True
                } else if missing {
                    Truth::Unknown
                } else if all {
                    Truth::True
                } else {
                    Truth::False
                };
                PredicateResult { truth, gaps }
            }
            Self::JournalFinality {
                signature_path,
                expected,
            } => {
                let mut r = logical(fields, facts, signature_path)
                    .and_then(Value::as_str)
                    .map_or_else(|| unknown(signature_path), context.finality);
                if !expected {
                    r.truth = match r.truth {
                        Truth::True => Truth::False,
                        Truth::False => Truth::True,
                        Truth::Unknown => Truth::Unknown,
                    };
                }
                r
            }
            Self::ScopeIntent { path } => match (
                logical(fields, facts, path).and_then(Value::as_str),
                context.scope,
            ) {
                (Some(id), Some(scope)) if scope.intent_ids.len() == 1 => {
                    result(scope.intent_ids[0] == id)
                }
                _ => unknown("intent_scope_mapping"),
            },
            Self::CompareObservedAt { path, relation } => {
                match (context.observed_at_ms, logical(fields, facts, path)) {
                    (Some(at), Some(value)) => {
                        compare(path_type(fields, path).ok(), &json_number(at), value)
                            .map_or_else(|| unknown(path), |order| result(relation.accepts(order)))
                    }
                    _ => unknown("source_time_basis"),
                }
            }
        }
    }
}
fn available(value: &Value) -> Option<&Value> {
    if value.get("availability")?.as_str()? == "available" {
        value.get("value")
    } else {
        None
    }
}
fn record_value<'a>(mut schema: &ValueType, mut value: &'a Value, path: &str) -> Option<&'a Value> {
    for name in path.split('.').filter(|s| !s.is_empty()) {
        while let ValueType::Availability { value: inner } = schema {
            value = available(value)?;
            schema = inner;
        }
        let ValueType::Record { fields, .. } = schema else {
            return None;
        };
        schema = fields.get(name)?;
        value = value.get(name)?;
    }
    while let ValueType::Availability { value: inner } = schema {
        value = available(value)?;
        schema = inner;
    }
    Some(value)
}
pub(crate) fn logical<'a>(
    fields: &BTreeMap<String, ValueType>,
    facts: &'a BTreeMap<String, Value>,
    path: &str,
) -> Option<&'a Value> {
    let (first, rest) = path.split_once('.').unwrap_or((path, ""));
    record_value(fields.get(first)?, available(facts.get(first)?)?, rest)
}
fn compare(schema: Option<&ValueType>, a: &Value, b: &Value) -> Option<Ordering> {
    let schema = schema?;
    if a.is_null() || b.is_null() {
        return if a == b {
            Some(Ordering::Equal)
        } else {
            Some(if a.is_null() {
                Ordering::Less
            } else {
                Ordering::Greater
            })
        };
    }
    match schema {
        ValueType::Integer { .. } => Some(super::number::compare(&a.to_string(), &b.to_string())),
        ValueType::Decimal { .. } => Some(super::number::compare(a.as_str()?, b.as_str()?)),
        ValueType::Nullable { value } => compare(Some(value), a, b),
        _ => Some(a.to_string().cmp(&b.to_string())),
    }
}

pub(crate) fn same_value(
    fields: &BTreeMap<String, ValueType>,
    path: &str,
    a: &Value,
    b: &Value,
) -> bool {
    match path_type(fields, path) {
        Ok(schema) if scalar(schema, false) => compare(Some(schema), a, b) == Some(Ordering::Equal),
        _ => a == b,
    }
}

fn json_number(value: i64) -> Value {
    Value::from(value)
}

pub(crate) fn cumulative_contains(
    fields: &BTreeMap<String, ValueType>,
    set: &CumulativeSet,
    old: &Value,
    new: &Value,
) -> Option<bool> {
    let ValueType::Array { items, .. } = path_type(fields, &set.path).ok()? else {
        return None;
    };
    let ValueType::Record { fields: inner, .. } = unwrapped(items) else {
        return None;
    };
    let schema = path_type(inner, &set.key).ok()?;
    let sorted = |value: &Value| -> Option<Vec<Value>> {
        let mut keys = value
            .as_array()?
            .iter()
            .map(|item| record_value(items, item, &set.key).cloned())
            .collect::<Option<Vec<_>>>()?;
        keys.sort_unstable_by(|a, b| compare(Some(schema), a, b).unwrap_or(Ordering::Equal));
        keys.dedup_by(|a, b| compare(Some(schema), a, b) == Some(Ordering::Equal));
        Some(keys)
    };
    let old = sorted(old)?;
    let new = sorted(new)?;
    let mut position = 0;
    for value in &old {
        while position < new.len()
            && compare(Some(schema), &new[position], value) == Some(Ordering::Less)
        {
            position += 1;
        }
        if position == new.len()
            || compare(Some(schema), &new[position], value) != Some(Ordering::Equal)
        {
            return Some(false);
        }
    }
    Some(true)
}
