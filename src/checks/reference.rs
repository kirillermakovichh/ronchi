//! Maintained reference requirements expressed through the same public definitions.
use super::Definition;
use serde_json::{Value, json};
fn compare(path: &str, relation: &str, value: Value) -> Value {
    json!({"compare":{"path":path,"relation":relation,"value":value}})
}
fn fields(left: &str, relation: &str, right: &str) -> Value {
    json!({"compare_fields":{"left":left,"relation":relation,"right":right}})
}
fn all(predicates: Vec<Value>) -> Value {
    json!({"all":{"predicates":predicates}})
}
/// Pure configuration fixture. It performs no source calls or evaluation.
pub fn definition(url: String) -> Definition {
    let mapping = json!({"scope_intent":{"path":"intent.intent_id"}});
    let full = compare("intent.prepared_set_complete", "eq", json!(true));
    let closed = all(vec![
        compare("intent.interval_closed", "eq", json!(true)),
        json!({"in":{"path":"intent.state","values":["succeeded","failed","unresolved"]}}),
    ]);
    let time_basis = compare("intent.time_basis", "eq", json!("application_unix_ms"));
    let consistent_time = fields("intent.deadline_ms", "ge", "intent.accepted_at_ms");
    let succeeded = compare("intent.state", "eq", json!("succeeded"));
    let resolved = json!({"in":{"path":"intent.state","values":["succeeded","failed"]}});
    let count = |relation: &str| json!({"count":{"path":"intent.prepared_identities","distinct_by":"signature","relation":relation,"value":1}});
    let finality = |expected: bool| json!({"journal_finality":{"signature_path":"intent.original_signature","expected":expected}});
    let identities = json!({"rule_id":"one-prepared-identity","version":1,"pass":all(vec![mapping.clone(),full.clone(),count("le")]),"fail":all(vec![mapping.clone(),count("gt")]),"complete":all(vec![mapping.clone(),full,closed.clone()]),"immutable_paths":["intent.intent_id"],"revision_paths":["intent.prepared_identities","intent.interval_closed"]});
    let finality = json!({"rule_id":"original-finality","version":1,"pass":all(vec![mapping.clone(),succeeded.clone(),finality(true)]),"fail":{"any":{"predicates":[all(vec![mapping.clone(),compare("intent.state","eq",json!("failed"))]),all(vec![mapping.clone(),succeeded,finality(false)])]}},"complete":all(vec![mapping.clone(),closed.clone()]),"immutable_paths":["intent.intent_id","intent.original_signature"],"revision_paths":["intent.state"]});
    let resolution = json!({"rule_id":"resolution-horizon","version":1,"pass":all(vec![mapping.clone(),time_basis.clone(),consistent_time.clone(),resolved.clone(),fields("intent.terminal_at_ms","ge","intent.accepted_at_ms"),fields("intent.terminal_at_ms","le","intent.deadline_ms")]),"fail":{"any":{"predicates":[all(vec![mapping.clone(),time_basis.clone(),consistent_time.clone(),compare("intent.state","eq",json!("unresolved")),json!({"compare_observed_at":{"path":"intent.deadline_ms","relation":"ge"}})]),all(vec![mapping.clone(),time_basis.clone(),consistent_time.clone(),resolved,fields("intent.terminal_at_ms","gt","intent.deadline_ms")])]}},"complete":all(vec![mapping,time_basis,consistent_time,closed]),"immutable_paths":["intent.intent_id","intent.accepted_at_ms","intent.deadline_ms"],"revision_paths":["intent.state","intent.terminal_at_ms"]});
    serde_json::from_value(json!({"check_id":"reference-intent","version":1,"source":"reference-relayer","schema_version":1,"url":url,"limits":{"max_facts":4000},"facts":{"intent":{"type":"record","fields":{
        "intent_id":{"type":"string","max_bytes":128},
        "state":{"type":"enum","values":["accepted","prepared","submitting","reconciling","succeeded","failed","unresolved"]},
        "original_signature":{"type":"availability","value":{"type":"string","max_bytes":88}},
        "prepared_identities":{"type":"availability","value":{"type":"array","max_items":2,"items":{"type":"record","fields":{"signature":{"type":"string","max_bytes":88}}}}},
        "accepted_at_ms":{"type":"integer","unit":"unix_ms"},"deadline_ms":{"type":"integer","unit":"unix_ms"},"terminal_at_ms":{"type":"availability","value":{"type":"integer","unit":"unix_ms"}},
        "time_basis":{"type":"enum","values":["application_unix_ms"]},"prepared_set_complete":{"type":"boolean"},"interval_closed":{"type":"boolean"}
    }}},"requirements":[identities,finality,resolution]})).expect("maintained reference definition deserializes")
}
