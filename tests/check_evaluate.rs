use ronchi::{
    checks::{Definition, Scope, evaluate::Selection},
    config::Mode,
    journal::{Event, JournalStore},
    query::{Arguments, Engine, Operation, Request},
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
struct History {
    temp: tempfile::TempDir,
    store: JournalStore,
    definition: Definition,
    seq: u64,
}
impl History {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let store = JournalStore::create(&temp.path().join("journal.sqlite"), "L1").unwrap();
        let definition=serde_json::from_value(json!({"check_id":"state","version":1,"source":"independent","schema_version":1,"url":"http://127.0.0.1:1/never-call","facts":{"enabled":{"type":"boolean"},"violation":{"type":"boolean"},"closed":{"type":"boolean"},"identity":{"type":"string","max_bytes":88}},"requirements":[{"rule_id":"bounded-state","version":1,"pass":{"compare":{"path":"enabled","relation":"eq","value":true}},"fail":{"compare":{"path":"violation","relation":"eq","value":true}},"complete":{"compare":{"path":"closed","relation":"eq","value":true}},"immutable_paths":["identity"],"revision_paths":["enabled","closed","violation"]}]})).unwrap();
        Self {
            temp,
            store,
            definition,
            seq: 0,
        }
    }
    fn event(&mut self, kind: &str, source: &str, data: Value) -> u64 {
        self.seq = self
            .store
            .append(&[Event {
                schema_version: 1,
                kind: kind.into(),
                source: source.into(),
                recorded_at: "2026-11-03T00:00:00Z".into(),
                elapsed_ns: self.seq,
                execution_mode: Mode::Simulator,
                data,
            }])
            .unwrap();
        self.seq
    }
    fn observe(
        &mut self,
        revision: &str,
        enabled: bool,
        violation: impl Into<Value>,
        closed: bool,
        identity: &str,
    ) -> u64 {
        let violation = violation.into();
        let violation = if violation.is_null() {
            json!({"availability":"redacted"})
        } else {
            json!({"availability":"available","value":violation})
        };
        let scope = Scope {
            intent_ids: vec!["W1".into()],
            ..Default::default()
        };
        let invocation_id = format!("I{}", self.seq);
        let request = json!({"contract_version":1,"invocation_id":invocation_id,"check_id":"state","lab_id":"L1","snapshot_seq":self.seq,"scope":scope,"facts":["closed","enabled","identity","violation"],"deadline":"2026-11-03T00:00:05Z"});
        let start = self.event(
            "check.invocation_started",
            "ronchi",
            json!({"definition":self.definition.public_definition(),"source_request":request}),
        );
        self.event("check.observation","application",json!({"check_id":"state","version":1,"request_id":invocation_id,"scope":scope,"cause_refs":[{"lab_id":"L1","event_seq":start}],"retention":"complete_validated_selection","payload":{"contract_version":1,"invocation_id":invocation_id,"source":"independent","schema_version":1,"observed_at":"2026-11-03T00:00:00Z","revision":revision,"facts":{"enabled":{"availability":"available","value":enabled},"violation":violation,"closed":{"availability":"available","value":closed},"identity":{"availability":"available","value":identity}}}}))
    }
    async fn try_evaluate(
        &self,
        snapshot: u64,
        engine: &Engine,
        cancel: CancellationToken,
    ) -> Result<ronchi::query::Response, ronchi::query::Error> {
        engine
            .execute(
                self.temp.path().join("journal.sqlite"),
                false,
                "explicit",
                Request {
                    contract_version: 1,
                    request_id: "evaluate-1".into(),
                    lab_id: "L1".into(),
                    operation: Operation::Evaluate,
                    arguments: Arguments {
                        snapshot_seq: Some(snapshot),
                        evaluation: Some(Selection {
                            check_id: self.definition.check_id.clone(),
                            definition_version: self.definition.version,
                            definition_digest: None,
                            scope: Scope {
                                intent_ids: vec!["W1".into()],
                                ..Default::default()
                            },
                        }),
                        ..Default::default()
                    },
                },
                cancel,
            )
            .await
    }
    async fn evaluate(&self, snapshot: u64) -> ronchi::query::Response {
        self.try_evaluate(snapshot, &Engine::default(), CancellationToken::new())
            .await
            .unwrap()
    }
}
#[tokio::test]
async fn bounded_positive_proof_replays_and_never_inherits_later_violations() {
    let mut h = History::new();
    let early = h.observe("1", false, false, false, "A");
    assert_eq!(h.evaluate(early).await.result["verdict"], "inconclusive");
    let terminal = h.observe("2", true, false, true, "A");
    let first = h.evaluate(terminal).await;
    assert_eq!(first.result["verdict"], "pass");
    let failure = h.observe("3", true, true, false, "A");
    assert_eq!(h.evaluate(failure).await.result["verdict"], "fail");
    assert_eq!(h.evaluate(terminal).await.result, first.result);
}

