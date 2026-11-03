use super::{Definition, SourceRequest, ValueType};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceResponse {
    pub contract_version: u32,
    pub invocation_id: String,
    pub source: String,
    pub schema_version: u32,
    pub observed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub facts: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<super::Scope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_relation: Option<String>,
}
/// Returns only configured typed facts; extras of projected records are never retained.
pub fn validate(
    definition: &Definition,
    request: &SourceRequest,
    mut response: SourceResponse,
) -> Result<SourceResponse, &'static str> {
    if response.contract_version != 1
        || response.invocation_id != request.invocation_id
        || response.source != definition.source
        || response.schema_version != definition.schema_version
        || response
            .check_id
            .as_ref()
            .is_some_and(|s| s != &request.check_id)
        || response
            .lab_id
            .as_ref()
            .is_some_and(|s| s != &request.lab_id)
        || response
            .snapshot_seq
            .is_some_and(|s| s != request.snapshot_seq)
        || response.scope.as_ref().is_some_and(|s| s != &request.scope)
    {
        return Err("source_identity");
    }
    if response.observed_at.len() > 64
        || time::OffsetDateTime::parse(
            &response.observed_at,
            &time::format_description::well_known::Rfc3339,
        )
        .is_err()
        || response.revision.as_ref().is_some_and(|s| s.len() > 128)
        || response
            .snapshot_relation
            .as_ref()
            .is_some_and(|s| s != "application_only")
    {
        return Err("source_metadata");
    }
    if response.facts.len() != definition.facts.len() {
        return Err("fact_schema");
    }
    let mut count = 0;
    count_values(
        &serde_json::to_value(&response.facts).map_err(|_| "fact_schema")?,
        &mut count,
        definition.limits.max_facts,
    )?;
    let mut facts = BTreeMap::new();
    for (name, schema) in &definition.facts {
        facts.insert(
            name.clone(),
            fact(response.facts.get(name).ok_or("fact_schema")?, schema)?,
        );
    }
    response.facts = facts;
    Ok(response)
}
fn count_values(value: &Value, count: &mut usize, max: usize) -> Result<(), &'static str> {
    *count += 1;
    if *count > max {
        return Err("fact_limit");
    }
    match value {
        Value::Array(a) => {
            for v in a {
                count_values(v, count, max)?;
            }
        }
        Value::Object(o) => {
            for v in o.values() {
                count_values(v, count, max)?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn fact(value: &Value, schema: &ValueType) -> Result<Value, &'static str> {
    let object = value.as_object().ok_or("fact_schema")?;
    if object
        .keys()
        .any(|s| !["availability", "value", "reason"].contains(&s.as_str()))
    {
        return Err("fact_schema");
    }
    if object
        .get("reason")
        .is_some_and(|v| v.as_str().is_none_or(|s| s.len() > 256))
    {
        return Err("fact_schema");
    }
    match value["availability"].as_str() {
        Some("available") => {
            let typed = typed(object.get("value").ok_or("fact_schema")?, schema)?;
            Ok(serde_json::json!({"availability":"available","value":typed}))
        }
        Some("unavailable" | "redacted") => {
            if object.contains_key("value") {
                return Err("fact_schema");
            }
            Ok(value.clone())
        }
        _ => Err("fact_schema"),
    }
}
fn typed(value: &Value, schema: &ValueType) -> Result<Value, &'static str> {
    let valid = match schema {
        ValueType::Boolean => value.is_boolean(),
        ValueType::Integer { .. } => value.is_number() && integer(&value.to_string()),
        ValueType::Decimal { .. } => value.as_str().is_some_and(decimal),
        ValueType::String { max_bytes } => value.as_str().is_some_and(|s| s.len() <= *max_bytes),
        ValueType::Enum { values } => value
            .as_str()
            .is_some_and(|s| values.iter().any(|v| v == s)),
        ValueType::Array { max_items, items } => {
            let values = value
                .as_array()
                .filter(|a| a.len() <= *max_items)
                .ok_or("fact_schema")?;
            return values
                .iter()
                .map(|v| typed(v, items))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array);
        }
        ValueType::Record { fields, exact } => {
            let object = value.as_object().ok_or("fact_schema")?;
            if *exact && object.len() != fields.len() {
                return Err("fact_schema");
            }
            let mut selected = serde_json::Map::new();
            for (name, schema) in fields {
                selected.insert(
                    name.clone(),
                    typed(object.get(name).ok_or("fact_schema")?, schema)?,
                );
            }
            return Ok(Value::Object(selected));
        }
        ValueType::Nullable { value: inner } => {
            if value.is_null() {
                return Ok(Value::Null);
            }
            return typed(value, inner);
        }
        ValueType::Availability { value: inner } => return fact(value, inner),
    };
    if valid {
        Ok(value.clone())
    } else {
        Err("fact_schema")
    }
}
fn integer(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}
fn decimal(value: &str) -> bool {
    if value.is_empty() || value.len() > 256 {
        return false;
    }
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let mut parts = unsigned.split(['e', 'E']);
    let mantissa = parts.next().unwrap_or("");
    let exponent = parts.next();
    if parts.next().is_some() {
        return false;
    }
    let mut m = mantissa.split('.');
    let whole = m.next().unwrap_or("");
    let fraction = m.next();
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || m.next().is_some()
        || fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    exponent.is_none_or(|s| {
        let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
    })
}
