use ronchi::{
    config::Mode,
    journal::{Event, JournalError, JournalReader, JournalState, JournalStore, MAX_EVENT_BYTES},
};
use tempfile::TempDir;

fn event(kind: &str) -> Event {
    Event {
        schema_version: 1,
        kind: kind.into(),
        recorded_at: "2026-11-02T00:00:00Z".into(),
        elapsed_ns: 0,
        source: "ronchi".into(),
        execution_mode: Mode::Simulator,
        data: serde_json::json!({"version":1}),
    }
}

#[test]
fn durable_atomic_units_and_terminal_state() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    assert_eq!(store.snapshot().unwrap().snapshot_seq, 0);
    assert_eq!(
        store
            .append(&[event("attempt"), event("acceptance")])
            .unwrap(),
        2
    );
    let reader = JournalReader::open(&path).unwrap();
    assert_eq!(reader.snapshot(true).unwrap().state, JournalState::Open);
    assert_eq!(reader.event(2).unwrap().unwrap().kind, "acceptance");
    assert!(reader.event(3).unwrap().is_none());
    assert_eq!(
        store
            .finish(&event("lab.finalized"), JournalState::Finalized)
            .unwrap(),
        3
    );
    assert!(matches!(
        store.append(&[event("late")]),
        Err(JournalError::Closed)
    ));
    drop(store);
    let snapshot = JournalReader::open(&path).unwrap().snapshot(false).unwrap();
    assert_eq!(snapshot.lab_id, "L1");
    assert_eq!(snapshot.snapshot_seq, 3);
    assert_eq!(snapshot.state, JournalState::Finalized);
    assert!(JournalStore::create(&path, "L2").is_err());
    assert_eq!(reader.snapshot(false).unwrap().lab_id, "L1");
}

#[test]
fn storage_failure_rolls_back_whole_unit_and_watermark() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    store.append(&[event("start")]).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_append BEFORE INSERT ON events WHEN NEW.event_seq=3 BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;").unwrap();
    assert!(matches!(
        store.append(&[event("attempt"), event("acceptance")]),
        Err(JournalError::Storage(_))
    ));
    let reader = JournalReader::open(&path).unwrap();
    assert_eq!(reader.snapshot(true).unwrap().snapshot_seq, 1);
    assert!(reader.event(2).unwrap().is_none());
    drop(store);
    assert_eq!(
        reader.snapshot(false).unwrap().state,
        JournalState::Interrupted
    );
}

#[test]
fn invalid_and_oversized_units_have_no_effects() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "L1").unwrap();
    assert!(store.append(&[]).is_err());
    let mut bad = event("bad");
    bad.schema_version = 2;
    assert!(store.append(&[event("valid"), bad]).is_err());
    let mut huge = event("large");
    huge.data = serde_json::json!({"payload":"x".repeat(MAX_EVENT_BYTES)});
    assert!(store.append(&[huge]).is_err());
    let many: Vec<_> = (0..257).map(|_| event("small")).collect();
    assert!(store.append(&many).is_err());
    assert_eq!(store.snapshot().unwrap().snapshot_seq, 0);
    assert!(store.finish(&event("end"), JournalState::Open).is_err());
}

