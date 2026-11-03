use ronchi::checks::{
    Definition,
    rules::{self, Predicate, Truth},
};
use serde_json::{Value, json};
fn source() -> Definition {
    serde_json::from_value(json!({"check_id":"ledger","version":4,"source":"application","schema_version":2,"url":"http://127.0.0.1:9000/observe","facts":{"balance":{"type":"decimal","unit":"atoms"},"other":{"type":"decimal","unit":"atoms"},"count":{"type":"integer"},"enabled":{"type":"boolean"},"identities":{"type":"array","max_items":16,"items":{"type":"record","fields":{"signature":{"type":"string","max_bytes":88}}}}}})).unwrap()
}
fn predicate(mut value: Value) -> Predicate {
    let op = value
        .as_object_mut()
        .unwrap()
        .remove("op")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    serde_json::from_value(json!({op:value})).unwrap()
}
fn check(p: &Predicate, s: &Definition) {
    let r = json!({"rule_id":"bounded","version":1,"pass":p,"fail":p,"complete":p});
    rules::validate(&[serde_json::from_value(r).unwrap()], &s.facts).unwrap();
}
fn evaluate(p: Predicate, facts: Value) -> Truth {
    let s = source();
    check(&p, &s);
    p.evaluate(&s.facts, &serde_json::from_value(facts).unwrap(), &|_| {
        panic!("Unexpected Journal join")
    })
    .truth
}
#[test]
fn exact_order_and_equality_preserve_arbitrary_precision_and_exponents() {
    let p = predicate(
        json!({"op":"compare","path":"balance","relation":"gt","value":"9007199254740992.0000000000000000001"}),
    );
    assert_eq!(
        evaluate(
            p.clone(),
            json!({"balance":{"availability":"available","value":"9007199254740992.0000000000000000002"}})
        ),
        Truth::True
    );
    assert_eq!(
        evaluate(
            p,
            json!({"balance":{"availability":"available","value":"9007199254740992.0000000000000000000"}})
        ),
        Truth::False
    );
    for (a, b, truth) in [
        (
            "1e999999999999999999999999999999999999",
            "10e999999999999999999999999999999999998",
            Truth::True,
        ),
        ("-0.00", "0e-9999999999999999999999999999999", Truth::True),
        ("-1.01", "-1.0100", Truth::True),
        ("10.0", "9.99", Truth::False),
    ] {
        assert_eq!(
            evaluate(
                predicate(json!({"op":"compare","path":"balance","relation":"eq","value":b})),
                json!({"balance":{"availability":"available","value":a}})
            ),
            truth
        );
    }
    let large: Value = serde_json::from_str("123456789012345678901234567890123456789").unwrap();
    assert_eq!(
        evaluate(
            predicate(json!({"op":"compare","path":"count","relation":"eq","value":large})),
            json!({"count":{"availability":"available","value":large}})
        ),
        Truth::True
    );
}

#[test]
fn composition_keeps_unknown_facts_and_does_not_substitute_false() {
    let enabled = predicate(json!({"op":"compare","path":"enabled","relation":"eq","value":true}));
    let balance = predicate(json!({"op":"compare","path":"balance","relation":"gt","value":"0"}));
    let all = Predicate::All {
        predicates: vec![enabled.clone(), balance.clone()],
    };
    let any = Predicate::Any {
        predicates: vec![enabled, balance],
    };
    for availability in ["unavailable", "redacted"] {
        let facts = json!({"enabled":{"availability":availability},"balance":{"availability":"available","value":"1"}});
        assert_eq!(evaluate(all.clone(), facts.clone()), Truth::Unknown);
        let source = source();
        let r = any.evaluate(
            &source.facts,
            &serde_json::from_value(facts).unwrap(),
            &|_| panic!(),
        );
        assert_eq!(r.truth, Truth::True);
        assert!(!r.gaps.is_empty());
    }
    assert_eq!(
        evaluate(
            all,
            json!({"enabled":{"availability":"available","value":true},"balance":{"availability":"available","value":"1"}})
        ),
        Truth::True
    );
}