#[tokio::test]
async fn declared_source_contradictions_do_not_turn_normal_revisions_into_conflicts() {
    let mut h = History::new();
    h.observe("same", true, false, true, "A");
    let conflict = h.observe("same", false, false, true, "A");
    let report = h.evaluate(conflict).await;
    assert_eq!(report.result["verdict"], "inconclusive");
    assert!(report.result.to_string().contains("source_contradiction"));
    let mut h = History::new();
    let mut independent = h.definition.requirements[0].clone();
    independent.rule_id = "independent-violation".into();
    independent.immutable_paths.clear();
    independent.revision_paths.clear();
    h.definition.requirements.push(independent);
    h.observe("1", false, false, false, "A");
    let conflict = h.observe("2", false, true, false, "B");
    let report = h.evaluate(conflict).await;
    assert_eq!(report.result["verdict"], "fail");
    assert_eq!(report.result["requirements"][0]["verdict"], "inconclusive");
    assert_eq!(report.result["requirements"][1]["verdict"], "fail");
    assert!(
        report.result["requirements"][0]["gaps"]
            .to_string()
            .contains("source_contradiction")
    );
}

#[tokio::test]
async fn source_errors_and_empty_requirements_do_not_manufacture_a_verdict() {
    let mut h = History::new();
    h.observe("1", true, false, false, "A");
    let end=h.event("check.invocation_error","ronchi",json!({"check_id":"state","version":1,"scope":{"intent_ids":["W1"]},"evidence_gap":"source_timeout"}));
    let result = h.evaluate(end).await;
    assert_eq!(result.result["verdict"], "inconclusive");
    let mut h = History::new();
    h.definition.requirements.clear();
    let end = h.observe("1", true, false, true, "A");
    let result = h.evaluate(end).await;
    assert!(result.result["verdict"].is_null());
    assert_eq!(result.result["requirements"], json!([]));
}

#[tokio::test]
async fn missing_failure_fact_does_not_allow_a_positive_pass() {
    let mut h = History::new();
    let end = h.observe("1", true, Value::Null, true, "A");
    let result = h.evaluate(end).await;
    assert_eq!(result.result["verdict"], "inconclusive");
    assert!(result.result.to_string().contains("unavailable:violation"));
}

#[tokio::test]
async fn finality_join_uses_the_original_signature_and_committed_snapshot_only() {
    let mut h = History::new();
    h.definition.requirements[0].pass =
        serde_json::from_value(json!({"journal_finality":{"signature_path":"identity"}})).unwrap();
    h.event(
        "transaction.accepted",
        "rpc_simulator",
        json!({"signature":"B","stage":"pending"}),
    );
    h.event(
        "transaction.transition",
        "rpc_simulator",
        json!({"signature":"B","stage":"finalized"}),
    );
    let before = h.observe("1", true, false, true, "A");
    assert_eq!(h.evaluate(before).await.result["verdict"], "inconclusive");
    h.event(
        "transaction.accepted",
        "rpc_simulator",
        json!({"signature":"A","stage":"pending"}),
    );
    let after = h.event(
        "transaction.transition",
        "rpc_simulator",
        json!({"signature":"A","stage":"finalized"}),
    );
    assert_eq!(h.evaluate(before).await.result["verdict"], "inconclusive");
    let report = h.evaluate(after).await;
    assert_eq!(report.result["verdict"], "pass");
    assert!(
        report.result["requirements"][0]["supporting"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["event_seq"] == after)
    );
}

