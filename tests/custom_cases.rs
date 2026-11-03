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
fn one_step(primitive: &str, method: &str, key: &str) -> Value {
    let mut value = artifact();
    value["requires"] = json!({"primitives":[primitive]});
    value["filter"]["methods"] = json!([method]);
    value["state"]["key"] = json!(key);
    value["sequence"] =
        json!([{"id":"select","using":primitive,"until":{"matches":1},"next":"done"}]);
    value.as_object_mut().unwrap().remove("findings");
    value
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
fn yaml_core_integer_forms_have_the_same_bounded_semantics() {
    for number in [
        "0x8000000000000000",
        "0o1000000000000000000000",
        "9223372036854775808",
        "-9223372036854775809",
    ] {
        let source = YAML.replace("revision: \"1\"", &format!("revision: {number}"));
        let error = custom::validate(&source, Mode::Simulator).unwrap_err();
        assert_eq!(error.category, "limit_exceeded", "{number}: {error}");
        assert_eq!(error.path, "revision");
    }
    // Several leading signs form a string under YAML 1.2, rather than an integer.
    let source = YAML.replace("revision: \"1\"", "revision: --1");
    assert_eq!(
        custom::validate(&source, Mode::Simulator)
            .unwrap()
            .artifact
            .revision,
        "--1"
    );
    for spelling in ["0x-1", "0o+1", "+-1"] {
        let source = format!("{YAML}\ndescription: {spelling}\n");
        assert_eq!(
            custom::validate(&source, Mode::Simulator)
                .unwrap()
                .artifact
                .description
                .as_deref(),
            Some(spelling)
        );
    }
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

#[test]
fn every_published_primitive_resolves_catalog_defaults_and_rejects_extra_arguments() {
    let catalog = custom::catalog();
    let entries = catalog["primitives"].as_array().unwrap();
    assert_eq!(entries.len(), 7);
    for entry in entries {
        let id = entry["id"].as_str().unwrap();
        let method = entry["methods"][0].as_str().unwrap();
        let value = one_step(id, method, "case");
        let plan = validate(&value).unwrap_or_else(|error| panic!("{id}: {error}"));
        let arguments = entry["arguments"].as_object().unwrap();
        for (name, spec) in arguments {
            assert_eq!(plan.artifact.sequence[0].with[name], spec["default"]);
            let mut explicit = value.clone();
            explicit["sequence"][0]["with"] = json!({});
            if name == "hold_at" {
                for hold_at in ["processed", "confirmed"] {
                    explicit["sequence"][0]["with"][name] = json!(hold_at);
                    assert!(validate(&explicit).is_ok(), "{id}/{name}={hold_at}");
                }
                explicit["sequence"][0]["with"][name] = json!("finalized");
                failure(explicit, "invalid_sequence", "sequence[0].with.hold_at");
            } else {
                for bound in [1, 120000] {
                    explicit["sequence"][0]["with"][name] = json!(bound);
                    assert!(validate(&explicit).is_ok(), "{id}/{name}={bound}");
                }
                for invalid in [0, 120001] {
                    explicit["sequence"][0]["with"][name] = json!(invalid);
                    failure(explicit.clone(), "limit_exceeded", "sequence[0].with");
                }
            }
        }
        let mut unknown = value;
        unknown["sequence"][0]["with"] = json!({"provider_script":"ignored"});
        failure(unknown, "unknown_field", "sequence[0].with.provider_script");
        assert_eq!(entry["argument_schema"]["additionalProperties"], false);
        for (name, spec) in arguments {
            assert_eq!(
                entry["argument_schema"]["properties"][name]["default"],
                spec["default"]
            );
        }
    }
}

#[test]
fn stage_filters_state_keys_and_modes_match_the_registered_capabilities() {
    let mut composite = one_step(
        "rpc.withhold-submission-response@1",
        "sendTransaction",
        "transaction_signature",
    );
    composite["requires"]["primitives"] =
        json!(["rpc.withhold-submission-response@1", "observe.rpc@1"]);
    composite["filter"]["methods"] = json!(["sendTransaction", "getTransaction"]);
    composite["sequence"][0]["next"] = json!("lookup");
    composite["sequence"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"lookup","using":"observe.rpc@1","until":{"matches":1},"next":"done"}));
    assert!(validate(&composite).is_ok());
    for primitive in [
        "rpc.withhold-submission-response@1",
        "sim.confirmation-delay@1",
    ] {
        failure(
            one_step(primitive, "getTransaction", "case"),
            "invalid_filter",
            "sequence[0].using",
        );
        assert!(validate(&one_step(primitive, "sendTransaction", "case")).is_ok());
    }
    failure(
        one_step(
            "rpc.blockhash-reuse@1",
            "getLatestBlockhash",
            "transaction_signature",
        ),
        "invalid_filter",
        "state.key",
    );
    failure(
        one_step("observe.rpc@1", "getHealth", "transaction_signature"),
        "invalid_filter",
        "sequence[0].using",
    );
    for method in ["getHealth", "getGenesisHash"] {
        let mut value = one_step("observe.rpc@1", method, "case");
        value["filter"]["commitments"] = json!(["confirmed"]);
        failure(value, "invalid_filter", "filter.commitments");
    }
    let mut unrestricted = one_step("observe.rpc@1", "getHealth", "case");
    unrestricted["filter"]
        .as_object_mut()
        .unwrap()
        .remove("methods");
    unrestricted["filter"]["commitments"] = json!(["processed", "confirmed", "finalized"]);
    assert!(validate(&unrestricted).is_ok());
    for method in ["simulateTransaction", "requestAirdrop"] {
        let mut value = one_step("observe.rpc@1", method, "case");
        failure(value.clone(), "unsupported_mode", "filter.methods[0]");
        value["modes"] = json!(["surfpool"]);
        assert!(custom::validate(&value.to_string(), Mode::Surfpool).is_ok());
    }
    let mut surfpool = one_step("sim.confirmation-delay@1", "sendTransaction", "case");
    surfpool["modes"] = json!(["simulator", "surfpool"]);
    failure(surfpool, "unsupported_mode", "requires.primitives[0]");
    let mut blocked_stage = one_step(
        "rpc.withhold-submission-response@1",
        "sendTransaction",
        "case",
    );
    blocked_stage["requires"]["primitives"] = json!([
        "rpc.withhold-submission-response@1",
        "rpc.blockhash-reuse@1"
    ]);
    blocked_stage["filter"]["methods"] = json!(["sendTransaction", "getLatestBlockhash"]);
    blocked_stage["filter"]["signatures"] = json!([bs58::encode([7u8; 64]).into_string()]);
    blocked_stage["sequence"][0]["next"] = json!("blockhash");
    blocked_stage["sequence"].as_array_mut().unwrap().push(json!({"id":"blockhash","using":"rpc.blockhash-reuse@1","until":{"matches":1},"next":"done"}));
    failure(blocked_stage, "invalid_filter", "sequence[1].using");
}

