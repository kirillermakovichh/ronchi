use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

struct Project(TempDir);
impl Project {
    fn new() -> Self {
        Self(TempDir::new().unwrap())
    }
    fn path(&self) -> &Path {
        self.0.path()
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_ronchi"));
        for key in [
            "RONCHI_MODE",
            "RONCHI_READ_UPSTREAM",
            "RONCHI_SURFPOOL_DATASOURCE",
            "RONCHI_RPC_LISTEN",
            "RONCHI_CONTROL_LISTEN",
            "RONCHI_INITIAL_CASE",
            "RONCHI_SLOT_DURATION_MS",
        ] {
            c.env_remove(key);
        }
        c.arg("--project")
            .arg(self.path())
            .arg("--json")
            .args(args)
            .stdin(Stdio::null());
        c
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert!(out.stderr.is_empty(), "{out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn up(&self) -> String {
        self.ok(&[
            "up",
            "--detach",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ])["lab_id"]
            .as_str()
            .unwrap()
            .to_string()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = self.run(&["down", "--force"]);
    }
}
fn code(out: Output, expected: &str) {
    assert!(!out.status.success(), "{out:?}");
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["error"]["code"], expected, "{value}");
}
#[test]
fn delete_requires_exact_stopped_identity_and_preserves_reused_name() {
    let p = Project::new();
    let old = p.up();
    code(p.run(&["lab", "delete"]), "invalid_argument");
    code(
        p.run(&["--lab-id", &old, "lab", "delete"]),
        "state_conflict",
    );
    p.ok(&["down"]);
    let current = p.up();
    let deleted = p.ok(&["--lab-id", &old, "lab", "delete"]);
    assert_eq!(deleted["result"]["deleted_lab_id"], old);
    assert!(!p.path().join("ronchi/labs").join(&old).exists());
    assert_eq!(p.ok(&["status"])["lab_id"], current);
    code(p.run(&["--lab-id", &old, "status"]), "lab_unavailable");
}

fn lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
#[test]
fn export_pins_selection_and_survives_deletion_without_raw_payloads() {
    let p = Project::new();
    let id = p.up();
    let out = p.ok(&[
        "--lab-id",
        &id,
        "lab",
        "export",
        "selected",
        "--kind",
        "lab.ready",
        "--max-bytes",
        "65536",
    ]);
    let path = std::path::PathBuf::from(out["result"]["path"].as_str().unwrap());
    let records = lines(&path);
    assert_eq!(
        records.first().unwrap()["format"],
        "ronchi-evidence-ndjson-v1"
    );
    assert_eq!(records[0]["lab_id"], id);
    assert_eq!(
        records[0]["scope"]["kind"],
        serde_json::json!(["lab.ready"])
    );
    assert_eq!(records[0]["replayable"], false);
    assert_eq!(records.last().unwrap()["selection_complete"], true);
    let page = &records[1]["response"];
    assert_eq!(page["result"]["items"][0]["kind"], "lab.ready");
    assert_eq!(page["result"]["items"][0]["event_ref"]["lab_id"], id);
    assert!(page["result"]["items"][0].get("payload").is_none());
    p.ok(&["down"]);
    p.ok(&["--lab-id", &id, "lab", "delete"]);
    assert!(path.exists());
    assert!(std::fs::metadata(path).unwrap().len() <= 65536);
}

