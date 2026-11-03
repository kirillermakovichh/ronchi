use ronchi::{
    cases::custom::{self, ValidationError},
    config::Mode,
};
use serde_json::{Value, json};
fn artifact() -> Value {
    json!({"schema":"ronchi.case/v1","id":"custom.lookup-gap","revision":"1","requires":{"primitives":["rpc.lookup-null@1","observe.rpc@1"],"finding_templates":["lookup-hidden-then-queried@1"]},"modes":["simulator"],"filter":{"methods":["getSignatureStatuses","getTransaction"],"transaction_origin":"accepted_under_this_activation"},"state":{"key":"transaction_signature","scope":"activation","max_keys":1024,"on_exhausted":"error"},"sequence":[{"id":"hide","using":"rpc.lookup-null@1","until":{"elapsed_ms":10000,"clock":"simulator"},"next":"observe"},{"id":"observe","using":"observe.rpc@1","until":{"matches":1},"next":"done"}],"findings":[{"id":"hidden-then-queried","template":"lookup-hidden-then-queried@1","facts":{"hidden":{"kind":"lookup_hidden","step":"hide"},"later":{"kind":"lookup_received","step":"observe"}},"same":"transaction_signature","order":["hidden","later"],"within_ms":60000}]})
}
fn validate(value: &Value) -> Result<custom::Plan, ValidationError> {
    custom::validate(&value.to_string(), Mode::Simulator)
}
#[test]
fn resolves_inspectable_composite_plan_without_effects() {
    let plan = validate(&artifact()).unwrap();
    assert_eq!(plan.artifact.sequence.len(), 2);
    assert_eq!(
        plan.resolved_primitives,
        vec!["observe.rpc@1", "rpc.lookup-null@1"]
    );
    assert_eq!(plan.digest.len(), 64);
    assert_eq!(
        plan.artifact.findings[0].template,
        "lookup-hidden-then-queried@1"
    );
    assert!(plan.revalidate(Mode::Simulator).is_ok());
}

const YAML: &str = r#"schema: ronchi.case/v1
id: custom.lookup-gap
revision: "1"
requires:
  primitives: [rpc.lookup-null@1, observe.rpc@1]
  finding_templates: [lookup-hidden-then-queried@1]
modes: [simulator]
filter:
  methods: [getSignatureStatuses, getTransaction]
  transaction_origin: accepted_under_this_activation
state: {key: transaction_signature, scope: activation, max_keys: 1024, on_exhausted: error}
sequence:
  - id: hide
    using: rpc.lookup-null@1
    until: {elapsed_ms: 10000, clock: simulator}
    next: observe
  - id: observe
    using: observe.rpc@1
    until: {matches: 1}
    next: done
findings:
  - id: hidden-then-queried
    template: lookup-hidden-then-queried@1
    facts:
      hidden: {kind: lookup_hidden, step: hide}
      later: {kind: lookup_received, step: observe}
    same: transaction_signature
    order: [hidden, later]
    within_ms: 60000
