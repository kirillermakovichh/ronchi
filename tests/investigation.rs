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

#[tokio::test]
async fn evidence_payload_is_explicit_bounded_and_respects_redaction() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let mut a = event("observed", "A");
    a.data["payload"] = json!("Я🙂repeat".repeat(1000));
    let mut b = event("observed", "B");
    b.data["payload_redacted"] = json!(true);
    let mut c = event("observed", "C");
    c.data.as_object_mut().unwrap().remove("payload");
    store.append(&[a, b, c]).unwrap();
    let engine = Engine::with_limits(ronchi::query::Limits {
        preview_bytes: 7,
        ..Default::default()
    })
    .unwrap();
    let mut req = request(Operation::Evidence);
    req.arguments.include_payload = true;
    let response = engine
        .execute(path, true, "explicit", req, CancellationToken::new())
        .await
        .unwrap();
    let items = response.result["items"].as_array().unwrap();
    assert_eq!(items[0]["payload"]["preview"], "Я🙂r");
    assert_eq!(items[0]["payload"]["retained_bytes"], 12000);
    assert_eq!(items[0]["payload"]["payload_truncated"], true);
    assert_eq!(items[1]["payload_status"], "redacted");
    assert_eq!(items[1]["payload"], serde_json::Value::Null);
    assert_eq!(items[2]["payload_status"], "absent");
    assert!(response.view.selection_complete);
    assert!(!response.page.truncated);
}