#[tokio::test]
async fn operational_limits_and_cancellation_remain_query_errors() {
    let mut h = History::new();
    let seq = h.observe("1", true, false, true, "A");
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(
        h.try_evaluate(seq, &Engine::default(), cancel)
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "cancelled"
    );
    let limits = ronchi::query::Limits {
        deadline: std::time::Duration::from_nanos(1),
        ..Default::default()
    };
    let engine = Engine::with_limits(limits).unwrap();
    assert_eq!(
        h.try_evaluate(seq, &engine, CancellationToken::new())
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "deadline_exceeded"
    );
    for _ in 0..512 {
        h.observe("1", true, false, true, "A");
    }
    assert_eq!(
        h.try_evaluate(h.seq, &Engine::default(), CancellationToken::new())
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "analysis_incomplete"
    );
}

fn reference_observation(h: &mut History, intent: Value, observed_at: &str) -> u64 {
    let invocation = format!("I{}", h.seq);
    let scope = json!({"intent_ids":["W1"]});
    let request = json!({"contract_version":1,"invocation_id":invocation,"check_id":h.definition.check_id,"lab_id":"L1","snapshot_seq":h.seq,"scope":scope,"facts":["intent"],"deadline":"2026-11-03T00:00:05Z"});
    let start = h.event(
        "check.invocation_started",
        "ronchi",
        json!({"definition":h.definition.public_definition(),"source_request":request}),
    );
    h.event("check.observation","application",json!({"check_id":h.definition.check_id,"version":h.definition.version,"request_id":invocation,"scope":scope,"cause_refs":[{"lab_id":"L1","event_seq":start}],"retention":"complete_validated_selection","payload":{"contract_version":1,"invocation_id":invocation,"source":"reference-relayer","schema_version":1,"observed_at":observed_at,"revision":"1","facts":{"intent":{"availability":"available","value":intent}}}}))
}
fn reference_intent() -> Value {
    json!({"intent_id":"W1","state":"prepared","original_signature":{"availability":"available","value":"A"},"prepared_identities":{"availability":"available","value":[{"signature":"A"}]},"accepted_at_ms":1000,"deadline_ms":61000,"terminal_at_ms":{"availability":"unavailable"},"time_basis":"application_unix_ms","prepared_set_complete":true,"interval_closed":false})
}
#[tokio::test]
async fn reference_rules_require_full_identity_history_terminal_evidence_and_declared_time_basis() {
    for scenario in [
        "early",
        "two",
        "same",
        "succeeded",
        "unresolved_late",
        "active_late",
        "unresolved_early",
        "partial",
        "wrong_intent",
        "redacted",
        "wrong_time",
    ] {
        let mut h = History::new();
        h.definition = ronchi::checks::reference::definition("http://127.0.0.1:1/unused".into());
        let mut intent = reference_intent();
        let mut at = "1970-01-01T00:00:02Z";
        match scenario {
            "two" => {
                intent["prepared_identities"]["value"] =
                    json!([{"signature":"A"},{"signature":"B"}])
            }
            "same" => {
                intent["state"] = json!("unresolved");
                intent["prepared_identities"]["value"] =
                    json!([{"signature":"A"},{"signature":"A"}]);
                intent["interval_closed"] = json!(true);
            }
            "succeeded" => {
                intent["state"] = json!("succeeded");
                intent["interval_closed"] = json!(true);
                intent["terminal_at_ms"] = json!({"availability":"available","value":2000});
            }
            "unresolved_late" => {
                intent["state"] = json!("unresolved");
                intent["interval_closed"] = json!(true);
                intent["terminal_at_ms"] = json!({"availability":"available","value":62000});
                at = "1970-01-01T00:01:02Z";
            }
            "active_late" => {
                at = "1970-01-01T00:01:02Z";
            }
            "unresolved_early" => {
                intent["state"] = json!("unresolved");
                intent["interval_closed"] = json!(true);
                intent["terminal_at_ms"] = json!({"availability":"available","value":2000});
            }
            "partial" => {
                intent["interval_closed"] = json!(true);
                intent["prepared_set_complete"] = json!(false);
            }
            "wrong_intent" => {
                intent["intent_id"] = json!("W2");
                intent["prepared_identities"]["value"] =
                    json!([{"signature":"A"},{"signature":"B"}]);
            }
            "redacted" => {
                intent["interval_closed"] = json!(true);
                intent["prepared_identities"] = json!({"availability":"redacted"});
            }
            "wrong_time" => {
                intent["accepted_at_ms"] = json!(100000);
                intent["state"] = json!("unresolved");
                intent["interval_closed"] = json!(true);
                at = "1970-01-01T00:01:02Z";
            }
            _ => (),
        }
        let snapshot = reference_observation(&mut h, intent, at);
        let result = h.evaluate(snapshot).await.result;
        match scenario {
            "two" => assert_eq!(result["requirements"][0]["verdict"], "fail", "{result}"),
            "same" => assert_eq!(result["requirements"][0]["verdict"], "pass", "{result}"),
            "unresolved_late" | "active_late" => {
                assert_eq!(result["requirements"][2]["verdict"], "fail", "{result}")
            }
            "succeeded" => {
                assert_eq!(
                    result["requirements"][1]["verdict"], "inconclusive",
                    "{result}"
                );
                assert_eq!(result["requirements"][2]["verdict"], "pass", "{result}");
            }
            _ => assert_eq!(result["verdict"], "inconclusive", "{scenario}: {result}"),
        }
    }
}

