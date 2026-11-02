use ronchi::{
    config::Mode,
    journal::{
        DiskLimits, Event, JournalError, JournalReader, JournalState, JournalStore, JournalWriter,
        MIN_SHUTDOWN_RESERVE, WriterHealth,
    },
};
use tempfile::TempDir;
fn event(size: usize) -> Event {
    Event {
        schema_version: 1,
        kind: "observed".into(),
        recorded_at: "2026-11-02T00:00:00Z".into(),
        elapsed_ns: 0,
        source: "ronchi".into(),
        execution_mode: Mode::Simulator,
        data: serde_json::json!({"payload":"x".repeat(size)}),
    }
}
fn limits(headroom: u64) -> DiskLimits {
    DiskLimits {
        total_bytes: MIN_SHUTDOWN_RESERVE + headroom,
        shutdown_reserve_bytes: MIN_SHUTDOWN_RESERVE,
    }
}

#[test]
fn defaults_and_initial_reserve_are_enforced_before_creation() {
    assert_eq!(DiskLimits::default().total_bytes, 1024 * 1024 * 1024);
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    assert!(matches!(
        JournalStore::with_disk_limits(&path, "L1", limits(1)),
        Err(JournalError::DiskBudget)
    ));
    assert!(!path.exists());
    let invalid = DiskLimits {
        total_bytes: 1024 * 1024,
        shutdown_reserve_bytes: 0,
    };
    assert!(matches!(
        JournalStore::with_disk_limits(&path, "L1", invalid),
        Err(JournalError::InvalidInput)
    ));
}

#[test]
fn real_database_and_wal_growth_stop_new_work_without_evicting_history() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let mut store = JournalStore::with_disk_limits(&path, "L1", limits(2 * 1024 * 1024)).unwrap();
    let initial = store.disk_usage().unwrap().used_bytes;
    let initial_wal = std::fs::metadata(tmp.path().join("journal.sqlite-wal"))
        .unwrap()
        .len();
    let mut acknowledged = 0;
    loop {
        match store.append(&[event(48 * 1024)]) {
            Ok(seq) => acknowledged = seq,
            Err(JournalError::DiskBudget) => break,
            other => panic!("unexpected append: {other:?}"),
        }
        assert!(acknowledged < 100, "budget did not limit actual growth");
    }
    assert!(acknowledged > 0);
    let usage = store.disk_usage().unwrap();
    assert!(usage.used_bytes > initial);
    assert!(
        std::fs::metadata(tmp.path().join("journal.sqlite-wal"))
            .unwrap()
            .len()
            > initial_wal
    );
    assert!(usage.used_bytes + usage.shutdown_reserve_bytes <= usage.total_bytes);
    assert_eq!(usage.reserved_bytes, 0);
    assert!(matches!(
        store.append(&[event(0)]),
        Err(JournalError::WriterFailed)
    ));
    store.finish(&event(0), JournalState::Interrupted).unwrap();
    drop(store);
    let reader = JournalReader::open(&path).unwrap();
    assert_eq!(
        reader.snapshot(false).unwrap().snapshot_seq,
        acknowledged + 1
    );
    assert_eq!(
        reader.snapshot(false).unwrap().state,
        JournalState::Interrupted
    );
    for seq in 1..=acknowledged {
        assert_eq!(
            reader.event(seq).unwrap().unwrap().data["payload"]
                .as_str()
                .unwrap()
                .len(),
            48 * 1024
        );
    }
    assert!(
        std::fs::metadata(&path).unwrap().len() > 4096,
        "checkpointed database growth is retained"
    );
}