#[tokio::test]
async fn full_utf8_envelope_budget_is_exact_and_byte_pages_do_not_skip_records() {
    use ronchi::query::Limits;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let e = || {
        let mut value = event("observed", "A");
        value.data["note"] = json!("Я🙂\"\n\\".repeat(70));
        value
    };
    store.append(&[e()]).unwrap();
    let req = request(Operation::Timeline);
    let response = Engine::default()
        .execute(
            path.clone(),
            true,
            "explicit",
            req.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let exact = serde_json::to_vec(&response).unwrap().len();
    assert!(exact > 1000);
    let engine = Engine::with_limits(Limits {
        response_bytes: exact,
        ..Default::default()
    })
    .unwrap();
    let fit = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            req.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(serde_json::to_vec(&fit).unwrap().len(), exact);
    let engine = Engine::with_limits(Limits {
        response_bytes: exact - 1,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        engine
            .execute(
                path.clone(),
                true,
                "explicit",
                req.clone(),
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "result_too_large"
    );
    store.append(&[e(), e(), e(), e()]).unwrap();
    let cap = exact + 900;
    let engine = Engine::with_limits(Limits {
        response_bytes: cap,
        ..Default::default()
    })
    .unwrap();
    let mut req = req;
    let mut seen = vec![];
    let mut byte_stops = 0;
    loop {
        let response = engine
            .execute(
                path.clone(),
                true,
                "explicit",
                req.clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(serde_json::to_vec(&response).unwrap().len() <= cap);
        seen.extend(
            response.result["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["event_ref"]["event_seq"].as_u64().unwrap()),
        );
        if response.page.truncated {
            assert_eq!(response.page.reason.as_deref(), Some("response_bytes"));
            byte_stops += 1;
        }
        req.arguments.cursor = response.page.next_cursor;
        if req.arguments.cursor.is_none() {
            assert!(response.view.selection_complete);
            break;
        }
    }
    assert_eq!(seen, vec![1, 2, 3, 4, 5]);
    assert!(byte_stops > 0);
}

#[tokio::test]
async fn metadata_projection_does_not_access_original_payload_storage() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&[event("observed", "A")]).unwrap();
    // Fault injection at the storage boundary: any evaluation of event_json fails.
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("ALTER TABLE events RENAME TO retained_events; CREATE VIEW events AS SELECT event_seq,json('invalid json') AS event_json FROM retained_events;").unwrap();
    let engine = Engine::default();
    let response = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            request(Operation::Timeline),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(response.result["items"][0]["event_ref"]["event_seq"], 1);
    let mut req = request(Operation::Evidence);
    req.arguments.include_payload = true;
    assert_eq!(
        engine
            .execute(path, true, "explicit", req, CancellationToken::new())
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "internal_error"
    );
}

#[tokio::test]
async fn read_only_legacy_adapter_declares_correlation_gap_without_migration() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&[event("observed", "A")]).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("DROP TRIGGER query_projection; DROP TABLE query_signatures; DROP TABLE query_events; PRAGMA user_version=5;").unwrap();
    let mut req = request(Operation::Timeline);
    req.arguments.scope.signature = vec!["A".into()];
    let response = Engine::default()
        .execute(path, false, "latest", req, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(response.result["items"].as_array().unwrap().len(), 1);
    assert!(
        response
            .view
            .evidence_gaps
            .iter()
            .any(|g| g.code == "legacy_correlation")
    );
    let version: u32 = connection
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 5);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_interrupts_sqlite_work_releases_snapshot_and_leaves_writer_healthy() {
    use std::time::{Duration, Instant};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&[event("observed", "A")]).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.busy_timeout(Duration::ZERO).unwrap();
    // Make one actual SQL query expensive without allocating or returning a huge result.
    // ALTER rewrites the writer trigger to the retained table, so appends remain real.
    connection.execute_batch("ALTER TABLE query_events RENAME TO retained_navigation; CREATE VIEW query_events AS WITH RECURSIVE work(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM work WHERE x<100000000) SELECT retained_navigation.* FROM retained_navigation CROSS JOIN work;").unwrap();
    let cancel = CancellationToken::new();
    let query = tokio::spawn({
        let path = path.clone();
        let cancel = cancel.clone();
        async move {
            Engine::default()
                .execute(path, true, "current", request(Operation::Summary), cancel)
                .await
        }
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        store.append(&[event("concurrent", "B")]).unwrap();
        let (busy, frames, done): (i64, i64, i64) = connection
            .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        if busy != 0 || done < frames {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "query did not retain its active page snapshot"
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    cancel.cancel();
    let error = tokio::time::timeout(Duration::from_secs(1), query)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "cancelled");
    store.append(&[event("after_cancel", "C")]).unwrap();
    let (busy, _, _): (i64, i64, i64) = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(busy, 0);
    connection
        .execute_batch(
            "DROP VIEW query_events; ALTER TABLE retained_navigation RENAME TO query_events;",
        )
        .unwrap();
    let response = Engine::default()
        .execute(
            path,
            true,
            "current",
            request(Operation::Summary),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(response.result["counts"]["total"].as_u64().unwrap() >= 3);
}

#[tokio::test]
async fn filter_budgets_are_validated_before_opening_storage() {
    let mut req = request(Operation::Timeline);
    req.arguments.scope.kind = (0..201).map(|n| format!("kind-{n}")).collect();
    let error = Engine::default()
        .execute(
            "/missing/journal.sqlite".into(),
            false,
            "explicit",
            req.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "invalid_argument");
    req.arguments.scope.kind.pop();
    assert_eq!(
        Engine::default()
            .execute(
                "/missing/journal.sqlite".into(),
                false,
                "explicit",
                req.clone(),
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "journal_unavailable"
    );
    req.arguments.limit = 201;
    assert_eq!(
        Engine::default()
            .execute(
                "/missing/journal.sqlite".into(),
                false,
                "explicit",
                req,
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "invalid_argument"
    );
}

#[tokio::test]
async fn payload_state_and_preview_truncation_do_not_imply_item_selection_truncation() {
    use ronchi::query::Limits;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let mut e = event("observed", "A");
    e.data["payload"] = json!("🙂\n\"".repeat(2000));
    store.append(&[e]).unwrap();
    let mut req = request(Operation::Evidence);
    req.arguments.include_payload = true;
    let response = Engine::with_limits(Limits {
        response_bytes: 2000,
        ..Default::default()
    })
    .unwrap()
    .execute(
        path.clone(),
        true,
        "explicit",
        req.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 2000);
    assert_eq!(
        response.result["items"][0]["payload"]["payload_truncated"],
        true
    );
    assert!(response.view.selection_complete);
    assert!(!response.page.truncated);
    assert!(response.page.next_cursor.is_none());
    let mut omitted = event("observed", "B");
    omitted.data["payload_status"] = json!("omitted_by_policy");
    let mut lost = event("observed", "C");
    lost.data["payload_status"] = json!("lost");
    store.append(&[omitted, lost]).unwrap();
    req.arguments.scope.from_seq = Some(2);
    let response = Engine::default()
        .execute(path, true, "explicit", req, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        response.result["items"][0]["payload_status"],
        "omitted_by_policy"
    );
    assert_eq!(response.result["items"][1]["payload_status"], "lost");
    for i in response.result["items"].as_array().unwrap() {
        assert!(i["payload"].is_null());
    }
}

#[tokio::test]
async fn case_method_reference_filters_and_no_match_keep_exact_event_identity() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let mut e = event("rpc.registered", "A");
    e.data = json!({"request_id":"r1:0","registration_seq":1,"method":"getTransaction","signatures":["A"],"binding":{"policy":{"revision":7}}});
    let mut decision = event("rpc.decision", "A");
    decision.data = json!({"request_id":"r1:0","result":null});
    store.append(&[e, decision, event("other", "A")]).unwrap();
    let mut req = request(Operation::Timeline);
    req.arguments.scope = Scope {
        case_activation_id: vec![7, 8],
        rpc_method: vec!["getTransaction".into(), "getSlot".into()],
        event_refs: vec![ronchi::query::EventRef {
            lab_id: "L1".into(),
            event_seq: 2,
        }],
        signature: vec!["A".into()],
        ..Default::default()
    };
    let response = Engine::default()
        .execute(
            path.clone(),
            true,
            "explicit",
            req.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(response.result["items"].as_array().unwrap().len(), 1);
    let item = &response.result["items"][0];
    assert_eq!(item["policy_ref"]["revision"], 7);
    assert_eq!(item["transaction_signatures"], json!(["A"]));
    assert_eq!(item["event_ref"]["event_seq"], 2);
    req.operation = Operation::Summary;
    req.arguments.scope.kind = vec!["absent".into()];
    let summary = Engine::default()
        .execute(path, true, "explicit", req, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(summary.result["traffic_state"], "no_match");
    assert_eq!(summary.result["counts"]["total"], 0);
}

#[tokio::test]
async fn all_snapshot_pages_are_stable_during_concurrent_writer_appends() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let original: Vec<_> = (0..100).map(|_| event("observed", "A")).collect();
    store.append(&original).unwrap();
    let engine = Engine::default();
    let mut req = request(Operation::Timeline);
    req.arguments.limit = 17;
    let first = engine
        .execute(
            path.clone(),
            true,
            "current",
            req.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let mut seen: Vec<_> = first.result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["event_ref"]["event_seq"].as_u64().unwrap())
        .collect();
    req.arguments.cursor = first.page.next_cursor;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let ready = barrier.clone();
    let writer = tokio::task::spawn_blocking(move || {
        ready.wait();
        for _ in 0..20 {
            store.append(&[event("observed", "B")]).unwrap();
        }
    });
    barrier.wait();
    while req.arguments.cursor.is_some() {
        let page = engine
            .execute(
                path.clone(),
                true,
                "current",
                req.clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(page.view.snapshot_seq, 100);
        seen.extend(
            page.result["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["event_ref"]["event_seq"].as_u64().unwrap()),
        );
        req.arguments.cursor = page.page.next_cursor;
    }
    writer.await.unwrap();
    assert_eq!(seen, (1..=100).collect::<Vec<_>>());
    let fresh = engine
        .execute(
            path,
            true,
            "current",
            request(Operation::Summary),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(fresh.view.snapshot_seq, 120);
}

#[tokio::test]
async fn cursors_bind_operation_lab_projection_payload_and_normalized_filters() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store
        .append(&[event("observed", "A"), event("observed", "B")])
        .unwrap();
    let engine = Engine::default();
    let mut req = request(Operation::Evidence);
    req.arguments.limit = 1;
    req.arguments.scope.signature = vec!["B".into(), "A".into()];
    req.arguments.fields = vec!["kind".into(), "source".into()];
    req.arguments.cursor = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            req.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .page
        .next_cursor;
    let mut normalized = req.clone();
    normalized.request_id = "new-request".into();
    normalized.arguments.scope.signature = vec!["A".into(), "B".into(), "A".into()];
    normalized.arguments.fields.reverse();
    assert_eq!(
        engine
            .execute(
                path.clone(),
                true,
                "explicit",
                normalized,
                CancellationToken::new()
            )
            .await
            .unwrap()
            .result["items"][0]["event_ref"]["event_seq"],
        2
    );
    let mut changed = vec![];
    let mut r = req.clone();
    r.operation = Operation::Timeline;
    changed.push(r);
    let mut r = req.clone();
    r.arguments.include_payload = true;
    changed.push(r);
    let mut r = req.clone();
    r.arguments.fields.push("data".into());
    changed.push(r);
    let mut r = req.clone();
    r.arguments.scope.signature = vec!["A".into()];
    changed.push(r);
    let mut r = req.clone();
    r.arguments.scope.to_seq = Some(1);
    changed.push(r);
    for r in changed {
        assert_eq!(
            engine
                .execute(path.clone(), true, "explicit", r, CancellationToken::new())
                .await
                .unwrap_err()
                .code
                .as_ref(),
            "invalid_cursor"
        );
    }
    let other = temp.path().join("other.sqlite");
    let _store = JournalStore::create(&other, "L2").unwrap();
    req.lab_id = "L2".into();
    assert_eq!(
        engine
            .execute(other, true, "explicit", req, CancellationToken::new())
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "invalid_cursor"
    );
}

#[tokio::test]
async fn json_payload_preview_preserves_literals_without_numeric_coercion() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let literals = [
        "true",
        "null",
        "1234567890123456789012345678901234567890",
        "[true,null,123456789012345678901234567890]",
    ];
    let events: Vec<_> = literals
        .iter()
        .map(|literal| {
            let mut e = event("observed", "A");
            e.data["payload"] = serde_json::from_str(literal).unwrap();
            e
        })
        .collect();
    store.append(&events).unwrap();
    let mut req = request(Operation::Evidence);
    req.arguments.include_payload = true;
    let result = Engine::default()
        .execute(path, true, "explicit", req, CancellationToken::new())
        .await
        .unwrap();
    for (item, literal) in result.result["items"]
        .as_array()
        .unwrap()
        .iter()
        .zip(literals)
    {
        assert_eq!(item["payload"]["preview"], literal);
        assert_eq!(item["payload"]["retained_bytes"], literal.len());
        assert_eq!(item["payload"]["encoding"], "json");
    }
}

#[tokio::test]
async fn payload_preview_pages_keep_continuation_when_metadata_uses_the_byte_budget() {
    use ronchi::query::Limits;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    for signature in ["A", "B", "C"] {
        let mut e = event("observed", signature);
        e.data["payload"] = json!("🙂\"\n".repeat(2000));
        store.append(&[e]).unwrap();
    }
    for cap in (1800..2200).step_by(37) {
        let engine = Engine::with_limits(Limits {
            response_bytes: cap,
            ..Default::default()
        })
        .unwrap();
        let mut req = request(Operation::Evidence);
        req.arguments.include_payload = true;
        req.arguments.fields = vec!["kind".into()];
        let mut seen = vec![];
        loop {
            let page = engine
                .execute(
                    path.clone(),
                    true,
                    "explicit",
                    req.clone(),
                    CancellationToken::new(),
                )
                .await
                .unwrap();
            assert!(serde_json::to_vec(&page).unwrap().len() <= cap);
            seen.extend(
                page.result["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| i["event_ref"]["event_seq"].as_u64().unwrap()),
            );
            req.arguments.cursor = page.page.next_cursor;
            if req.arguments.cursor.is_none() {
                assert!(page.view.selection_complete);
                break;
            }
            assert!(page.page.truncated);
            assert_eq!(page.page.reason.as_deref(), Some("response_bytes"));
        }
        assert_eq!(seen, vec![1, 2, 3]);
    }
}

#[tokio::test]
async fn summary_preserves_lab_mode_and_case_context_at_the_selected_watermark() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let mut changed = event("case.changed", "");
    changed.data = json!({"binding":{"registration_seq":7,"policy":{"revision":7,"case_id":"tx-response-lost"}}});
    store.append(&[changed]).unwrap();
    let response = Engine::default()
        .execute(
            path,
            true,
            "explicit",
            request(Operation::Summary),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.result["lab_context"]["execution_mode"],
        "simulator"
    );
    assert_eq!(
        response.result["lab_context"]["case_binding"]["policy"]["revision"],
        7
    );
}
