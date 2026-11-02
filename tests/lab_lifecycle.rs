use ronchi::{
    journal::{JournalReader, JournalState},
    lab::Manifest,
    workspace::Workspace,
};
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

struct Project {
    directory: TempDir,
}
impl Project {
    fn new() -> Self {
        Self {
            directory: TempDir::new().unwrap(),
        }
    }
    fn path(&self) -> &Path {
        self.directory.path()
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
    fn up(&self) -> Value {
        self.ok(&[
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ])
    }
    fn manifest(&self) -> Manifest {
        Workspace::new(self.path()).target("default", None).unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        if let Ok(entries) = std::fs::read_dir(self.path().join("ronchi/names")) {
            for entry in entries.flatten() {
                if let Some(name) = entry.path().file_stem().and_then(|n| n.to_str()) {
                    let _ = self.run(&["down", "--lab", name, "--force"]);
                }
            }
        }
    }
}
fn error(out: Output, exit: i32, code: &str) -> Value {
    assert_eq!(out.status.code(), Some(exit), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["code"], code);
    assert!(v.get("result").is_none());
    v
}
fn raw(address: SocketAddr, request: String) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut result = String::new();
    stream.read_to_string(&mut result).unwrap();
    result
}

#[test]
fn persistent_lifecycle_authentication_and_retained_evidence() {
    let project = Project::new();
    let result = project.up();
    assert_eq!(result["result"]["state"], "ready");
    assert_eq!(result["result"]["snapshot_seq"], 2);
    assert_eq!(project.ok(&["status"])["lab_id"], result["lab_id"]);
    let manifest = project.manifest();
    let id = &manifest.lab_id;
    let control = manifest.control_address.unwrap();
    let rpc = manifest.rpc_address.unwrap();
    let wrong = raw(
        control,
        format!(
            "GET /v1/labs/{id}/status HTTP/1.1\r\nHost: {control}\r\nAuthorization: Bearer wrong\r\nConnection: close\r\n\r\n"
        ),
    );
    assert!(wrong.starts_with("HTTP/1.1 401"));
    let token = std::fs::read_to_string(
        Workspace::new(project.path())
            .lab(id)
            .unwrap()
            .join("token"),
    )
    .unwrap();
    let origin = raw(
        control,
        format!(
            "GET /v1/labs/{id}/status HTTP/1.1\r\nHost: {control}\r\nOrigin: http://example.test\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
        ),
    );
    assert!(origin.starts_with("HTTP/1.1 403"));
    let reply = raw(
        rpc,
        format!(
            "POST / HTTP/1.1\r\nHost: {rpc}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        ),
    );
    assert!(reply.starts_with("HTTP/1.1 200"));
    assert!(reply.contains("-32600"));
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(
            Workspace::new(project.path())
                .lab(id)
                .unwrap()
                .join("token")
        )
        .unwrap()
        .permissions()
        .mode()
            & 0o777,
        0o600
    );
    let down = project.ok(&["down"]);
    assert_eq!(down["result"]["journal_state"], "finalized");
    assert_eq!(down["result"]["state"], "stopped");
    assert!(
        ronchi::process::identity(manifest.process.unwrap().pid)
            .unwrap()
            .is_none()
    );
    let reader = JournalReader::open(
        &Workspace::new(project.path())
            .lab(id)
            .unwrap()
            .join("journal.sqlite"),
    )
    .unwrap();
    assert_eq!(
        reader.snapshot(false).unwrap().state,
        JournalState::Finalized
    );
    assert_eq!(
        reader
            .event(reader.snapshot(false).unwrap().snapshot_seq)
            .unwrap()
            .unwrap()
            .kind,
        "lab.finalized"
    );
    assert_eq!(
        project.ok(&["status"])["result"]["selection_basis"],
        "latest"
    );
    assert!(
        project
            .path()
            .read_dir()
            .unwrap()
            .all(|e| e.unwrap().file_name() == "ronchi")
    );
}

