use ronchi::{
    config::Mode,
    journal::{Event, JournalStore},
    query::{Arguments, Engine, Operation, Request, Scope},
};
use serde_json::json;
use tokio_util::sync::CancellationToken;
fn event(kind: &str, signature: &str) -> Event {
    Event {
        schema_version: 1,
        kind: kind.into(),
        source: "rpc_simulator".into(),
        recorded_at: "2026-11-03T00:00:00Z".into(),
        elapsed_ns: 1,
        execution_mode: Mode::Simulator,
        data: json!({"signature":signature,"method":"sendTransaction","stage":"processed","payload":"DO NOT RETURN IN TIMELINE"}),
    }
}
fn request(op: Operation) -> Request {
    Request {
        contract_version: 1,
        request_id: "q1".into(),
        lab_id: "L1".into(),
        operation: op,
        arguments: Arguments::default(),
    }
}
#[tokio::test]
async fn scoped_timeline_selects_in_storage_and_preserves_reference_without_payload() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store
        .append(&[
            event("observed", "A"),
            event("observed", "B"),
            event("other", "A"),
        ])
        .unwrap();
    let mut req = request(Operation::Timeline);
    req.arguments.scope = Scope {
        signature: vec!["A".into()],
        kind: vec!["observed".into()],
        ..Default::default()
    };
    let response = Engine::default()
        .execute(path, true, "explicit", req, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(response.result["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        response.result["items"][0]["event_ref"],
        json!({"lab_id":"L1","event_seq":1})
    );
    assert_eq!(response.view.snapshot_seq, 3);
    assert!(response.view.selection_complete);
    assert!(
        !serde_json::to_string(&response)
            .unwrap()
            .contains("DO NOT RETURN")
    );
}

#[tokio::test]
async fn pages_keep_committed_watermark_and_reject_changed_or_damaged_cursor() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store
        .append(&[
            event("observed", "A"),
            event("observed", "B"),
            event("observed", "C"),
        ])
        .unwrap();
    let engine = Engine::default();
    let mut req = request(Operation::Timeline);
    req.arguments.limit = 2;
    let first = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            req.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(!first.view.selection_complete);
    store.append(&[event("observed", "D")]).unwrap();
    req.arguments.cursor = first.page.next_cursor;
    let second = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            req.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(second.view.snapshot_seq, 3);
    assert_eq!(second.result["items"].as_array().unwrap().len(), 1);
    assert_eq!(second.result["items"][0]["event_ref"]["event_seq"], 3);
    assert!(second.view.selection_complete);
    let mut changed = req.clone();
    changed.arguments.limit = 1;
    assert_eq!(
        engine
            .execute(
                path.clone(),
                true,
                "explicit",
                changed,
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "invalid_cursor"
    );
    req.arguments.cursor.as_mut().unwrap().push('x');
    assert_eq!(
        engine
            .execute(path, true, "explicit", req, CancellationToken::new())
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "invalid_cursor"
    );
}

#[tokio::test]
async fn filters_use_or_within_and_between_and_exact_time_and_sequence_bounds() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let mut events = vec![
        event("observed", "A"),
        event("observed", "B"),
        event("other", "A"),
        event("observed", "A"),
    ];
    for (e, t) in events.iter_mut().zip([
        "2026-11-03T00:00:00Z",
        "2026-11-03T00:00:00.000000001Z",
        "2026-11-03T00:00:00.000000002Z",
        "2026-11-03T00:00:00.000000003Z",
    ]) {
        e.recorded_at = t.into();
    }
    store.append(&events).unwrap();
    let mut req = request(Operation::Timeline);
    req.arguments.scope = Scope {
        signature: vec!["B".into(), "A".into()],
        kind: vec!["observed".into()],
        from_seq: Some(2),
        to_seq: Some(4),
        since: Some("2026-11-03T00:00:00.000000001Z".into()),
        until: Some("2026-11-03T00:00:00.000000003Z".into()),
        ..Default::default()
    };
    let result = Engine::default()
        .execute(path, true, "explicit", req, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.result["items"].as_array().unwrap().len(), 1);
    assert_eq!(result.result["items"][0]["event_ref"]["event_seq"], 2);
}

#[tokio::test]
async fn empty_summary_discloses_unperformed_analysis_and_projection_keeps_identity() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let engine = Engine::default();
    let response = engine
        .execute(
            path.clone(),
            false,
            "latest",
            request(Operation::Summary),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(response.result["analysis"]["findings"], "not_performed");
    assert_eq!(response.result["findings"], serde_json::Value::Null);
    assert_eq!(response.result["traffic_state"], "no_rpc_traffic");
    assert_eq!(
        serde_json::to_value(response.view.journal_state).unwrap(),
        "interrupted"
    );
    store.append(&[event("observed", "A")]).unwrap();
    let mut req = request(Operation::Evidence);
    req.arguments.fields = vec!["kind".into()];
    let response = engine
        .execute(
            path.clone(),
            true,
            "current",
            req.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let item = &response.result["items"][0];
    assert_eq!(item["transaction_signature"], "A");
    assert!(item.get("data").is_none());
    assert_eq!(item["source"], "rpc_simulator");
    req.arguments.fields = vec!["unknown".into()];
    assert_eq!(
        engine
            .execute(
                path.clone(),
                true,
                "current",
                req.clone(),
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "invalid_field"
    );
    req.arguments.scope.finding_id = Some("F1".into());
    assert_eq!(
        engine
            .execute(path, true, "current", req, CancellationToken::new())
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "unsupported_capability"
    );
}