#[tokio::test]
async fn aggregate_checks_keep_gaps_and_empty_selection_has_no_verdict() {
    use ronchi::checks::evaluate::{Result as Evaluation, Verdict, aggregate};
    let mut h = History::new();
    let open = h.observe("1", false, false, false, "A");
    let mut a: Evaluation = serde_json::from_value(h.evaluate(open).await.result).unwrap();
    let terminal = h.observe("2", false, true, true, "A");
    let mut b: Evaluation = serde_json::from_value(h.evaluate(terminal).await.result).unwrap();
    b.check_id = "other-check".into();
    a.snapshot_seq = b.snapshot_seq;
    let mut wrong_lab = b.clone();
    wrong_lab.lab_id = "L2".into();
    assert!(aggregate(vec![a.clone(), wrong_lab]).is_err());
    let result = aggregate(vec![a, b]).unwrap();
    assert_eq!(result.verdict, Some(Verdict::Fail));
    assert!(!result.checks[0].requirements[0].gaps.is_empty());
    assert!(aggregate(vec![]).unwrap().verdict.is_none());
}

#[tokio::test]
async fn cumulative_inventory_accepts_growth_and_preserves_contradicting_histories() {
    for contradictory in [false, true] {
        let mut h = History::new();
        h.definition = ronchi::checks::reference::definition("http://127.0.0.1:1/unused".into());
        let mut first = reference_intent();
        reference_observation(&mut h, first.clone(), "1970-01-01T00:00:02Z");
        first["prepared_identities"]["value"] = if contradictory {
            json!([{"signature":"B"}])
        } else {
            json!([{"signature":"A"},{"signature":"B"}])
        };
        let invocation = format!("I{}", h.seq);
        let scope = json!({"intent_ids":["W1"]});
        let request = json!({"contract_version":1,"invocation_id":invocation,"check_id":h.definition.check_id,"lab_id":"L1","snapshot_seq":h.seq,"scope":scope,"facts":["intent"],"deadline":"2026-11-03T00:00:05Z"});
        let start = h.event(
            "check.invocation_started",
            "ronchi",
            json!({"definition":h.definition.public_definition(),"source_request":request}),
        );
        let end=h.event("check.observation","application",json!({"check_id":h.definition.check_id,"version":h.definition.version,"request_id":invocation,"scope":scope,"cause_refs":[{"lab_id":"L1","event_seq":start}],"retention":"complete_validated_selection","payload":{"contract_version":1,"invocation_id":invocation,"source":"reference-relayer","schema_version":1,"observed_at":"1970-01-01T00:00:03Z","revision":"2","facts":{"intent":{"availability":"available","value":first}}}}));
        let report = h.evaluate(end).await.result;
        assert_eq!(
            report["requirements"][0]["verdict"],
            if contradictory {
                "inconclusive"
            } else {
                "fail"
            },
            "{report}"
        );
        if contradictory {
            assert!(
                report["requirements"][0]["gaps"]
                    .to_string()
                    .contains("source_contradiction")
            );
            assert_eq!(
                report["requirements"][0]["supporting"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
        }
    }
}
