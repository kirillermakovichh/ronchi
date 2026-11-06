use ronchi::{
    config::Mode,
    journal::{Event, JournalStore},
    query::{Arguments, Engine, Limits, Operation, Request},
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
fn event(kind: &str, id: &str, data: Value) -> Event {
    let mut data = data.as_object().unwrap().clone();
    data.insert("request_id".into(), json!(id));
    Event {
        schema_version: 1,
        kind: kind.into(),
        source: "ronchi".into(),
        recorded_at: "2026-11-04T00:00:00Z".into(),
        elapsed_ns: 7,
        execution_mode: Mode::Simulator,
        data: Value::Object(data),
    }
}
fn request(operation: Operation) -> Request {
    Request {
        contract_version: 1,
        request_id: "query".into(),
        lab_id: "L1".into(),
        operation,
        arguments: Arguments::default(),
    }
}
#[tokio::test]
async fn exchange_pages_pin_related_facts_and_keep_batch_request_identities_separate() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store
        .append(&[
            event(
                "rpc.registered",
                "r1:0",
                json!({"method":"getHealth","registration_seq":1,"batch_id":1}),
            ),
            event(
                "rpc.registered",
                "r1:1",
                json!({"method":"sendTransaction","registration_seq":1,"batch_id":1}),
            ),
            event(
                "rpc.response_prepared",
                "r1:0",
                json!({"result":"ok","payload":"PRIVATE BODY"}),
            ),
        ])
        .unwrap();
    let engine = Engine::default();
    let mut first = request(Operation::Requests);
    first.arguments.limit = 1;
    first.arguments.snapshot_seq = Some(3);
    let response = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            first.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let item = &response.result["items"][0];
    assert_eq!(item["request_id"], "r1:0");
    assert_eq!(item["exchange"]["prepared"]["event_ref"]["event_seq"], 3);
    assert!(item["exchange"]["boundary"].is_null());
    assert!(
        !serde_json::to_string(&response)
            .unwrap()
            .contains("PRIVATE BODY")
    );
    store.append(&[
        event("rpc.boundary", "r1:0", json!({"rpc_response":"result","transport":"http_write_completed","client_receipt":"unknown"})),
        event("transaction.accepted", "r1:1", json!({"signature":"B"})),
    ]).unwrap();
    first.arguments.snapshot_seq = Some(response.view.snapshot_seq);
    first.arguments.cursor = response.page.next_cursor;
    let next = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            first.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(next.result["items"][0]["request_id"], "r1:1");
    assert!(next.result["items"][0]["exchange"]["acceptance"].is_null());
    assert!(next.page.next_cursor.is_none());
    first.arguments.scope.request_id = vec!["r1:0".into()];
    assert_eq!(
        engine
            .execute(
                path.clone(),
                true,
                "explicit",
                first,
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code
            .as_ref(),
        "invalid_cursor"
    );
    let mut exact = request(Operation::Evidence);
    exact.arguments.scope.request_id = vec!["r1:1".into()];
    let selected = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            exact,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(selected.result["items"].as_array().unwrap().len(), 2);
    assert!(
        selected.result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["request_id"] == "r1:1")
    );
    let latest = engine
        .execute(
            path,
            true,
            "explicit",
            request(Operation::Requests),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        latest.result["items"][0]["exchange"]["boundary"]["data"]["client_receipt"],
        "unknown"
    );
    assert_eq!(
        latest.result["items"][1]["exchange"]["acceptance"]["transaction_signature"],
        "B"
    );
}
#[tokio::test]
async fn exchange_projection_honors_response_and_filter_budgets() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store
        .append(&[
            event("rpc.registered", "r1:0", json!({"method":"getHealth"})),
            event(
                "rpc.response_prepared",
                "r1:0",
                json!({"result":"x".repeat(8000)}),
            ),
        ])
        .unwrap();
    let engine = Engine::with_limits(Limits {
        response_bytes: 4096,
        ..Default::default()
    })
    .unwrap();
    let result = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            request(Operation::Requests),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(serde_json::to_vec(&result).unwrap().len() <= 4096);
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains(&"x".repeat(8000))
    );
    assert!(
        result.result["items"][0]["exchange"]["prepared"]["data"]
            .get("result")
            .is_none()
    );
    let tiny = Engine::with_limits(Limits {
        response_bytes: 512,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        tiny.execute(
            path.clone(),
            true,
            "explicit",
            request(Operation::Requests),
            CancellationToken::new()
        )
        .await
        .unwrap_err()
        .code
        .as_ref(),
        "result_too_large"
    );
    let mut reduced = request(Operation::Requests);
    reduced.arguments.fields = vec!["rpc_method".into()];
    let result = engine
        .execute(path, true, "explicit", reduced, CancellationToken::new())
        .await
        .unwrap();
    assert!(result.result["items"][0].get("exchange").is_none());
    let mut invalid = request(Operation::Requests);
    invalid.arguments.scope.request_id = (0..201).map(|n| format!("r{n}:0")).collect();
    assert_eq!(
        invalid.normalize().unwrap_err().code.as_ref(),
        "invalid_argument"
    );
}

#[tokio::test]
async fn newest_pages_are_bounded_and_old_cursors_keep_their_snapshot_and_direction() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    for n in 1..=55 {
        store
            .append(&[event(
                "rpc.registered",
                &format!("r{n}"),
                json!({"method":"getHealth","registration_seq":n}),
            )])
            .unwrap();
    }
    let mut r = request(Operation::Requests);
    r.arguments = serde_json::from_value(json!({"limit":2,"order":"desc"})).unwrap();
    let engine = Engine::default();
    let first = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            r.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(first.result["items"][0]["request_id"], "r55");
    assert_eq!(first.result["items"][1]["request_id"], "r54");
    store
        .append(&[event(
            "rpc.registered",
            "r56",
            json!({"method":"getHealth","registration_seq":56}),
        )])
        .unwrap();
    let latest = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            r.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(latest.result["items"][0]["request_id"], "r56");
    r.arguments.cursor = first.page.next_cursor;
    let older = engine
        .execute(
            path.clone(),
            true,
            "explicit",
            r.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(older.view.snapshot_seq, 55);
    assert_eq!(older.result["items"][0]["request_id"], "r53");
    r.arguments = serde_json::from_value(json!({"limit":2})).unwrap();
    let ascending = engine
        .execute(path, true, "explicit", r, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(ascending.result["items"][0]["request_id"], "r1");
}