#[test]
fn writer_process_fixture() {
    let Some(path) = std::env::var_os("RONCHI_TEST_JOURNAL") else {
        return;
    };
    let mut store = JournalStore::create(std::path::Path::new(&path), "L-crash").unwrap();
    store
        .append(&[event("attempt"), event("acceptance")])
        .unwrap();
    use std::io::Write;
    println!("COMMITTED");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn acknowledged_events_survive_process_death() {
    use std::io::BufRead;
    use std::process::{Command, Stdio};
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "writer_process_fixture", "--nocapture"])
        .env("RONCHI_TEST_JOURNAL", &path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let mut committed = false;
    for line in std::io::BufReader::new(output).lines() {
        if line.unwrap().contains("COMMITTED") {
            committed = true;
            break;
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(committed);
    let reader = JournalReader::open(&path).unwrap();
    let snapshot = reader.snapshot(false).unwrap();
    assert_eq!(snapshot.state, JournalState::Interrupted);
    assert_eq!(snapshot.snapshot_seq, 2);
    assert_eq!(reader.event(1).unwrap().unwrap().kind, "attempt");
    assert_eq!(reader.event(2).unwrap().unwrap().kind, "acceptance");
    let db = rusqlite::Connection::open(&path).unwrap();
    let mode: String = db
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    let version: String = db
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .unwrap();
    let source: String = db
        .query_row("SELECT sqlite_source_id()", [], |r| r.get(0))
        .unwrap();
    println!("SQLite {version}; source {source}");
}

#[test]
fn dedicated_writer_commits_units_and_drains_before_finalization() {
    use ronchi::journal::{JournalWriter, WriterHealth};
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let writer = JournalWriter::start(JournalStore::create(&path, "L1").unwrap()).unwrap();
    let handle = writer.handle();
    let first = handle
        .try_append(&[event("attempt"), event("acceptance")])
        .unwrap();
    let second = handle.try_append(&[event("observed")]).unwrap();
    assert_eq!(first.blocking_committed().unwrap(), 2);
    assert_eq!(second.blocking_committed().unwrap(), 3);
    let abandoned = handle.try_append(&[event("caller_disconnected")]).unwrap();
    drop(abandoned);
    assert_eq!(
        writer
            .shutdown(Some((&event("lab.finalized"), JournalState::Finalized)))
            .unwrap(),
        5
    );
    assert_eq!(handle.status().health, WriterHealth::Closed);
    assert_eq!(handle.status().snapshot_seq, 5);
    assert!(matches!(
        handle.try_append(&[event("late")]),
        Err(JournalError::Closed)
    ));
    let reader = JournalReader::open(&path).unwrap();
    assert_eq!(
        reader.snapshot(false).unwrap().state,
        JournalState::Finalized
    );
    assert_eq!(
        reader.event(4).unwrap().unwrap().kind,
        "caller_disconnected"
    );
}

#[test]
fn writer_capacity_failure_is_permanent_for_count_and_bytes() {
    use ronchi::journal::{JournalWriter, QueueLimits, WriterHealth};
    for limits in [
        QueueLimits {
            events: 1,
            bytes: 1024 * 1024,
        },
        QueueLimits {
            events: 4096,
            bytes: 1,
        },
    ] {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("journal.sqlite");
        let writer =
            JournalWriter::with_limits(JournalStore::create(&path, "L1").unwrap(), limits).unwrap();
        let handle = writer.handle();
        assert!(matches!(
            handle.try_append(&[event("attempt"), event("acceptance")]),
            Err(JournalError::Saturated)
        ));
        assert_eq!(handle.status().health, WriterHealth::Failed);
        assert!(matches!(
            handle.try_append(&[event("retry")]),
            Err(JournalError::WriterFailed)
        ));
        assert!(
            writer
                .shutdown(Some((&event("end"), JournalState::Finalized)))
                .is_err()
        );
        let snapshot = JournalReader::open(&path).unwrap().snapshot(false).unwrap();
        assert_eq!(snapshot.state, JournalState::Interrupted);
        assert_eq!(snapshot.snapshot_seq, 0);
    }
}

#[test]
fn writer_storage_failure_resolves_receipts_and_fails_health() {
    use ronchi::journal::{JournalWriter, WriterHealth};
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let store = JournalStore::create(&path, "L1").unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'injected disk error'); END;").unwrap();
    let writer = JournalWriter::start(store).unwrap();
    let handle = writer.handle();
    let receipt = handle
        .try_append(&[event("attempt"), event("acceptance")])
        .unwrap();
    assert!(receipt.blocking_committed().is_err());
    assert_eq!(handle.status().health, WriterHealth::Failed);
    assert!(writer.shutdown(None).is_err());
    assert_eq!(
        JournalReader::open(&path)
            .unwrap()
            .snapshot(false)
            .unwrap()
            .snapshot_seq,
        0
    );
}

#[test]
fn case_activity_counts_only_committed_effects_for_the_current_activation() {
    use ronchi::journal::JournalWriter;
    use serde_json::json;
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("journal.sqlite");
    let writer = JournalWriter::start(JournalStore::create(&path, "L1").unwrap()).unwrap();
    let handle = writer.handle();
    let change = |revision| {
        let mut e = event("case.changed");
        e.data = json!({"binding":{"policy":{"revision":revision}}});
        e
    };
    let applied = |revision| {
        let mut e = event("fault.withholding_started");
        e.data = json!({"request":{"binding":{"policy":{"revision":revision}}}});
        e
    };
    handle
        .try_append(&[change(1), applied(1), applied(1)])
        .unwrap()
        .blocking_committed()
        .unwrap();
    assert_eq!(handle.status().case_activity.unwrap().applied_records, 2);
    handle
        .try_append(&[change(2), applied(1)])
        .unwrap()
        .blocking_committed()
        .unwrap();
    assert_eq!(
        handle.status().case_activity.unwrap().applied_records,
        0,
        "old held work must not increment the new activation"
    );
    let gate = rusqlite::Connection::open(&path).unwrap();
    gate.execute_batch("CREATE TRIGGER reject_effect BEFORE INSERT ON events WHEN json_extract(NEW.event_json,'$.kind')='fault.withholding_started' BEGIN SELECT RAISE(ABORT,'effect write rejected'); END;").unwrap();
    assert!(
        handle
            .try_append(&[applied(2)])
            .unwrap()
            .blocking_committed()
            .is_err()
    );
    assert_eq!(
        handle.status().case_activity.unwrap().applied_records,
        0,
        "uncommitted effects must not count"
    );
    assert!(writer.shutdown(None).is_err());
}

#[test]
fn rpc_traffic_is_committed_lab_wide_and_replays_after_shutdown() {
    use ronchi::journal::JournalWriter;
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("journal.sqlite");
    let writer = JournalWriter::start(JournalStore::create(&path, "Lrates").unwrap()).unwrap();
    let handle = writer.handle();
    let at = |kind: &str, seconds: u64| {
        let mut fact = event(kind);
        fact.elapsed_ns = seconds * 1_000_000_000;
        fact
    };
    // First two are in one interval. Exactly ten seconds starts a new interval.
    // Batch members are separate registered calls, even with the same timestamp.
    handle
        .try_append(&[
            at("rpc.registered", 5),
            at("rpc.boundary", 6),
            at("rpc.registered", 14),
            at("rpc.registered", 24),
            at("rpc.registered", 24),
            at("unrelated.observation", 30),
        ])
        .unwrap()
        .blocking_committed()
        .unwrap();
    let traffic = handle.status().rpc_traffic;
    assert_eq!(traffic.requests, 4);
    assert_eq!(traffic.first_elapsed_ns, Some(5_000_000_000));
    assert_eq!(traffic.active_completed_ns, 9_000_000_000);
    assert_eq!(traffic.active_start_elapsed_ns, 24_000_000_000);
    assert_eq!(traffic.observed_elapsed_ns, 30_000_000_000);
    assert!(!traffic.end_known);
    assert_eq!(
        traffic,
        JournalReader::open(&path).unwrap().rpc_traffic().unwrap()
    );
    writer
        .shutdown(Some((&at("lab.finalized", 70), JournalState::Finalized)))
        .unwrap();
    let final_traffic = handle.status().rpc_traffic;
    assert_eq!(final_traffic.requests, 4);
    assert_eq!(final_traffic.observed_elapsed_ns, 70_000_000_000);
    assert!(final_traffic.end_known);
    assert!(!final_traffic.running);
    assert_eq!(
        final_traffic,
        JournalReader::open(&path).unwrap().rpc_traffic().unwrap()
    );
}

#[test]
fn rejected_rpc_registration_never_changes_traffic_counters() {
    use ronchi::journal::JournalWriter;
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("journal.sqlite");
    let writer = JournalWriter::start(JournalStore::create(&path, "Lrates").unwrap()).unwrap();
    let handle = writer.handle();
    handle
        .try_append(&[event("rpc.registered")])
        .unwrap()
        .blocking_committed()
        .unwrap();
    let before = handle.status().rpc_traffic;
    let gate = rusqlite::Connection::open(&path).unwrap();
    gate.execute_batch("CREATE TRIGGER reject_rpc BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT,'registration rejected'); END;").unwrap();
    assert!(
        handle
            .try_append(&[event("rpc.registered")])
            .unwrap()
            .blocking_committed()
            .is_err()
    );
    assert_eq!(handle.status().rpc_traffic, before);
    assert_eq!(
        JournalReader::open(&path).unwrap().rpc_traffic().unwrap(),
        before
    );
    assert!(writer.shutdown(None).is_err());
}

#[test]
fn traffic_replay_handles_legacy_history_empty_history_and_concurrent_timestamps() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "Lrates").unwrap();
    assert_eq!(
        JournalReader::open(&path)
            .unwrap()
            .rpc_traffic()
            .unwrap()
            .requests,
        0
    );
    let mut first = event("rpc.registered");
    first.elapsed_ns = 12_000_000_000;
    let mut concurrent = event("rpc.registered");
    concurrent.elapsed_ns = 11_999_000_000;
    store.append(&[first, concurrent]).unwrap();
    // No materialized query index is required by the retained replay.
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TRIGGER query_projection; DROP TABLE query_events;")
        .unwrap();
    let replay = JournalReader::open(&path).unwrap().rpc_traffic().unwrap();
    assert_eq!(replay.requests, 2);
    assert_eq!(replay.last_rpc_elapsed_ns, Some(12_000_000_000));
    assert_eq!(replay.active_completed_ns, 0);
    assert!(!replay.end_known);
}

#[test]
fn retired_application_records_remain_readable_as_original_evidence() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("journal.sqlite");
    let mut store = JournalStore::create(&path, "Lhistory").unwrap();
    let mut original = event("check.observation");
    original.data = serde_json::json!({"source":"retired-app","facts":{"state":"pending"}});
    store.append(&[original]).unwrap();
    store
        .finish(&event("lab.finalized"), JournalState::Finalized)
        .unwrap();
    drop(store);
    let retained = JournalReader::open(&path)
        .unwrap()
        .event(1)
        .unwrap()
        .unwrap();
    assert_eq!(retained.kind, "check.observation");
    assert_eq!(retained.data["facts"]["state"], "pending");
}