#[test]
fn all_existing_lab_files_share_the_budget_and_fail_the_owner() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let store = JournalStore::with_disk_limits(&path, "L1", limits(2 * 1024 * 1024)).unwrap();
    let writer = JournalWriter::start(store).unwrap();
    let handle = writer.handle();
    handle
        .try_append(&[event(0)])
        .unwrap()
        .blocking_committed()
        .unwrap();
    let initial = handle.disk_usage().unwrap().used_bytes;
    // Stand-ins for future owned files; no attachment or logging subsystem is introduced.
    for name in ["retained", "attachments", "diagnostics"] {
        std::fs::create_dir(tmp.path().join(name)).unwrap();
        std::fs::write(
            tmp.path().join(name).join("owned.dat"),
            vec![0u8; 700 * 1024],
        )
        .unwrap();
    }
    let usage = handle.disk_usage().unwrap();
    assert!(usage.used_bytes >= initial + 2100 * 1024);
    assert!(matches!(
        handle.try_append(&[event(0)]),
        Err(JournalError::DiskBudget)
    ));
    assert_eq!(handle.status().health, WriterHealth::Failed);
    assert!(writer.shutdown(None).is_err());
    let reader = JournalReader::open(&path).unwrap();
    assert_eq!(reader.snapshot(false).unwrap().snapshot_seq, 1);
    assert_eq!(
        reader.snapshot(false).unwrap().state,
        JournalState::Interrupted
    );
    assert!(reader.event(1).unwrap().is_some());
    assert_eq!(
        std::fs::read(tmp.path().join("retained/owned.dat"))
            .unwrap()
            .len(),
        700 * 1024
    );
}

#[test]
fn concurrent_admissions_cannot_spend_the_same_disk_headroom() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let writer = JournalWriter::start(
        JournalStore::with_disk_limits(&path, "L1", limits(2 * 1024 * 1024)).unwrap(),
    )
    .unwrap();
    let handle = writer.handle();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(17));
    let mut threads = Vec::new();
    for _ in 0..16 {
        let handle = handle.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            handle.try_append(&[event(48 * 1024)])
        }));
    }
    barrier.wait();
    let mut receipts = Vec::new();
    let mut rejected = 0;
    for thread in threads {
        match thread.join().unwrap() {
            Ok(receipt) => receipts.push(receipt),
            Err(JournalError::DiskBudget | JournalError::WriterFailed) => rejected += 1,
            Err(other) => panic!("unexpected failure: {other}"),
        }
    }
    assert!(rejected > 0);
    let mut committed = 0;
    for receipt in receipts {
        if let Ok(seq) = receipt.blocking_committed() {
            committed = committed.max(seq);
        }
    }
    assert!(committed > 0);
    assert!(writer.shutdown(None).is_err());
    let reader = JournalReader::open(&path).unwrap();
    assert_eq!(reader.snapshot(false).unwrap().snapshot_seq, committed);
    for seq in 1..=committed {
        assert!(reader.event(seq).unwrap().is_some());
    }
}

#[test]
fn free_filesystem_capacity_is_checked_before_creating_sqlite_files() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let available = fs2::available_space(tmp.path()).unwrap();
    let limits = DiskLimits {
        total_bytes: u64::MAX,
        shutdown_reserve_bytes: available + 1024 * 1024 * 1024,
    };
    assert!(matches!(
        JournalStore::with_disk_limits(&path, "L1", limits),
        Err(JournalError::DiskSpace)
    ));
    assert!(!path.exists());
}

#[test]
fn shutdown_reserve_is_unavailable_to_normal_admission_but_can_record_interruption() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("journal.sqlite");
    let mut store = JournalStore::with_disk_limits(&path, "L1", limits(2 * 1024 * 1024)).unwrap();
    store.append(&[event(0)]).unwrap();
    let padding = std::fs::File::create(tmp.path().join("retained.data")).unwrap();
    let usage = store.disk_usage().unwrap();
    padding
        .set_len(usage.total_bytes - usage.shutdown_reserve_bytes - usage.used_bytes)
        .unwrap();
    let usage = store.disk_usage().unwrap();
    assert_eq!(
        usage.used_bytes + usage.shutdown_reserve_bytes,
        usage.total_bytes
    );
    assert!(matches!(
        store.append(&[event(0)]),
        Err(JournalError::DiskBudget)
    ));
    assert!(matches!(
        store.finish(&event(0), JournalState::Finalized),
        Err(JournalError::WriterFailed)
    ));
    store.finish(&event(0), JournalState::Interrupted).unwrap();
    drop(store);
    let snapshot = JournalReader::open(&path).unwrap().snapshot(false).unwrap();
    assert_eq!(snapshot.snapshot_seq, 2);
    assert_eq!(snapshot.state, JournalState::Interrupted);
}
