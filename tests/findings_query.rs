use ronchi::{
    config::Mode,
    journal::{Event, JournalStore},
    query::{Arguments, Engine, EventRef, Operation, Request},
};
use serde_json::json;
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
    assert!(
        !projected.result["items"][0]["limitations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
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

#[tokio::test]
async fn legacy_correlation_gaps_are_read_only_and_keep_other_confirmed_findings() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let mut events = history();
    events[3].data.as_object_mut().unwrap().remove("request");
    events[3]
        .data
        .as_object_mut()
        .unwrap()
        .remove("admission_request");
    store.append(&events).unwrap();
    let mut independent = history();
    for (i, e) in independent.iter_mut().enumerate() {
        e.data["signature"] = json!("B");
        e.data["request"]["request_id"] = json!(format!("b-{i}"));
        e.data["admission_request"] = e.data["request"].clone();
    }
    // One incident has one request identity; later requests have their own identities.
    for e in independent.iter_mut().take(4) {
        e.data["request"]["request_id"] = json!("b-original");
        e.data["admission_request"] = e.data["request"].clone();
    }
    independent[4] = event("rpc.registered", "b-lookup", 2, "B");
    independent[5] = event("transaction.submission", "b-retry", 3, "B");
    store.append(&independent).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("DROP TRIGGER query_projection; DROP TABLE query_signatures; DROP TABLE query_events; PRAGMA user_version=5;").unwrap();
    let events_before: String = connection
        .query_row("SELECT json_group_array(event_json) FROM events", [], |r| {
            r.get(0)
        })
        .unwrap();
    let response = query(&path, req(Operation::Findings)).await;
    assert_eq!(
        response.result["items"].as_array().unwrap().len(),
        2,
        "{response:?}"
    );
    assert!(
        response.result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["scope"]["signature"] == "B")
    );
    assert!(
        response
            .view
            .evidence_gaps
            .iter()
            .any(|g| g.code == "legacy_correlation")
    );
    assert!(
        response.result["analysis"]["rules"][0]["insufficient_evidence"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    // The exact immutable event set remains unchanged, with no migration or derived writes.
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        12
    );
    let events_after: String = connection
        .query_row("SELECT json_group_array(event_json) FROM events", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(events_before, events_after);
}

#[tokio::test]
async fn shared_batch_boundary_does_not_select_a_fault_for_another_element() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    let events = vec![
        event("transaction.submission", "batch:0", 1, "A"),
        event("transaction.accepted", "batch:0", 1, "A"),
        event("fault.response_loss_selected", "batch:0", 1, "A"),
        event("fault.withholding_started", "batch:0", 1, "A"),
        event("transaction.submission", "batch:1", 1, "B"),
        event("transaction.accepted", "batch:1", 1, "B"),
    ];
    store.append(&events).unwrap();
    let mut boundary = event("rpc.boundary", "batch:1", 1, "B");
    boundary.data["shared_withholding"] = json!(true);
    boundary.data["transport"] = json!("withholding_deadline");
    boundary.data["rpc_response"] = json!("no_transport_response");
    store
        .append(&[
            boundary,
            event("rpc.registered", "lookup-a", 2, "A"),
            event("rpc.registered", "lookup-b", 3, "B"),
            event("transaction.submission", "retry-b", 4, "B"),
        ])
        .unwrap();
    let response = query(&path, req(Operation::Findings)).await;
    assert_eq!(response.result["items"].as_array().unwrap().len(), 1);
    assert_eq!(response.result["items"][0]["scope"]["signature"], "A");
    assert_eq!(
        response.result["analysis"]["rules"][0]["insufficient_evidence"],
        1
    );
    assert_eq!(
        response.result["items"][0]["supporting_events"][2]["event_ref"]["event_seq"],
        3
    );
}

#[tokio::test]
async fn metadata_analysis_never_reads_signed_payload_storage() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&history()).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("ALTER TABLE events RENAME TO retained_events; CREATE VIEW events AS SELECT event_seq,json('invalid json') AS event_json FROM retained_events;").unwrap();
    let response = query(&path, req(Operation::Findings)).await;
    assert_eq!(response.result["items"].as_array().unwrap().len(), 2);
    let summary = query(&path, req(Operation::Summary)).await;
    assert_eq!(summary.result["findings"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn summary_is_five_and_byte_limited_pages_keep_every_finding_once() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    for n in 0..4 {
        let signature = format!("S{n}");
        let original = format!("original-{n}");
        let reg = n * 3 + 1;
        store
            .append(&[
                event("transaction.submission", &original, reg, &signature),
                event("transaction.accepted", &original, reg, &signature),
                event("fault.response_loss_selected", &original, reg, &signature),
                event("fault.withholding_started", &original, reg, &signature),
                event(
                    "rpc.registered",
                    &format!("lookup-{n}"),
                    reg + 1,
                    &signature,
                ),
                event(
                    "transaction.submission",
                    &format!("retry-{n}"),
                    reg + 2,
                    &signature,
                ),
            ])
            .unwrap();
    }
    let all = query(&path, req(Operation::Findings)).await;
    let expected: Vec<_> = all.result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["finding_id"].clone())
        .collect();
    assert_eq!(expected.len(), 8);
    let summary = query(&path, req(Operation::Summary)).await;
    assert_eq!(summary.result["findings"].as_array().unwrap().len(), 5);
    assert_eq!(summary.result["findings_more"], true);
    let mut single = req(Operation::Findings);
    single.arguments.limit = 1;
    let first = query(&path, single).await;
    let cap = serde_json::to_vec(&first).unwrap().len() + 100;
    let engine = Engine::with_limits(ronchi::query::Limits {
        response_bytes: cap,
        ..Default::default()
    })
    .unwrap();
    let mut request = req(Operation::Findings);
    let mut actual = Vec::new();
    let mut stops = 0;
    loop {
        let response = engine
            .execute(
                path.clone(),
                false,
                "explicit",
                request.clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(serde_json::to_vec(&response).unwrap().len() <= cap);
        actual.extend(
            response.result["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["finding_id"].clone()),
        );
        if response.page.truncated {
            stops += 1;
            assert_eq!(response.page.reason.as_deref(), Some("response_bytes"));
        }
        request.arguments.cursor = response.page.next_cursor;
        if request.arguments.cursor.is_none() {
            break;
        }
    }
    assert_eq!(actual, expected);
    assert!(stops > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn active_analysis_deadline_and_cancellation_release_snapshots_without_blocking_writes() {
    for timeout in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.sqlite");
        let mut store = JournalStore::create(&path, "L1").unwrap();
        store.append(&history()).unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.busy_timeout(std::time::Duration::ZERO).unwrap();
        connection.execute_batch("ALTER TABLE query_events RENAME TO retained_navigation; CREATE VIEW query_events AS WITH RECURSIVE work(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM work WHERE x<100000000) SELECT retained_navigation.* FROM retained_navigation CROSS JOIN work;").unwrap();
        let engine = Engine::with_limits(ronchi::query::Limits {
            deadline: std::time::Duration::from_millis(if timeout { 250 } else { 10000 }),
            ..Default::default()
        })
        .unwrap();
        let cancel = CancellationToken::new();
        let child = cancel.clone();
        let query_path = path.clone();
        let task = tokio::spawn(async move {
            engine
                .execute(query_path, true, "current", req(Operation::Findings), child)
                .await
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            // This write finishes while the analyzer still owns its page snapshot.
            store
                .append(&[event("concurrent", "writer", 100, "Z")])
                .unwrap();
            let (busy, frames, done): (i64, i64, i64) = connection
                .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .unwrap();
            if busy != 0 || done < frames {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        if !timeout {
            cancel.cancel();
        }
        let error = tokio::time::timeout(std::time::Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(
            error.code.as_ref(),
            if timeout {
                "deadline_exceeded"
            } else {
                "cancelled"
            }
        );
        store
            .append(&[event("after_analysis", "writer", 101, "Z")])
            .unwrap();
        let (busy, _, _): (i64, i64, i64) = connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(busy, 0);
    }
}

#[tokio::test]
async fn later_fact_removal_and_same_signatures_in_other_labs_preserve_identity_limits() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("l1.sqlite");
    let other = temp.path().join("l2.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&history()).unwrap();
    let mut second = JournalStore::create(&other, "L2").unwrap();
    second.append(&history()).unwrap();
    let all = query(&path, req(Operation::Findings)).await;
    for (omitted, remaining) in [
        (5, "submission-after-withholding"),
        (6, "lookup-after-withholding"),
    ] {
        let mut r = req(Operation::Findings);
        r.arguments.scope.event_refs = (1..=6)
            .filter(|n| *n != omitted)
            .map(|event_seq| EventRef {
                lab_id: "L1".into(),
                event_seq,
            })
            .collect();
        let response = query(&path, r).await;
        assert_eq!(response.result["items"].as_array().unwrap().len(), 1);
        assert!(
            response.result["items"][0]["rule_id"]
                .as_str()
                .unwrap()
                .ends_with(remaining)
        );
    }
    let mut r = req(Operation::Findings);
    r.lab_id = "L2".into();
    let findings = Engine::default()
        .execute(
            other.clone(),
            false,
            "explicit",
            r,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_ne!(
        findings.result["items"][0]["finding_id"],
        all.result["items"][0]["finding_id"]
    );
    let mut evidence = req(Operation::Evidence);
    evidence.lab_id = "L2".into();
    evidence.arguments =
        serde_json::from_value(all.result["items"][0]["evidence_arguments"].clone()).unwrap();
    assert_eq!(
        Engine::default()
            .execute(other, false, "explicit", evidence, CancellationToken::new())
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "finding_unavailable"
    );
}

#[test]
fn linked_sqlite_plans_use_request_and_signature_indexes_for_finding_joins() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&history()).unwrap();
    let connection = rusqlite::Connection::open(path).unwrap();
    let version: (String, String) = connection
        .query_row("SELECT sqlite_version(),sqlite_source_id()", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    let cte = "WITH q AS (SELECT * FROM query_events), selected AS NOT MATERIALIZED (SELECT * FROM q WHERE event_seq>0 AND event_seq<=6)";
    let queries = [
        format!(
            "{cte} SELECT event_seq,kind,source,metadata_json FROM selected WHERE request_id='original' AND kind='transaction.accepted' ORDER BY event_seq LIMIT 1"
        ),
        format!(
            "{cte} SELECT event_seq,kind,source,metadata_json FROM selected WHERE kind='rpc.registered' AND rpc_method IN ('getSignatureStatuses','getTransaction') AND event_seq>4 AND request_id<>'original' AND json_extract(metadata_json,'$.registration_seq')>1 AND event_seq IN (SELECT event_seq FROM query_signatures WHERE signature='A') ORDER BY event_seq LIMIT 1"
        ),
    ];
    let mut plans = Vec::new();
    for sql in queries {
        let mut stmt = connection
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap();
        let rows: Vec<String> = stmt
            .query_map([], |r| r.get(3))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        plans.push(rows.join("\n"));
    }
    eprintln!(
        "SQLite {} ({})\n{}\n{}",
        version.0, version.1, plans[0], plans[1]
    );
    assert!(plans[0].contains("query_request"), "{}", plans[0]);
    assert!(
        plans[1].contains("COVERING INDEX sqlite_autoindex_query_signatures"),
        "{}",
        plans[1]
    );
}

#[tokio::test]
async fn transaction_lookup_alone_matches_and_unsupported_mode_is_explicit() {
    for mode in [Mode::Simulator, Mode::Surfpool] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.sqlite");
        let mut store = JournalStore::create(&path, "L1").unwrap();
        let mut events = history();
        events.pop();
        events[4].data["method"] = json!("getTransaction");
        for event in &mut events {
            event.execution_mode = mode;
        }
        store.append(&events).unwrap();
        let response = query(&path, req(Operation::Findings)).await;
        assert_eq!(
            response.result["items"].as_array().unwrap().len(),
            if mode == Mode::Simulator { 1 } else { 0 }
        );
        assert_eq!(
            response.result["analysis"]["rules"][0]["state"],
            if mode == Mode::Simulator {
                "matched"
            } else {
                "unsupported"
            }
        );
    }
}
