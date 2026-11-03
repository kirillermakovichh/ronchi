use ronchi::checks::{Config, Definition, Scope, SourceRequest, SourceResponse, validate};
use serde_json::{Value, json};
fn definition() -> Definition {
    serde_json::from_value(json!({"check_id":"state","version":1,"source":"independent-app","schema_version":1,"url":"http://127.0.0.1:1234/observe","facts":{"amount":{"type":"decimal","unit":"tokens"}}})).unwrap()
}
fn request() -> SourceRequest {
    SourceRequest {
        contract_version: 1,
        invocation_id: "I1".into(),
        check_id: "state".into(),
        lab_id: "L1".into(),
        snapshot_seq: 7,
        scope: Scope {
            intent_ids: vec!["W1".into()],
            ..Default::default()
        },
        facts: vec!["amount".into()],
        deadline: "2026-11-03T00:00:00Z".into(),
    }
}
fn response(value: Value) -> SourceResponse {
    serde_json::from_value(json!({"contract_version":1,"invocation_id":"I1","source":"independent-app","schema_version":1,"observed_at":"2026-11-03T00:00:00Z","revision":"17","facts":{"amount":value}})).unwrap()
}
#[test]
fn exact_decimal_strings_and_unavailable_values_survive_without_coercion() {
    let d = definition();
    d.validate().unwrap();
    let r = request();
    let exact = "9007199254740993.0000000000000001";
    let observed = validate(
        &d,
        &r,
        response(json!({"availability":"available","value":exact})),
    )
    .unwrap();
    assert_eq!(observed.facts["amount"]["value"], exact);
    let unavailable = validate(
        &d,
        &r,
        response(json!({"availability":"unavailable","reason":"not_collected"})),
    )
    .unwrap();
    assert!(unavailable.facts["amount"].get("value").is_none());
    for invalid in ["NaN", "Infinity", "--1", "1.", "1e", "1e--2"] {
        assert!(
            validate(
                &d,
                &r,
                response(json!({"availability":"available","value":invalid}))
            )
            .is_err(),
            "{invalid}"
        );
    }
    assert!(
        validate(
            &d,
            &r,
            response(json!({"availability":"available","value":0.1}))
        )
        .is_err()
    );
}
#[test]
fn configured_origins_limits_and_duplicate_names_fail_before_lab_files_exist() {
    let temp = tempfile::tempdir().unwrap();
    let d = definition();
    let mut config = Config {
        sources: vec![d.clone()],
        ..Default::default()
    };
    config.validate().unwrap();
    for url in [
        "http://user:password@127.0.0.1:1234/observe",
        "http://127.0.0.1:1234/observe?token=secret",
        "http://example.test/observe",
    ] {
        config.sources[0].url = url.into();
        assert!(config.validate().is_err());
    }
    config.sources[0] = d.clone();
    config.sources[0].url = "http://example.test/observe".into();
    config.sources[0].allowed_origin = Some("http://example.test".into());
    config.validate().unwrap();
    config.sources[0].allowed_origin = Some("http://other.test".into());
    assert!(config.validate().is_err());
    config.sources = vec![d.clone(), d];
    assert!(config.validate().is_err());
    let overrides = ronchi::config::Settings {
        checks: Some(config),
        ..Default::default()
    };
    assert!(ronchi::config::resolve(temp.path().into(), overrides).is_err());
    assert!(!temp.path().join("ronchi").exists());
}
#[test]
fn source_identity_types_and_payload_availability_are_checked_independently() {
    let d = definition();
    let r = request();
    let mut wrong = response(json!({"availability":"available","value":"1"}));
    wrong.invocation_id = "another".into();
    assert!(validate(&d, &r, wrong).is_err());
    let mut wrong = response(json!({"availability":"available","value":"1"}));
    wrong.source = "another".into();
    assert!(validate(&d, &r, wrong).is_err());
    let mut wrong = response(json!({"availability":"available","value":"1"}));
    wrong.schema_version = 2;
    assert!(validate(&d, &r, wrong).is_err());
    assert!(
        validate(
            &d,
            &r,
            response(json!({"availability":"unavailable","value":false}))
        )
        .is_err()
    );
    let mut record = d;
    record.facts=serde_json::from_value(json!({"amount":{"type":"record","exact":true,"fields":{"enabled":{"type":"boolean"},"count":{"type":"integer","unit":"items"},"state":{"type":"enum","values":["pending","done"]},"names":{"type":"array","max_items":2,"items":{"type":"string","max_bytes":8}}}}})).unwrap();
    let input =
        json!({"enabled":false,"count":9007199254740993u64,"state":"pending","names":["A","B"]});
    let valid = validate(
        &record,
        &r,
        response(json!({"availability":"available","value":input})),
    )
    .unwrap();
    assert_eq!(valid.facts["amount"]["value"]["count"], 9007199254740993u64);
    assert!(validate(&record,&r,response(json!({"availability":"available","value":{"enabled":0,"count":1,"state":"pending","names":[]}}))).is_err());
}