fn replace_journal(p: &Project, id: &str) -> ronchi::journal::JournalStore {
    let directory = p.path().join("ronchi/labs").join(id);
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(directory.join(format!("journal.sqlite{suffix}")));
    }
    ronchi::journal::JournalStore::create(&directory.join("journal.sqlite"), id).unwrap()
}
fn fixture_event(number: u64) -> ronchi::journal::Event {
    ronchi::journal::Event {
        schema_version: 1,
        kind: "fixture.observed".into(),
        recorded_at: "2026-11-03T00:00:00Z".into(),
        elapsed_ns: number,
        source: "test_fixture".into(),
        execution_mode: ronchi::config::Mode::Simulator,
        data: serde_json::json!({"number":number,"summary":"x".repeat(1000),"payload":"PRIVATE_SELECTED_PAYLOAD"}),
    }
}
#[test]
fn bounded_export_reports_truncation_and_never_overwrites_or_escapes() {
    let p = Project::new();
    let id = p.up();
    p.ok(&["down"]);
    let mut store = replace_journal(&p, &id);
    store
        .append(&(0..120).map(fixture_event).collect::<Vec<_>>())
        .unwrap();
    let out = p.ok(&[
        "--lab-id",
        &id,
        "lab",
        "export",
        "bounded",
        "--max-bytes",
        "16384",
    ]);
    assert_eq!(out["result"]["truncated"], true);
    assert_eq!(out["result"]["selection_complete"], false);
    assert_eq!(out["result"]["reason"], "output_bytes");
    assert!(out["result"]["pages"].as_u64().unwrap() > 0);
    let path = std::path::PathBuf::from(out["result"]["path"].as_str().unwrap());
    assert!(std::fs::metadata(&path).unwrap().len() <= 16384);
    let original = std::fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&original).contains("PRIVATE_SELECTED_PAYLOAD"));
    code(
        p.run(&["--lab-id", &id, "lab", "export", "bounded"]),
        "state_conflict",
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    code(
        p.run(&["--lab-id", &id, "lab", "export", "../escape"]),
        "invalid_argument",
    );
    code(
        p.run(&[
            "--lab-id",
            &id,
            "lab",
            "export",
            "small",
            "--max-bytes",
            "42",
        ]),
        "invalid_argument",
    );
    assert_eq!(
        ronchi::journal::JournalReader::open(
            &p.path()
                .join("ronchi/labs")
                .join(&id)
                .join("journal.sqlite")
        )
        .unwrap()
        .snapshot(false)
        .unwrap()
        .snapshot_seq,
        120
    );
}
#[test]
fn raw_selection_preserves_redaction_and_exposes_chunk_omissions() {
    let p = Project::new();
    let id = p.up();
    p.ok(&["down"]);
    let mut store = replace_journal(&p, &id);
    let mut redacted = fixture_event(1);
    redacted.data["payload_status"] = serde_json::json!("redacted");
    let mut long = fixture_event(2);
    long.data["payload"] = serde_json::json!("🙂".repeat(3000));
    store.append(&[redacted, long]).unwrap();
    let out = p.ok(&["--lab-id", &id, "lab", "export", "raw", "--include-payload"]);
    let path = std::path::PathBuf::from(out["result"]["path"].as_str().unwrap());
    let records = lines(&path);
    let items = records[1]["response"]["result"]["items"]
        .as_array()
        .unwrap();
    assert_eq!(items[0]["payload_status"], "redacted");
    assert!(items[0]["payload"].is_null());
    assert_eq!(items[1]["payload"]["payload_truncated"], true);
    assert_eq!(out["result"]["selection_complete"], true);
    assert_eq!(out["result"]["payload_complete"], false);
    assert!(
        !std::fs::read_to_string(path)
            .unwrap()
            .contains("PRIVATE_SELECTED_PAYLOAD")
    );
}