#[test]
fn concurrent_starts_reuse_only_compatible_settings_and_leave_other_processes_alive() {
    let project = Project::new();
    let args = [
        "up",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ];
    let first = project
        .command(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let second = project
        .command(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let out1 = first.wait_with_output().unwrap();
    let out2 = second.wait_with_output().unwrap();
    assert!(out1.status.success(), "{out1:?}");
    assert!(out2.status.success(), "{out2:?}");
    let a: Value = serde_json::from_slice(&out1.stdout).unwrap();
    let b: Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(a["lab_id"], b["lab_id"]);
    assert_eq!(project.ok(&["up"])["lab_id"], a["lab_id"]);
    error(
        project.run(&["up", "--rpc-listen", "127.0.0.1:8898"]),
        5,
        "state_conflict",
    );
    let other = project.ok(&[
        "up",
        "--lab",
        "other",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    assert_ne!(other["lab_id"], a["lab_id"]);
    let backend = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = backend.local_addr().unwrap();
    project.ok(&["down"]);
    assert!(TcpStream::connect(endpoint).is_ok());
    assert_eq!(
        project.ok(&["status", "--lab", "other"])["result"]["state"],
        "ready"
    );
    project.ok(&["down", "--lab", "other"]);
}

#[test]
fn name_reuse_preserves_exact_historical_ids_and_rotates_credentials() {
    let project = Project::new();
    let first = project.up();
    let id = first["lab_id"].as_str().unwrap();
    let ws = Workspace::new(project.path());
    let token = std::fs::read(ws.lab(id).unwrap().join("token")).unwrap();
    project.ok(&["down"]);
    let next = project.up();
    let nextid = next["lab_id"].as_str().unwrap();
    assert_ne!(id, nextid);
    assert_ne!(
        token,
        std::fs::read(ws.lab(nextid).unwrap().join("token")).unwrap()
    );
    let historical = project.ok(&["status", "--lab-id", id]);
    assert_eq!(historical["result"]["selection_basis"], "explicit");
    assert_eq!(historical["result"]["journal_state"], "finalized");
    assert!(historical["result"]["rpc_address"].is_null());
    project.ok(&["down"]);
}

#[test]
fn occupied_ports_fail_without_terminating_the_occupant_or_leaking_listeners() {
    for flag in ["--rpc-listen", "--control-listen"] {
        let project = Project::new();
        let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = occupied.local_addr().unwrap().to_string();
        let mut args = vec![
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ];
        if flag == "--rpc-listen" {
            args[2] = &address;
        } else {
            args[4] = &address;
        }
        error(project.run(&args), 5, "state_conflict");
        assert!(TcpStream::connect(occupied.local_addr().unwrap()).is_ok());
        let m = project.manifest();
        assert!(m.rpc_address.is_none());
        assert!(m.control_address.is_none());
        assert_eq!(
            project.ok(&["status"])["result"]["journal_state"],
            "interrupted"
        );
    }
}

#[test]
fn force_stop_closes_owned_connections_and_retains_an_interrupted_journal() {
    let project = Project::new();
    project.up();
    let m = project.manifest();
    let rpc = m.rpc_address.unwrap();
    let control = m.control_address.unwrap();
    let mut held = TcpStream::connect(rpc).unwrap();
    held.write_all(b"POST / HTTP/1.1\r\n").unwrap();
    let result = project.ok(&["down", "--force"]);
    assert_eq!(result["result"]["journal_state"], "interrupted");
    assert!(TcpListener::bind(rpc).is_ok());
    assert!(TcpListener::bind(control).is_ok());
}

#[test]
fn forged_birth_identity_never_signals_or_takes_over_the_live_owner() {
    let project = Project::new();
    project.up();
    let original = project.manifest();
    let mut forged = original.clone();
    forged.process.as_mut().unwrap().birth.push_str("-wrong");
    let path = Workspace::new(project.path())
        .lab(&original.lab_id)
        .unwrap()
        .join("manifest.json");
    ronchi::workspace::atomic_json(&path, &forged).unwrap();
    error(project.run(&["up"]), 5, "state_conflict");
    error(project.run(&["status"]), 5, "state_conflict");
    error(project.run(&["down"]), 3, "lab_unavailable");
    assert_eq!(
        ronchi::process::identity(original.process.as_ref().unwrap().pid).unwrap(),
        original.process
    );
    ronchi::workspace::atomic_json(&path, &original).unwrap();
    project.ok(&["down"]);
}

#[test]
fn process_death_is_interrupted_and_the_next_start_does_not_resume_history() {
    let project = Project::new();
    project.up();
    let m = project.manifest();
    let identity = m.process.as_ref().unwrap();
    assert_eq!(
        ronchi::process::identity(identity.pid).unwrap().as_ref(),
        Some(identity)
    );
    // SAFETY: the test just authenticated the child identity and owns this isolated Lab.
    assert_eq!(unsafe { libc::kill(identity.pid as i32, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    while ronchi::process::identity(identity.pid).unwrap().is_some() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let stopped = project.ok(&["status"]);
    assert_eq!(stopped["result"]["journal_state"], "interrupted");
    assert!(stopped["result"]["rpc_address"].is_null());
    let next = project.up();
    assert_ne!(next["lab_id"], m.lab_id);
    assert_eq!(next["result"]["snapshot_seq"], 2);
    project.ok(&["down"]);
}

#[test]
fn cancellation_before_start_does_not_leave_a_lab() {
    let project = Project::new();
    let ws = Workspace::new(project.path());
    ws.initialize().unwrap();
    let lock = ws.lock("default", "operation").unwrap();
    fs2::FileExt::try_lock_exclusive(&lock).unwrap();
    let mut child = project
        .command(&[
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Wait until the CLI has installed its cancellation handler while waiting on the lock.
    std::thread::sleep(Duration::from_millis(150));
    // SAFETY: child is an unreaped process spawned and exclusively owned by this test.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("CLI did not acknowledge cancellation");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    let value = error(output, 130, "cancelled");
    assert_eq!(value["error"]["effects"], "none");
    assert_eq!(std::fs::read_dir(ws.root.join("labs")).unwrap().count(), 0);
}

#[test]
fn closing_a_real_terminal_after_readiness_does_not_stop_the_lab() {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    };
    let project = Project::new();
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: both descriptor pointers are valid; optional terminal attributes are null.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    // SAFETY: openpty returned two distinct owned descriptors.
    let mut master = unsafe { std::fs::File::from_raw_fd(master) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    let fd = slave.as_raw_fd();
    let mut command = project.command(&[
        "up",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    command
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave.try_clone().unwrap());
    // SAFETY: the post-fork closure calls only async-signal-safe OS functions.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(fd, libc::TIOCSCTTY as libc::c_ulong, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().unwrap();
    drop(slave);
    drop(command);
    let mut received = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !received.contains(&b'\n') {
        assert!(Instant::now() < deadline);
        let mut poll = libc::pollfd {
            fd: master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll points to one valid descriptor record.
        if unsafe { libc::poll(&mut poll, 1, 100) } > 0 {
            let mut bytes = [0u8; 4096];
            let n = master.read(&mut bytes).unwrap();
            assert!(n > 0);
            received.extend_from_slice(&bytes[..n]);
        }
    }
    let result: Value = serde_json::from_slice(&received).unwrap();
    assert_eq!(result["result"]["state"], "ready");
    assert!(child.wait().unwrap().success());
    drop(master);
    assert_eq!(project.ok(&["status"])["result"]["state"], "ready");
    project.ok(&["down"]);
}

#[test]
fn storage_exhaustion_is_a_product_failure_and_retains_acknowledged_evidence() {
    let project = Project::new();
    project.up();
    let m = project.manifest();
    let dir = Workspace::new(project.path()).lab(&m.lab_id).unwrap();
    // A sparse owned file exceeds the configured Lab quota without filling the host disk.
    std::fs::File::create(dir.join("retained.data"))
        .unwrap()
        .set_len(1024 * 1024 * 1024)
        .unwrap();
    let failed = error(project.run(&["down"]), 6, "internal_error");
    assert_eq!(failed["error"]["effects"], "applied");
    let status = project.ok(&["status"]);
    assert_eq!(status["result"]["journal_state"], "interrupted");
    assert_eq!(status["result"]["health"], "failed");
    assert!(status["result"]["rpc_address"].is_null());
    let reader = JournalReader::open(&dir.join("journal.sqlite")).unwrap();
    assert!(reader.snapshot(false).unwrap().snapshot_seq >= 1);
    assert_eq!(reader.event(2).unwrap().unwrap().kind, "lab.ready");
}

#[test]
fn cancelling_startup_while_readiness_is_blocked_cleans_up_the_owned_process() {
    for _ in 0..5 {
        let project = Project::new();
        let ws = Workspace::new(project.path());
        ws.initialize().unwrap();
        let mut child = project
            .command(&[
                "up",
                "--rpc-listen",
                "127.0.0.1:0",
                "--control-listen",
                "127.0.0.1:0",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let dir = loop {
            if let Some(entry) = std::fs::read_dir(ws.root.join("labs")).unwrap().next() {
                break entry.unwrap().path();
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Startup did not create its Lab directory");
            }
            std::thread::sleep(Duration::from_micros(100));
        };
        let gate = ronchi::workspace::open_private(&dir.join("readiness.gate"), true).unwrap();
        if fs2::FileExt::try_lock_exclusive(&gate).is_err() {
            drop(gate);
            let _ = child.wait_with_output().unwrap();
            project.ok(&["down"]);
            continue;
        }
        let manifest = loop {
            if let Ok(m) = ws.target("default", None)
                && m.process.is_some()
            {
                break m;
            }
            if Instant::now() >= deadline {
                drop(gate);
                let _ = child.kill();
                let _ = child.wait();
                panic!("Daemon did not publish its process identity");
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        if manifest.state == ronchi::lab::Lifecycle::Ready {
            drop(gate);
            let _ = child.wait_with_output().unwrap();
            project.ok(&["down"]);
            continue;
        }
        // SAFETY: the CLI child remains unreaped and belongs exclusively to this test.
        assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                drop(gate);
                let _ = child.kill();
                let _ = child.wait();
                panic!("Startup cancellation did not settle");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        drop(gate);
        error(output, 130, "cancelled");
        assert!(
            ronchi::process::identity(manifest.process.unwrap().pid)
                .unwrap()
                .is_none()
        );
        let result = project.ok(&["status"]);
        assert_eq!(result["result"]["journal_state"], "interrupted");
        assert!(result["result"]["rpc_address"].is_null());
        return;
    }
    panic!("Could not exercise cancellation before readiness publication");
}

#[test]
fn explicit_ids_never_stop_history_and_configured_changes_require_restart() {
    let project = Project::new();
    let first = project.up();
    let id = first["lab_id"].as_str().unwrap();
    project.ok(&["down", "--lab-id", id]);
    error(project.run(&["down", "--lab-id", id]), 5, "state_conflict");
    let current = project.up();
    assert_ne!(current["lab_id"], id);
    error(project.run(&["down", "--lab-id", id]), 5, "state_conflict");
    assert_eq!(project.ok(&["status"])["result"]["state"], "ready");
    std::fs::write(
        project.path().join("ronchi/config.toml"),
        "schema_version = 1\nrpc_listen = '127.0.0.1:8898'\n",
    )
    .unwrap();
    error(project.run(&["up"]), 5, "state_conflict");
    project.ok(&["down"]);
}

#[test]
fn replacing_the_binary_file_does_not_change_the_live_process_birth_identity() {
    let project = Project::new();
    let binary = TempDir::new().unwrap();
    let original = binary.path().join("ronchi");
    std::fs::copy(env!("CARGO_BIN_EXE_ronchi"), &original).unwrap();
    let output = Command::new(&original)
        .arg("--project")
        .arg(project.path())
        .args([
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let before = project.manifest().process.unwrap();
    let replacement = binary.path().join("replacement");
    std::fs::copy(env!("CARGO_BIN_EXE_ronchi"), &replacement).unwrap();
    std::fs::rename(replacement, &original).unwrap();
    assert!(ronchi::process::matches(&before).unwrap());
    assert_eq!(project.ok(&["status"])["result"]["state"], "ready");
    project.ok(&["down"]);
}

#[test]
fn unknown_recorded_children_block_name_reuse_without_signals_or_history_deletion() {
    let project = Project::new();
    project.up();
    project.ok(&["down"]);
    let mut unrelated = Command::new("/bin/sleep").arg("20").spawn().unwrap();
    let mut manifest = project.manifest();
    let identity = ronchi::process::identity(unrelated.id()).unwrap().unwrap();
    manifest.owned_children.push(identity.clone());
    let ws = Workspace::new(project.path());
    let dir = ws.lab(&manifest.lab_id).unwrap();
    ronchi::workspace::atomic_json(&dir.join("manifest.json"), &manifest).unwrap();
    let output = project.run(&["up"]);
    let still_running = unrelated.try_wait().unwrap().is_none();
    let actual = ronchi::process::identity(identity.pid).unwrap();
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
    error(output, 5, "state_conflict");
    assert!(still_running);
    assert_eq!(actual, Some(identity));
    assert_eq!(std::fs::read_dir(ws.root.join("labs")).unwrap().count(), 1);
    assert_eq!(
        JournalReader::open(&dir.join("journal.sqlite"))
            .unwrap()
            .snapshot(false)
            .unwrap()
            .state,
        JournalState::Finalized
    );
}

#[test]
fn native_startup_retains_explicit_protocol_fixture_and_serves_nonce_fee_quote() {
    let project = Project::new();
    let key = |n| bs58::encode([n; 32]).into_string();
    let account = key(71);
    let authority = key(72);
    let nonce = key(73);
    std::fs::create_dir_all(project.path().join("ronchi")).unwrap();
    std::fs::write(project.path().join("ronchi/config.toml"),format!("schema_version = 1\n[[simulator_state.nonce_accounts]]\naddress = '{account}'\nauthority = '{authority}'\nnonce = '{nonce}'\n")).unwrap();
    let ready = project.up();
    assert_eq!(ready["result"]["state"], "ready");
    let manifest = project.manifest();
    let rpc = manifest.rpc_address.unwrap();
    let mut message = vec![1, 0, 2, 4];
    message.extend_from_slice(&[72; 32]);
    message.extend_from_slice(&[71; 32]);
    message.extend(
        bs58::decode("SysvarRecentB1ockHashes11111111111111111111")
            .into_vec()
            .unwrap(),
    );
    message.extend_from_slice(&[0; 32]);
    message.extend_from_slice(&[73; 32]);
    message.extend_from_slice(&[1, 3, 3, 1, 2, 0, 4]);
    message.extend_from_slice(&4u32.to_le_bytes());
    use base64::{Engine, engine::general_purpose::STANDARD};
    let body=serde_json::json!({"jsonrpc":"2.0","id":11,"method":"getFeeForMessage","params":[STANDARD.encode(message)]}).to_string();
    let response = raw(
        rpc,
        format!(
            "POST / HTTP/1.1\r\nHost: {rpc}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    );
    let value: Value = serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(value["result"]["value"], 5000);
    project.ok(&["down"]);
    let path = Workspace::new(project.path())
        .lab(&manifest.lab_id)
        .unwrap()
        .join("journal.sqlite");
    let reader = JournalReader::open(&path).unwrap();
    assert_eq!(reader.nonce(&account, None).unwrap().unwrap().nonce, nonce);
}