#[test]
fn counts_use_distinct_identity_keys_and_membership_uses_declared_types() {
    let two = predicate(
        json!({"op":"count","path":"identities","distinct_by":"signature","relation":"gt","value":1}),
    );
    let history = |signatures: Vec<&str>| json!({"identities":{"availability":"available","value":signatures.into_iter().map(|signature|json!({"signature":signature})).collect::<Vec<_>>()}});
    assert_eq!(
        evaluate(two.clone(), history(vec!["A", "A", "A"])),
        Truth::False
    );
    assert_eq!(evaluate(two, history(vec!["A", "A", "B"])), Truth::True);
    let member = predicate(json!({"op":"in","path":"balance","values":["1.0","2"]}));
    assert_eq!(
        evaluate(
            member.clone(),
            json!({"balance":{"availability":"available","value":"10e-1"}})
        ),
        Truth::True
    );
    assert_eq!(
        evaluate(
            member,
            json!({"balance":{"availability":"available","value":"10"}})
        ),
        Truth::False
    );
}

#[test]
fn invalid_paths_types_units_and_rule_budgets_are_configuration_errors() {
    let mut s = source();
    let p =
        predicate(json!({"op":"compare_fields","left":"balance","right":"other","relation":"le"}));
    check(&p, &s);
    s.facts.insert(
        "other".into(),
        serde_json::from_value(json!({"type":"decimal","unit":"SOL"})).unwrap(),
    );
    let rule = |p: Predicate| {
        serde_json::from_value(json!({"rule_id":"r1","version":1,"pass":p,"fail":p,"complete":p}))
            .unwrap()
    };
    assert!(rules::validate(&[rule(p)], &s.facts).is_err());
    for value in [
        json!({"op":"compare","path":"raw_unselected","relation":"eq","value":true}),
        json!({"op":"compare","path":"enabled","relation":"gt","value":true}),
        json!({"op":"compare","path":"balance","relation":"eq","value":1}),
        json!({"op":"compare","path":"count","relation":"eq","value":null}),
    ] {
        assert!(rules::validate(&[rule(predicate(value))], &s.facts).is_err());
    }
    let mut deep = predicate(json!({"op":"compare","path":"enabled","relation":"eq","value":true}));
    for _ in 0..10 {
        deep = Predicate::All {
            predicates: vec![deep],
        };
    }
    assert!(rules::validate(&[rule(deep)], &s.facts).is_err());
    let mut configured = source();
    configured.requirements = vec![rule(predicate(
        json!({"op":"compare","path":"absent","relation":"eq","value":true}),
    ))];
    assert!(configured.validate().is_err());
}

#[test]
fn logical_paths_follow_declared_availability_wrappers_and_nullable_types() {
    let mut s = source();
    s.facts.insert("record".into(),serde_json::from_value(json!({"type":"record","fields":{"availability":{"type":"string","max_bytes":20},"nested":{"type":"availability","value":{"type":"nullable","value":{"type":"integer","unit":"ms"}}}}})).unwrap());
    let p = predicate(
        json!({"op":"compare","path":"record.availability","relation":"eq","value":"available"}),
    );
    check(&p, &s);
    let facts=serde_json::from_value(json!({"record":{"availability":"available","value":{"availability":"available","nested":{"availability":"available","value":null}}}})).unwrap();
    assert_eq!(
        p.evaluate(&s.facts, &facts, &|_| panic!()).truth,
        Truth::True
    );
    let p = predicate(json!({"op":"compare","path":"record.nested","relation":"eq","value":null}));
    check(&p, &s);
    assert_eq!(
        p.evaluate(&s.facts, &facts, &|_| panic!()).truth,
        Truth::True
    );
    let missing=serde_json::from_value(json!({"record":{"availability":"available","value":{"availability":"available","nested":{"availability":"redacted"}}}})).unwrap();
    assert_eq!(
        p.evaluate(&s.facts, &missing, &|_| panic!()).truth,
        Truth::Unknown
    );
}