#[test]
fn actual_retained_viewer_lease_blocks_deletion_until_owner_exits() {
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};
    let p = Project::new();
    let id = p.up();
    p.ok(&["down"]);
    let mut viewer = p
        .command(&["--lab-id", &id, "dashboard", "--serve"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(viewer.stdout.take().unwrap());
    let mut ready = String::new();
    stdout.read_line(&mut ready).unwrap();
    let response: Value = serde_json::from_str(&ready).unwrap();
    assert_eq!(response["lab_id"], id);
    code(p.run(&["--lab-id", &id, "lab", "delete"]), "state_conflict");
    drop(stdout);
    let deadline = Instant::now() + Duration::from_secs(5);
    while viewer.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            viewer.kill().unwrap();
            panic!("retained viewer did not release its owned resources");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    p.ok(&["--lab-id", &id, "lab", "delete"]);
}
#[test]
fn deletion_rejects_symlinked_artifacts_and_preserves_external_files() {
    use std::os::unix::fs::symlink;
    let p = Project::new();
    let id = p.up();
    p.ok(&["down"]);
    let outside = TempDir::new().unwrap();
    let valuable = outside.path().join("user-data");
    std::fs::write(&valuable, "preserve").unwrap();
    let link = p.path().join("ronchi/labs").join(&id).join("external");
    symlink(outside.path(), &link).unwrap();
    code(p.run(&["--lab-id", &id, "lab", "delete"]), "state_conflict");
    assert_eq!(std::fs::read_to_string(&valuable).unwrap(), "preserve");
    assert!(
        p.path()
            .join("ronchi/labs")
            .join(&id)
            .join("journal.sqlite")
            .exists()
    );
    std::fs::remove_file(link).unwrap();
    p.ok(&["--lab-id", &id, "lab", "delete"]);
    assert_eq!(std::fs::read_to_string(valuable).unwrap(), "preserve");
}
#[test]
fn process_death_retains_wal_and_export_never_migrates_original_schema() {
    use std::time::{Duration, Instant};
    let p = Project::new();
    let id = p.up();
    let manifest = ronchi::workspace::Workspace::new(p.path())
        .manifest(&id)
        .unwrap();
    let process = manifest.process.unwrap();
    // SAFETY: PID came from the authenticated ready Lab started in this disposable project.
    assert_eq!(unsafe { libc::kill(process.pid as i32, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(3);
    while ronchi::process::matches(&process).unwrap() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let directory = p.path().join("ronchi/labs").join(&id);
    let db = directory.join("journal.sqlite");
    let wal = directory.join("journal.sqlite-wal");
    assert!(std::fs::metadata(&wal).unwrap().len() > 0);
    let db_before = std::fs::read(&db).unwrap();
    let wal_before = std::fs::read(&wal).unwrap();
    let out = p.ok(&["--lab-id", &id, "lab", "export", "recovered"]);
    let records = lines(std::path::Path::new(
        out["result"]["path"].as_str().unwrap(),
    ));
    assert_eq!(records[0]["journal_state"], "interrupted");
    assert!(out["result"]["snapshot_seq"].as_u64().unwrap() > 0);
    assert_eq!(std::fs::read(&db).unwrap(), db_before);
    assert_eq!(std::fs::read(&wal).unwrap(), wal_before);
    // Unsupported future schema is a failure, with no in-place migration or output replacement.
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection.pragma_update(None, "user_version", 999).unwrap();
    let db_before = std::fs::read(&db).unwrap();
    let wal_before = std::fs::read(&wal).ok();
    code(
        p.run(&["--lab-id", &id, "lab", "export", "unsupported"]),
        "internal_error",
    );
    assert_eq!(std::fs::read(&db).unwrap(), db_before);
    assert_eq!(std::fs::read(&wal).ok(), wal_before);
    assert!(!p.path().join("ronchi/exports/unsupported.ndjson").exists());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_export_removes_only_its_incomplete_file() {
    use ronchi::{
        maintenance::{ExportOptions, export},
        query::Arguments,
    };
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;
    let p = Project::new();
    let id = p.up();
    p.ok(&["down"]);
    let mut store = replace_journal(&p, &id);
    for start in [0, 200, 400] {
        store
            .append(&(start..start + 200).map(fixture_event).collect::<Vec<_>>())
            .unwrap();
    }
    let path = p.path().join("ronchi/exports/cancelled.ndjson");
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let project = p.path().to_path_buf();
    let selected = id.clone();
    let task = tokio::spawn(async move {
        export(
            project,
            &selected,
            ExportOptions {
                name: "cancelled".into(),
                max_bytes: 8 * 1024 * 1024,
                arguments: Arguments {
                    limit: 1,
                    ..Default::default()
                },
            },
            token,
        )
        .await
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while std::fs::metadata(&path).map_or(true, |metadata| metadata.len() == 0) {
        assert!(Instant::now() < deadline, "export did not start");
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    cancel.cancel();
    assert_eq!(task.await.unwrap().unwrap_err().code, "cancelled");
    assert!(!path.exists());
    let reader = ronchi::journal::JournalReader::open(
        &p.path().join("ronchi/labs").join(id).join("journal.sqlite"),
    )
    .unwrap();
    assert_eq!(reader.snapshot(false).unwrap().snapshot_seq, 600);
}

#[test]
fn maintenance_schemas_are_offline_and_expose_exact_identity_and_byte_bounds() {
    let p = Project::new();
    let deletion = p.ok(&["schema", "lab.delete"]);
    assert_eq!(deletion["operation"], "lab.delete");
    assert!(
        deletion["request"]["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("lab_id"))
    );
    let export = p.ok(&["schema", "lab.export"]);
    let options = &export["request"]["$defs"]["ExportOptions"];
    assert_eq!(options["properties"]["max_bytes"]["minimum"], 16384);
    assert_eq!(options["properties"]["max_bytes"]["maximum"], 67108864);
    assert!(!p.path().join("ronchi").exists());
}
#[test]
fn deletion_rejects_a_still_live_recorded_child_without_signalling_it() {
    let p = Project::new();
    let id = p.up();
    p.ok(&["down"]);
    let ws = ronchi::workspace::Workspace::new(p.path());
    let mut manifest = ws.manifest(&id).unwrap();
    manifest.owned_children.push(
        ronchi::process::identity(std::process::id())
            .unwrap()
            .unwrap(),
    );
    let path = ws.lab(&id).unwrap().join("manifest.json");
    ronchi::workspace::atomic_json(&path, &manifest).unwrap();
    code(p.run(&["--lab-id", &id, "lab", "delete"]), "state_conflict");
    assert!(ronchi::process::matches(&manifest.owned_children[0]).unwrap());
    manifest.owned_children.clear();
    ronchi::workspace::atomic_json(&path, &manifest).unwrap();
    p.ok(&["--lab-id", &id, "lab", "delete"]);
}