"#;
fn failure(value: Value, category: &str, path: &str) {
    let error = validate(&value).unwrap_err();
    assert_eq!(error.category, category, "{error}");
    assert!(error.path.starts_with(path), "{error}");
    assert!(error.message.len() <= 512);
    assert!(error.path.len() <= 256);
}
#[test]
fn block_yaml_comments_and_mapping_order_have_the_same_canonical_digest() {
    let json = validate(&artifact()).unwrap();
    let yaml = custom::validate(YAML, Mode::Simulator).unwrap();
    assert_eq!(json.digest, yaml.digest);
    assert_eq!(yaml.canonical_input, artifact());
    let commented = format!(
        "# !include and &anchor in comments are inert\n%YAML 1.2\n---\n{YAML}\n# trailing comment\n"
    );
    assert_eq!(
        custom::validate(&commented, Mode::Simulator)
            .unwrap()
            .digest,
        json.digest
    );
    assert_eq!(
        custom::validate(
            &YAML.replace("max_keys: 1024", "max_keys: 0x400"),
            Mode::Simulator
        )
        .unwrap()
        .digest,
        json.digest
    );
    assert!(yaml.artifact.sequence[0].with.is_object());
    assert!(yaml.canonical_input["sequence"][0].get("with").is_none());
    let mut explicit = artifact();
    explicit["sequence"][0]["with"] = json!({});
    assert_ne!(validate(&explicit).unwrap().digest, json.digest);
    let mut a = artifact();
    a["description"] = json!("é");
    let mut b = a.clone();
    b["description"] = json!("e\u{301}");
    assert_ne!(validate(&a).unwrap().digest, validate(&b).unwrap().digest);
    assert_eq!(validate(&a).unwrap().canonical_input["description"], "é");
}
#[test]
fn rejects_yaml_features_before_they_can_change_the_artifact() {
    let cases = [
        (format!("{YAML}\nid: other\n"), "id"),
        (
            YAML.replace("revision: \"1\"", "revision: &r \"1\""),
            "revision",
        ),
        (YAML.replace("revision: \"1\"", "revision: *r"), "revision"),
        (
            YAML.replace("revision: \"1\"", "revision: !!str 1"),
            "revision",
        ),
        (YAML.replace("revision: \"1\"", "revision: 1.5"), "revision"),
        (
            YAML.replace("revision: \"1\"", "revision: .nan"),
            "revision",
        ),
        (
            YAML.replace("revision: \"1\"", "revision: 2026-11-04"),
            "revision",
        ),
        (
            YAML.replace("revision: \"1\"", "revision: null"),
            "revision",
        ),
        (
            YAML.replace("revision: \"1\"", "revision: NULL"),
            "revision",
        ),
        (
            YAML.replace("until: {matches: 1}", "until: {matches: 1, matches: 2}"),
            "sequence[1].until.matches",
        ),
        (format!("<<: {{schema: ronchi.case/v1}}\n{YAML}"), "<<"),
        (format!("{YAML}\n---\n{YAML}"), "$"),
        (format!("%YAML 1.1\n---\n{YAML}"), "$"),
        (format!("%TAG !p! tag:example.test,2026:\n---\n{YAML}"), "$"),
        ("? [complex, key]\n: value".into(), "$"),
        ("{unclosed: [".into(), "unclosed"),
    ];
    for (source, path) in cases {
        let error = custom::validate(&source, Mode::Simulator).unwrap_err();
        assert_eq!(error.category, "invalid_yaml", "{source}: {error}");
        assert!(error.path.starts_with(path), "{error}");
    }
    assert!(
        custom::validate(
            &YAML.replace("revision: \"1\"", "revision: \"2026-11-04\""),
            Mode::Simulator
        )
        .is_ok()
    );
}
#[test]
fn every_stable_validation_category_has_a_specific_bounded_failure() {
    let mut v = artifact();
    v["schema"] = json!("ronchi.case/v2");
    failure(v, "unsupported_schema", "schema");
    let mut v = artifact();
    v["script"] = json!("do not execute");
    failure(v, "unknown_field", "script");
    let mut v = artifact();
    v["requires"]["primitives"][0] = json!("provider.custom-write@1");
    failure(v, "unknown_primitive", "requires.primitives[0]");
    let mut v = artifact();
    v["requires"]["primitives"][0] = json!("rpc.lookup-null@2");
    failure(v, "incompatible_version", "requires.primitives[0]");
    let mut v = artifact();
    v["modes"] = json!(["surfpool"]);
    failure(v, "unsupported_mode", "modes");
    let mut v = artifact();
    v["filter"]["methods"] = json!(["providerWrite"]);
    failure(v, "invalid_filter", "filter.methods");
    let mut v = artifact();
    v["sequence"][0]["next"] = json!("missing");
    failure(v, "invalid_sequence", "sequence[0].next");
    let mut v = artifact();
    v["findings"][0]["same"] = json!("economic_intent");
    failure(v, "invalid_finding_rule", "findings[0]");
    let mut v = artifact();
    v["state"]["max_keys"] = json!(1025);
    failure(v, "limit_exceeded", "state.max_keys");
    let mut v = artifact();
    v["sequence"][0]["until"]["elapsed_ms"] = json!(1.5);
    failure(v, "invalid_yaml", "sequence[0].until.elapsed_ms");
}
#[test]
fn exact_limits_and_untrusted_plan_revalidation_preserve_the_original_contract() {
    let padding = custom::MAX_ARTIFACT_BYTES - YAML.len() - 2;
    let padded = format!("{YAML}#{}\n", "x".repeat(padding));
    assert_eq!(padded.len(), custom::MAX_ARTIFACT_BYTES);
    let plan = custom::validate(&padded, Mode::Simulator).unwrap();
    assert_eq!(plan.digest, validate(&artifact()).unwrap().digest);
    assert_eq!(
        custom::validate(&(padded + " "), Mode::Simulator)
            .unwrap_err()
            .category,
        "limit_exceeded"
    );
    let decoded: custom::Plan =
        serde_json::from_value(serde_json::to_value(&plan).unwrap()).unwrap();
    assert_eq!(
        decoded.revalidate(Mode::Simulator).unwrap().digest,
        plan.digest
    );
    let mut forged = plan.clone();
    forged.digest = "0".repeat(64);
    assert_eq!(
        forged.revalidate(Mode::Simulator).unwrap_err().category,
        "incompatible_version"
    );
    let mut forged = plan.clone();
    forged.artifact.state.max_keys = 1;
    assert_eq!(
        forged.revalidate(Mode::Simulator).unwrap_err().category,
        "incompatible_version"
    );
    let mut forged = plan.clone();
    forged.resolved_primitives.clear();
    assert_eq!(
        forged.revalidate(Mode::Simulator).unwrap_err().category,
        "incompatible_version"
    );
    let mut forged = plan;
    forged.canonical_input["sequence"][0]["until"]["elapsed_ms"] = json!(3.0);
    assert_eq!(
        forged.revalidate(Mode::Simulator).unwrap_err().category,
        "invalid_yaml"
    );
}
#[test]
fn positive_repetition_is_bounded_reachable_and_has_no_zero_time_cycle() {
    let mut v = artifact();
    v["sequence"][1]["next"] = json!("hide");
    v["repeat"] = json!({"cycles":100});
    assert!(validate(&v).is_ok());
    v["repeat"] = json!({"while_active":true});
    assert!(validate(&v).is_ok());
    let mut invalid = v.clone();
    invalid["sequence"][0]["until"] = json!({"matches":1});
    failure(invalid, "invalid_sequence", "repeat");
    let mut invalid = v.clone();
    invalid["repeat"] = json!({"cycles":101});
    failure(invalid, "limit_exceeded", "repeat.cycles");
    let mut invalid = v.clone();
    invalid["repeat"] = json!({"cycles":2,"while_active":true});
    failure(invalid, "invalid_sequence", "repeat");
    let mut invalid = v.clone();
    invalid["sequence"][0]["next"] = json!("hide");
    failure(invalid, "invalid_sequence", "sequence");
    let mut invalid = artifact();
    invalid["sequence"][0]["next"] = json!("done");
    failure(invalid, "invalid_sequence", "sequence");
    let mut invalid = artifact();
    invalid["sequence"][0]["until"] = json!({"matches":1,"elapsed_ms":1,"clock":"real"});
    failure(invalid, "invalid_sequence", "sequence[0].until");
}
