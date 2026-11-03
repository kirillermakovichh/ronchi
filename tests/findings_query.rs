use ronchi::{
    config::Mode,
    journal::{Event, JournalStore},
    query::{Arguments, Engine, EventRef, Operation, Request},
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
fn event(kind: &str, request: &str, registration: u64, signature: &str) -> Event {
    let reference = json!({"request_id":request,"registration_seq":registration});
    let data = if kind == "rpc.registered" {
        json!({"request_id":request,"registration_seq":registration,"method":"getSignatureStatuses","signatures":["OTHER",signature],"id":1})
    } else {
        json!({"request":reference,"admission_request":reference,"signature":signature,"digest":"saved-content-digest","policy":{"revision":1}})
    };
    Event {
        schema_version: 1,
        kind: kind.into(),
        source: if kind == "transaction.accepted" {
            "rpc_simulator"
        } else {
            "ronchi"
        }
        .into(),
        execution_mode: Mode::Simulator,
        recorded_at: "2026-11-03T00:00:00Z".into(),
        elapsed_ns: 1,
        data,
    }
}
fn req(operation: Operation) -> Request {
    Request {
        contract_version: 1,
        request_id: "q1".into(),
        lab_id: "L1".into(),
        operation,
        arguments: Arguments::default(),
    }
}
fn history() -> Vec<Event> {
    vec![
        event("transaction.submission", "original", 1, "A"),
        event("transaction.accepted", "original", 1, "A"),
        event("fault.response_loss_selected", "original", 1, "A"),
        event("fault.withholding_started", "original", 1, "A"),
        event("rpc.registered", "lookup", 2, "A"),
        event("transaction.submission", "retry", 3, "A"),
    ]
}
async fn query(path: &std::path::Path, r: Request) -> ronchi::query::Response {
    Engine::default()
        .execute(path.into(), false, "explicit", r, CancellationToken::new())
        .await
        .unwrap()
}
#[tokio::test]
async fn journal_patterns_ignore_presentation_and_resolve_the_same_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&history()).unwrap();
    let mut r = req(Operation::Findings);
    r.arguments.limit = 1;
    let first = query(&path, r.clone()).await;
    assert_eq!(first.result["items"].as_array().unwrap().len(), 1);
    let lookup = first.result["items"][0].clone();
    assert_eq!(lookup["scope"]["signature"], "A");
    assert!(
        lookup["statement"]
            .as_str()
            .unwrap()
            .contains("lookup for A")
    );
    assert!(first.page.next_cursor.is_some());
    store
        .append(&[event("rpc.registered", "poll", 4, "A")])
        .unwrap();
    r.arguments.cursor = first.page.next_cursor;
    let second = query(&path, r).await;
    assert_eq!(second.view.snapshot_seq, 6);
    assert!(second.page.next_cursor.is_none());
    assert!(
        second.result["items"][0]["statement"]
            .as_str()
            .unwrap()
            .contains("retained content digests match")
    );
    let all = query(&path, req(Operation::Findings)).await;
    assert_eq!(all.result["items"].as_array().unwrap().len(), 2);
    assert_eq!(all.result["items"][0]["finding_id"], lookup["finding_id"]);
    let mut projection = req(Operation::Findings);
    projection.arguments.fields = vec!["guidance".into()];
    let projected = query(&path, projection).await;
    assert_eq!(
        projected.result["items"][0]["finding_id"],
        lookup["finding_id"]
    );
    assert!(projected.result["items"][0].get("statement").is_none());
    let mut evidence = req(Operation::Evidence);
    evidence.arguments = serde_json::from_value(lookup["evidence_arguments"].clone()).unwrap();
    let support = query(&path, evidence).await;
    assert_eq!(support.view.snapshot_seq, 6);
    assert_eq!(
        support.result["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["event_ref"]["event_seq"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    let summary = query(&path, req(Operation::Summary)).await;
    assert_eq!(summary.result["findings"].as_array().unwrap().len(), 2);
    assert_eq!(summary.result["analysis"]["findings"], "performed");
    let _: Value = lookup;
}

#[tokio::test]
async fn required_facts_cannot_be_replaced_by_nearby_events_or_scope_expansion() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&history()).unwrap();
    for omitted in 1..=4 {
        let mut r = req(Operation::Findings);
        r.arguments.scope.event_refs = (1..=6)
            .filter(|n| *n != omitted)
            .map(|event_seq| EventRef {
                lab_id: "L1".into(),
                event_seq,
            })
            .collect();
        let response = query(&path, r).await;
        assert!(
            response.result["items"].as_array().unwrap().is_empty(),
            "omitted {omitted}"
        );
        assert!(!response.view.evidence_gaps.is_empty());
        assert_eq!(
            response.result["analysis"]["rules"][0]["insufficient_evidence"],
            1
        );
    }
    let mut r = req(Operation::Findings);
    r.arguments.scope.from_seq = Some(5);
    assert!(
        query(&path, r).await.result["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut r = req(Operation::Findings);
    r.arguments.scope.rpc_method = vec!["getSignatureStatuses".into()];
    assert!(
        query(&path, r).await.result["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut r = req(Operation::Findings);
    r.lab_id = "L2".into();
    assert_eq!(
        Engine::default()
            .execute(path, false, "explicit", r, CancellationToken::new())
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "invalid_argument"
    );
}

#[tokio::test]
async fn selection_alone_other_identity_and_pre_withholding_requests_do_not_match() {
    for variant in 0..4 {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.sqlite");
        let mut store = JournalStore::create(&path, "L1").unwrap();
        let mut events = history();
        match variant {
            0 => {
                events.remove(3);
            }
            1 => {
                events[4] = event("rpc.registered", "lookup", 2, "B");
                events[5] = event("transaction.submission", "retry", 3, "B");
            }
            2 => {
                events.swap(3, 4);
                events.pop();
            }
            _ => {
                events[4] = event("rpc.registered", "lookup", 1, "A");
                events[5] = event("transaction.submission", "retry", 1, "A");
            }
        }
        store.append(&events).unwrap();
        let response = query(&path, req(Operation::Findings)).await;
        assert!(
            response.result["items"].as_array().unwrap().is_empty(),
            "variant {variant}"
        );
    }
}

#[tokio::test]
async fn polling_deduplicates_and_one_incomplete_incident_keeps_independent_findings() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&history()).unwrap();
    for n in 4..40 {
        store
            .append(&[event("rpc.registered", &format!("poll-{n}"), n, "A")])
            .unwrap();
    }
    store
        .append(&[
            event("transaction.submission", "other", 40, "B"),
            event("transaction.accepted", "other", 40, "B"),
            event("fault.response_loss_selected", "other", 40, "B"),
        ])
        .unwrap();
    let response = query(&path, req(Operation::Findings)).await;
    assert_eq!(response.result["items"].as_array().unwrap().len(), 2);
    assert_eq!(response.result["analysis"]["rules"][0]["matched"], 1);
    assert_eq!(
        response.result["analysis"]["rules"][0]["insufficient_evidence"],
        1
    );
    assert_eq!(
        response.result["items"][0]["supporting_events"][4]["event_ref"]["event_seq"],
        5
    );
    assert!(
        response
            .view
            .evidence_gaps
            .iter()
            .any(|g| g.code == "finding_evidence_gap")
    );
}

#[tokio::test]
async fn operational_limit_is_incomplete_and_never_no_finding_or_a_new_watermark() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&history()).unwrap();
    for n in 0..256 {
        store
            .append(&[event(
                "transaction.accepted",
                &format!("request-{n}"),
                n + 10,
                &format!("S{n}"),
            )])
            .unwrap();
    }
    let response = query(&path, req(Operation::Findings)).await;
    assert_eq!(response.result["items"].as_array().unwrap().len(), 2);
    assert_eq!(response.result["analysis"]["findings"], "operational_limit");
    assert_eq!(
        response.result["analysis"]["rules"][0]["state"],
        "operational_limit"
    );
    assert!(!response.view.selection_complete);
    let engine = Engine::with_limits(ronchi::query::Limits {
        deadline: std::time::Duration::ZERO,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        engine
            .execute(
                path.clone(),
                false,
                "explicit",
                req(Operation::Findings),
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "deadline_exceeded"
    );
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert_eq!(
        Engine::default()
            .execute(path, false, "explicit", req(Operation::Findings), cancelled)
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "cancelled"
    );
}