#[test]
fn input_ceiling_boundaries_are_enforced_before_a_plan_can_be_activated() {
    let mut value = one_step("observe.rpc@1", "getTransaction", "transaction_signature");
    let signature = bs58::encode([7u8; 64]).into_string();
    value["filter"]["signatures"] = json!(vec![signature; 32]);
    value["sequence"][0]["until"] = json!({"matches":10000});
    value["state"]["max_keys"] = json!(1024);
    assert!(validate(&value).is_ok());
    let mut invalid = value.clone();
    invalid["filter"]["signatures"]
        .as_array_mut()
        .unwrap()
        .push(json!(bs58::encode([8u8; 64]).into_string()));
    failure(invalid, "limit_exceeded", "filter.signatures");
    let mut invalid = value.clone();
    invalid["filter"]["signatures"] = json!(["not-a-signature"]);
    failure(invalid, "invalid_filter", "filter.signatures[0]");
    for invalid in [0, 10001] {
        let mut bad = value.clone();
        bad["sequence"][0]["until"]["matches"] = json!(invalid);
        failure(bad, "limit_exceeded", "sequence[0].until.matches");
    }
    value["sequence"][0]["until"] = json!({"elapsed_ms":120000,"clock":"real"});
    assert!(validate(&value).is_ok());
    value["sequence"][0]["until"]["elapsed_ms"] = json!(120001);
    failure(
        value.clone(),
        "limit_exceeded",
        "sequence[0].until.elapsed_ms",
    );
    value["sequence"] = Value::Array((0..8).map(|index| json!({"id":format!("step{index}"),"using":"observe.rpc@1","until":{"matches":1},"next":if index==7 {"done".into()} else {format!("step{}",index+1)}})).collect());
    assert!(validate(&value).is_ok());
    value["sequence"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"extra","using":"observe.rpc@1","until":{"matches":1},"next":"done"}));
    failure(value, "limit_exceeded", "sequence");
    let mut rules = artifact();
    let rule = rules["findings"][0].clone();
    rules["findings"] = Value::Array(
        (0..8)
            .map(|index| {
                let mut rule = rule.clone();
                rule["id"] = json!(format!("rule{index}"));
                rule["within_ms"] = json!(300000);
                rule
            })
            .collect(),
    );
    assert!(validate(&rules).is_ok());
    rules["findings"].as_array_mut().unwrap().push(rule);
    failure(rules, "limit_exceeded", "findings");
}

#[test]
fn finding_rules_require_installed_templates_and_compatible_ordered_facts() {
    let mut value = artifact();
    value["findings"][0]["facts"]["hidden"]["kind"] = json!("rpc_received");
    failure(value, "invalid_finding_rule", "findings[0]");
    let mut value = artifact();
    value["requires"]["primitives"][0] = json!("rpc.lookup-snapshot@1");
    value["sequence"][0]["using"] = json!("rpc.lookup-snapshot@1");
    // A snapshot can satisfy the hidden fact only when runtime evidence proves
    // that an available authoritative result changed to null at the boundary.
    assert!(validate(&value).is_ok());
    let mut value = artifact();
    value["findings"][0]["facts"]["hidden"]["step"] = json!("observe");
    failure(value, "invalid_finding_rule", "findings[0]");
    let mut value = artifact();
    value["findings"][0]["facts"]["invented"] = json!({"kind":"lookup_received","step":"observe"});
    failure(value, "invalid_finding_rule", "findings[0]");
    let mut value = artifact();
    value["findings"][0]["order"] = json!(["later", "hidden"]);
    failure(value, "invalid_finding_rule", "findings[0]");
    let mut value = artifact();
    value["findings"][0]["template"] = json!("lookup-hidden-then-queried@2");
    failure(value, "incompatible_version", "findings[0].template");
    let mut value = artifact();
    value["requires"]["finding_templates"] = json!([]);
    failure(value, "incompatible_version", "findings[0].template");
    for invalid in [0, 300001] {
        let mut value = artifact();
        value["findings"][0]["within_ms"] = json!(invalid);
        failure(value, "limit_exceeded", "findings[0].within_ms");
    }
}
