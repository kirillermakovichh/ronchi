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
            "--detach",
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

fn foreground(project: &Project) -> (std::process::Child, Value) {
    use std::os::fd::AsRawFd;
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
    let stdout = child.stdout.as_mut().unwrap();
    let mut ready = Vec::new();
    while !ready.contains(&b'\n') {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("readiness was not flushed while up was running");
        }
        let mut poll = libc::pollfd {
            fd: stdout.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll receives one valid descriptor record.
        if unsafe { libc::poll(&mut poll, 1, 100) } > 0 {
            let mut bytes = [0u8; 8192];
            let n = stdout.read(&mut bytes).unwrap();
            assert!(n > 0, "CLI closed stdout before readiness: {ready:?}");
            ready.extend_from_slice(&bytes[..n]);
            assert!(ready.len() < 128 * 1024);
        }
    }
    let ready = serde_json::from_slice(&ready).unwrap();
    (child, ready)
}
fn finish_foreground(mut child: std::process::Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(35);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("foreground shutdown timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().unwrap()
}
#[test]
fn foreground_up_serves_until_ctrl_c_and_finalizes_the_journal() {
    let project = Project::new();
    let (mut child, ready) = foreground(&project);
    assert_eq!(ready["result"]["state"], "ready");
    assert!(
        child.try_wait().unwrap().is_none(),
        "up returned instead of owning the terminal"
    );
    assert_eq!(project.ok(&["status"])["result"]["state"], "ready");
    let manifest = project.manifest();
    let rpc = manifest.rpc_address.unwrap();
    assert!(raw(rpc, format!("POST / HTTP/1.1\r\nHost: {rpc}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}" )).contains("-32600"));
    // SAFETY: child is the unreaped CLI process owned by this test.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
    let output = finish_foreground(child);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "normal stop emitted extra JSON: {output:?}"
    );
    assert_eq!(ready["lab_id"], manifest.lab_id);
    assert_eq!(
        project.ok(&["status"])["result"]["journal_state"],
        "finalized"
    );
    assert!(TcpStream::connect(rpc).is_err());
}

#[test]
fn foreground_exits_after_external_down_and_propagates_daemon_failure() {
    let project = Project::new();
    let (child, _) = foreground(&project);
    project.ok(&["down"]);
    assert!(finish_foreground(child).status.success());
    let (child, _) = foreground(&project);
    let manifest = project.manifest();
    // SAFETY: the manifest authenticates the daemon owned by this isolated test Lab.
    assert_eq!(
        unsafe { libc::kill(manifest.process.unwrap().pid as i32, libc::SIGKILL) },
        0
    );
    error(finish_foreground(child), 6, "internal_error");
    assert_eq!(
        project.ok(&["status"])["result"]["journal_state"],
        "interrupted"
    );
}

#[test]
fn foreground_does_not_take_ownership_of_an_existing_lab() {
    let project = Project::new();
    let existing = project.up();
    error(project.run(&["up"]), 5, "state_conflict");
    assert_eq!(project.ok(&["status"])["lab_id"], existing["lab_id"]);
    assert_eq!(project.ok(&["status"])["result"]["state"], "ready");
    project.ok(&["down"]);
}

#[test]
fn foreground_terminal_signals_and_owner_death_stop_only_the_owned_lab() {
    let project = Project::new();
    let independent = project.ok(&[
        "up",
        "--detach",
        "--lab",
        "independent",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    for sig in [libc::SIGTERM, libc::SIGHUP, libc::SIGKILL] {
        let (mut child, _) = foreground(&project);
        // SAFETY: child is an unreaped process exclusively owned by this test.
        assert_eq!(unsafe { libc::kill(child.id() as i32, sig) }, 0);
        if sig == libc::SIGKILL {
            child.wait().unwrap();
        } else {
            assert!(finish_foreground(child).status.success());
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let state = project.ok(&["status"]);
            if state["result"]["state"] == "stopped" {
                assert_eq!(state["result"]["journal_state"], "finalized");
                break;
            }
            assert!(Instant::now() < deadline, "owner death left a live Lab");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            project.ok(&["status", "--lab", "independent"])["lab_id"],
            independent["lab_id"]
        );
    }
    project.ok(&["down", "--lab", "independent"]);
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
        "--detach",
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
    assert_eq!(project.ok(&["up", "--detach"])["lab_id"], a["lab_id"]);
    error(
        project.run(&["up", "--detach", "--rpc-listen", "127.0.0.1:8898"]),
        5,
        "state_conflict",
    );
    let other = project.ok(&[
        "up",
        "--detach",
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
            "--detach",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ];
        if flag == "--rpc-listen" {
            args[3] = &address;
        } else {
            args[5] = &address;
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
    error(project.run(&["up", "--detach"]), 5, "state_conflict");
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
fn real_terminal_ctrl_c_and_closure_respect_foreground_and_detach_ownership() {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    };
    for (detached, hangup) in [(true, true), (false, false), (false, true)] {
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
        // Do not leak the PTY master into the CLI: that would prevent a real hangup.
        for descriptor in [master.as_raw_fd(), fd] {
            // SAFETY: these are owned, valid PTY descriptors.
            assert_eq!(
                unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) },
                0
            );
        }
        let mut command = project.command(&[
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ]);
        if detached {
            command.arg("--detach");
        }
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
        if detached {
            assert!(child.wait().unwrap().success());
            drop(master);
            assert_eq!(project.ok(&["status"])["result"]["state"], "ready");
            project.ok(&["down"]);
        } else {
            assert!(child.try_wait().unwrap().is_none());
            if hangup {
                drop(master);
            } else {
                master.write_all(&[3]).unwrap();
            }
            assert!(finish_foreground(child).status.success());
            assert_eq!(
                project.ok(&["status"])["result"]["journal_state"],
                "finalized"
            );
        }
    }
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
                "--detach",
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
    error(project.run(&["up", "--detach"]), 5, "state_conflict");
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
            "--detach",
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
    let output = project.run(&["up", "--detach"]);
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

#[test]
fn public_case_operations_preserve_lab_identity_and_reject_invalid_replacement() {
    let project = Project::new();
    let catalog = project.ok(&["case", "list"]);
    assert_eq!(catalog["result"][0]["case_id"], "tx-response-lost");
    let ready = project.ok(&[
        "up",
        "--detach",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
        "--case",
        "tx-response-lost",
    ]);
    let id = ready["lab_id"].clone();
    let process = project.manifest().process;
    assert_eq!(
        ready["result"]["active_case"]["case_id"],
        "tx-response-lost"
    );
    let first = ready["result"]["active_case"]["revision"].as_u64().unwrap();
    let disarmed = project.ok(&["case", "disarm"]);
    assert!(disarmed["result"]["active_case"].is_null());
    let armed = project.ok(&["case", "arm", "tx-response-lost", "--hold-ms", "100"]);
    assert!(armed["result"]["active_case"]["revision"].as_u64().unwrap() > first);
    error(
        project.run(&["case", "arm", "unsupported"]),
        4,
        "unsupported_capability",
    );
    let status = project.ok(&["status"]);
    assert_eq!(status["lab_id"], id);
    assert_eq!(
        status["result"]["active_case"],
        armed["result"]["active_case"]
    );
    assert_eq!(project.manifest().process, process);
    let manifest = project.manifest();
    let address = manifest.control_address.unwrap();
    let token = std::fs::read_to_string(
        Workspace::new(project.path())
            .lab(&manifest.lab_id)
            .unwrap()
            .join("token"),
    )
    .unwrap();
    let body = r#"{"case_id":"tx-response-lost","unknown":true}"#;
    let response = raw(
        address,
        format!(
            "POST /v1/labs/{}/case/arm HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            manifest.lab_id,
            body.len()
        ),
    );
    assert!(response.starts_with("HTTP/1.1 400"), "{response}");
    let value: Value = serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(value["error"]["code"], "invalid_argument");
    assert_eq!(value["error"]["phase"], "case");
    assert_eq!(value["error"]["retryable"], false);
    assert!(value["error"]["recovery"].is_string());
    project.ok(&["down"]);
    assert_eq!(
        project.ok(&["status"])["result"]["active_case"],
        armed["result"]["active_case"]
    );
    error(project.run(&["case", "disarm"]), 5, "state_conflict");
}

#[test]
fn checkpoint_b_native_http_loses_response_then_tracks_original_signature_without_restart() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ed25519_dalek::{Signer, SigningKey};
    fn rpc(address: SocketAddr, value: Value) -> Value {
        let body = value.to_string();
        let response = raw(
            address,
            format!(
                "POST / HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        );
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
    }
    let project = Project::new();
    let ready = project.up();
    let id = ready["lab_id"].clone();
    let manifest = project.manifest();
    let identity = manifest.process.clone();
    let address = manifest.rpc_address.unwrap();
    project.ok(&["case", "arm", "tx-response-lost", "--hold-ms", "180"]);
    let hash = rpc(
        address,
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"getLatestBlockhash"}),
    )["result"]["value"]["blockhash"]
        .as_str()
        .unwrap()
        .to_string();
    let signer = SigningKey::from_bytes(&[57; 32]);
    let mut message = vec![1, 0, 0, 1];
    message.extend_from_slice(&signer.verifying_key().to_bytes());
    message.extend(bs58::decode(hash).into_vec().unwrap());
    message.push(0);
    let signature = signer.sign(&message).to_bytes();
    let expected = bs58::encode(signature).into_string();
    let mut wire = vec![1];
    wire.extend_from_slice(&signature);
    wire.extend(message);
    let body=serde_json::json!({"jsonrpc":"2.0","id":2,"method":"sendTransaction","params":[STANDARD.encode(&wire),{"encoding":"base64","skipPreflight":true}]}).to_string();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .write_all(
            format!(
                "POST / HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .unwrap();
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).unwrap();
    assert!(reply.is_empty());
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let response = rpc(
            address,
            serde_json::json!({"jsonrpc":"2.0","id":3,"method":"getSignatureStatuses","params":[[expected]]}),
        );
        if response["result"]["value"][0]["confirmationStatus"] == "processed" {
            break;
        }
        assert!(Instant::now() < deadline, "{response}");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(project.ok(&["status"])["lab_id"], id);
    assert_eq!(project.manifest().process, identity);
    project.ok(&["down", "--force"]);
    let path = Workspace::new(project.path())
        .lab(&manifest.lab_id)
        .unwrap()
        .join("journal.sqlite");
    let reader = JournalReader::open(&path).unwrap();
    assert_eq!(
        reader.snapshot(false).unwrap().state,
        JournalState::Interrupted
    );
    assert!(reader.response_loss(&expected).unwrap().is_some());
    assert_eq!(
        reader.transaction(&expected).unwrap().unwrap().wire_base64,
        STANDARD.encode(wire)
    );
    let facts: Vec<_> = (1..=reader.snapshot(false).unwrap().snapshot_seq)
        .map(|seq| reader.event(seq).unwrap().unwrap())
        .collect();
    for kind in [
        "transaction.accepted",
        "fault.response_loss_selected",
        "fault.withholding_started",
    ] {
        assert!(facts.iter().any(|event| event.kind == kind));
    }
    assert!(facts.iter().any(|event| event.kind == "rpc.boundary"
        && event.data["transport"] == "withholding_deadline"
        && event.data["client_receipt"] == "unknown"));
}

#[test]
fn native_normal_and_force_stop_with_live_hold_finalize_or_interrupt_after_owned_evidence() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ed25519_dalek::{Signer, SigningKey};
    for force in [false, true] {
        let project = Project::new();
        let owner = if force {
            project.up();
            None
        } else {
            Some(foreground(&project).0)
        };
        project.ok(&[
            "case",
            "arm",
            "tx-response-lost",
            "--hold-ms",
            if force { "5000" } else { "1000" },
        ]);
        let manifest = project.manifest();
        let address = manifest.rpc_address.unwrap();
        let body =
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"getLatestBlockhash"}).to_string();
        let response = raw(
            address,
            format!(
                "POST / HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        );
        let value: Value =
            serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
        let signer = SigningKey::from_bytes(&[58; 32]);
        let mut message = vec![1, 0, 0, 1];
        message.extend_from_slice(&signer.verifying_key().to_bytes());
        message.extend(
            bs58::decode(value["result"]["value"]["blockhash"].as_str().unwrap())
                .into_vec()
                .unwrap(),
        );
        message.push(0);
        let signature = signer.sign(&message).to_bytes();
        let expected = bs58::encode(signature).into_string();
        let mut wire = vec![1];
        wire.extend_from_slice(&signature);
        wire.extend(message);
        let body=serde_json::json!({"jsonrpc":"2.0","id":2,"method":"sendTransaction","params":[STANDARD.encode(wire),{"encoding":"base64","skipPreflight":true}]}).to_string();
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .write_all(
                format!(
                    "POST / HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .unwrap();
        let path = Workspace::new(project.path())
            .lab(&manifest.lab_id)
            .unwrap()
            .join("journal.sqlite");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let reader = JournalReader::open(&path).unwrap();
            if reader.response_loss(&expected).unwrap().is_some() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            !project.ok(&["status"])["result"]["held_effects"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        if let Some(child) = owner {
            // SAFETY: child is the unreaped foreground CLI owned by this test.
            assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
            assert!(finish_foreground(child).status.success());
        } else {
            project.ok(&["down", "--force"]);
        }
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        assert!(response.is_empty());
        let reader = JournalReader::open(&path).unwrap();
        assert_eq!(
            reader.snapshot(false).unwrap().state,
            if force {
                JournalState::Interrupted
            } else {
                JournalState::Finalized
            }
        );
        assert!(reader.transaction(&expected).unwrap().is_some());
        assert!(reader.response_loss(&expected).unwrap().is_some());
        if !force {
            assert_eq!(
                reader.transaction(&expected).unwrap().unwrap().stage,
                ronchi::journal::TransactionPhase::Finalized
            );
        }
        assert!(
            (1..=reader.snapshot(false).unwrap().snapshot_seq)
                .filter_map(|seq| reader.event(seq).unwrap())
                .any(|event| event.kind == "rpc.boundary"
                    && event.data["transport"]
                        == if force {
                            "cancelled"
                        } else {
                            "withholding_deadline"
                        })
        );
    }
}

#[test]
fn investigation_cli_queries_live_and_retained_identity_without_new_capture() {
    let p = Project::new();
    let up = p.up();
    let id = up["lab_id"].as_str().unwrap();
    let first = p.ok(&[
        "investigate",
        "timeline",
        "--fields",
        "kind,source",
        "--limit",
        "2",
    ]);
    assert_eq!(first["lab_id"], id);
    assert_eq!(first["selection_basis"], "current");
    assert!(!first["result"]["items"].as_array().unwrap().is_empty());
    let stopped = p.ok(&["down"]);
    let historical = p.ok(&[
        "investigate",
        "evidence",
        "--lab-id",
        id,
        "--event-seq",
        "1",
    ]);
    assert_eq!(historical["selection_basis"], "explicit");
    assert_eq!(
        historical["result"]["items"][0]["event_ref"],
        serde_json::json!({"lab_id":id,"event_seq":1})
    );
    assert_eq!(
        historical["view"]["snapshot_seq"],
        stopped["result"]["snapshot_seq"]
    );
    let latest = p.ok(&["investigate", "summary"]);
    assert_eq!(latest["selection_basis"], "latest");
    assert_eq!(latest["result"]["analysis"]["findings"], "performed");
    p.up();
    let retained = p.ok(&[
        "investigate",
        "evidence",
        "--lab-id",
        id,
        "--event-seq",
        "1",
    ]);
    assert_eq!(retained["result"], historical["result"]);
    p.ok(&["down", "--force"]);
    let interrupted = p.ok(&["investigate", "summary"]);
    assert_eq!(interrupted["view"]["journal_state"], "interrupted");
    assert!(
        interrupted["view"]["evidence_gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["code"] == "journal_interrupted")
    );
}

#[test]
fn investigation_control_auth_and_scoped_cli_result_agree() {
    let p = Project::new();
    p.up();
    let manifest = p.manifest();
    let address = manifest.control_address.unwrap();
    let id = &manifest.lab_id;
    let token =
        std::fs::read_to_string(Workspace::new(p.path()).lab(id).unwrap().join("token")).unwrap();
    let body=serde_json::json!({"contract_version":1,"request_id":"http-query","lab_id":id,"operation":"investigate.timeline","arguments":{"scope":{"kind":["lab.starting"]},"fields":["kind","source"],"limit":50}}).to_string();
    let request = |credential: &str, body: &str| {
        format!(
            "POST /v1/labs/{id}/investigate HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {credential}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    };
    assert!(raw(address, request("wrong", &body)).starts_with("HTTP/1.1 401"));
    let http = raw(address, request(token.trim(), &body));
    assert!(http.starts_with("HTTP/1.1 200"), "{http}");
    let response: Value = serde_json::from_str(http.split_once("\r\n\r\n").unwrap().1).unwrap();
    let cli = p.ok(&[
        "investigate",
        "timeline",
        "--kind",
        "lab.starting",
        "--fields",
        "kind,source",
    ]);
    assert_eq!(cli["result"], response["result"]);
    assert_eq!(cli["view"], response["view"]);
    assert_eq!(response["request_id"], "http-query");
    let malformed = raw(address, request(token.trim(), "{"));
    assert!(malformed.contains("invalid_argument"));
    let exact_body = format!("{}{}", body, " ".repeat(65536 - body.len()));
    let exact = raw(address, request(token.trim(), &exact_body));
    assert!(exact.starts_with("HTTP/1.1 200"), "{exact}");
    let oversized = raw(address, request(token.trim(), &format!("{exact_body} ")));
    assert!(oversized.contains("invalid_argument"));
    let schema = p.ok(&["schema", "investigate.evidence"]);
    assert_eq!(schema["implemented"], true);
    assert!(schema["request"].to_string().contains("include_payload"));
}

#[test]
fn investigation_ctrl_c_and_broken_pipe_release_readers_while_rpc_keeps_working() {
    let p = Project::new();
    p.up();
    let manifest = p.manifest();
    let path = Workspace::new(p.path())
        .lab(&manifest.lab_id)
        .unwrap()
        .join("journal.sqlite");
    let connection = rusqlite::Connection::open(path).unwrap();
    connection.execute_batch("ALTER TABLE query_events RENAME TO retained_navigation; CREATE VIEW query_events AS WITH RECURSIVE work(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM work WHERE x<100000000) SELECT retained_navigation.* FROM retained_navigation CROSS JOIN work;").unwrap();
    // Checkpoints below must report retained readers without waiting for them.
    connection.busy_timeout(Duration::ZERO).unwrap();
    let address = manifest.rpc_address.unwrap();
    let health = || {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"getHealth"}"#;
        let reply = raw(
            address,
            format!(
                "POST / HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        );
        assert!(reply.contains("\"result\":\"ok\""), "{reply}");
    };
    for mode in ["signal", "pipe", "deadline"] {
        let broken_pipe = mode == "pipe";
        let mut child = p
            .command(&["investigate", "summary"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            health(); // A real RPC appends evidence while the query runs.
            let (_, frames, done): (i64, i64, i64) = connection
                .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .unwrap();
            if done < frames {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "query never acquired a page snapshot"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        if broken_pipe {
            drop(child.stdout.take());
        } else if mode == "signal" {
            assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
        }
        let deadline =
            Instant::now() + Duration::from_secs(if mode == "deadline" { 12 } else { 2 });
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "query did not stop after cancellation (broken_pipe={broken_pipe}): {output:?}"
                );
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(if broken_pipe {
                0
            } else if mode == "deadline" {
                124
            } else {
                130
            }),
            "{output:?}"
        );
        if !broken_pipe {
            let error: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                error["error"]["code"],
                if mode == "deadline" {
                    "deadline_exceeded"
                } else {
                    "cancelled"
                }
            );
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let (busy, _, _): (i64, i64, i64) = connection
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .unwrap();
            if busy == 0 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "cancelled query retained a reader snapshot"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        health();
        assert_eq!(p.ok(&["status"])["lab_id"], manifest.lab_id);
    }
    // Schema restoration is a write, unlike the nonblocking checkpoint probes.
    connection.busy_timeout(Duration::from_secs(5)).unwrap();
    connection
        .execute_batch(
            "DROP VIEW query_events; ALTER TABLE retained_navigation RENAME TO query_events;",
        )
        .unwrap();
    p.ok(&["investigate", "summary"]);
}

#[test]
fn investigation_text_escapes_untrusted_terminal_controls() {
    let p = Project::new();
    p.up();
    let out = Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(p.path())
        .args([
            "investigate",
            "timeline",
            "--signature",
            "\u{1b}[31m\u{85}\u{202e}",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains(['\u{1b}', '\u{85}', '\u{202e}']));
    assert!(text.contains("\\u001b"));
    assert!(text.contains("\\u{202e}"));
    assert!(out.stderr.is_empty());
}

#[test]
fn findings_native_http_supports_both_rules_and_retained_snapshot_links() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ed25519_dalek::{Signer, SigningKey};
    fn rpc(address: SocketAddr, value: &Value) -> Value {
        let body = value.to_string();
        let response = raw(
            address,
            format!(
                "POST / HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        );
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
    }
    for force in [false, true] {
        let p = Project::new();
        p.up();
        let manifest = p.manifest();
        let id = &manifest.lab_id;
        let rpc_address = manifest.rpc_address.unwrap();
        let control = manifest.control_address.unwrap();
        p.ok(&["case", "arm", "tx-response-lost", "--hold-ms", "30"]);
        let hash = rpc(
            rpc_address,
            &serde_json::json!({"jsonrpc":"2.0","id":7,"method":"getLatestBlockhash"}),
        )["result"]["value"]["blockhash"]
            .as_str()
            .unwrap()
            .to_string();
        let signer = SigningKey::from_bytes(&[92; 32]);
        let mut message = vec![1, 0, 0, 1];
        message.extend(signer.verifying_key().to_bytes());
        message.extend(bs58::decode(hash).into_vec().unwrap());
        message.push(0);
        let signature = signer.sign(&message).to_bytes();
        let expected = bs58::encode(signature).into_string();
        let mut wire = vec![1];
        wire.extend(signature);
        wire.extend(message);
        let submission = serde_json::json!({"jsonrpc":"2.0","id":7,"method":"sendTransaction","params":[STANDARD.encode(wire),{"encoding":"base64","skipPreflight":true}]});
        let body = submission.to_string();
        assert!(raw(rpc_address,format!("POST / HTTP/1.1\r\nHost: {rpc_address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())).is_empty());
        for _ in 0..3 {
            rpc(
                rpc_address,
                &serde_json::json!({"jsonrpc":"2.0","id":7,"method":"getSignatureStatuses","params":[[expected]]}),
            );
        }
        rpc(
            rpc_address,
            &serde_json::json!({"jsonrpc":"2.0","id":7,"method":"getTransaction","params":[expected]}),
        );
        assert_eq!(rpc(rpc_address, &submission)["result"], expected);
        let findings = p.ok(&["investigate", "findings", "--signature", &expected]);
        assert_eq!(
            findings["result"]["items"].as_array().unwrap().len(),
            2,
            "{findings}"
        );
        let snapshot = findings["view"]["snapshot_seq"]
            .as_u64()
            .unwrap()
            .to_string();
        let token =
            std::fs::read_to_string(Workspace::new(p.path()).lab(id).unwrap().join("token"))
                .unwrap();
        let http_query = |operation: &str, arguments: Value| {
            let body=serde_json::json!({"contract_version":1,"request_id":"findings-http","lab_id":id,"operation":operation,"arguments":arguments}).to_string();
            let response = raw(
                control,
                format!(
                    "POST /v1/labs/{id}/investigate HTTP/1.1\r\nHost: {control}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    token.trim(),
                    body.len()
                ),
            );
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
            serde_json::from_str::<Value>(response.split_once("\r\n\r\n").unwrap().1).unwrap()
        };
        let http = http_query(
            "investigate.findings",
            serde_json::json!({"scope":{"signature":[expected]},"snapshot_seq":snapshot.parse::<u64>().unwrap()}),
        );
        assert_eq!(http["result"], findings["result"]);
        for finding in findings["result"]["items"].as_array().unwrap() {
            let evidence = http_query(
                "investigate.evidence",
                finding["evidence_arguments"].clone(),
            );
            let cli = p.ok(&[
                "investigate",
                "evidence",
                "--signature",
                &expected,
                "--finding-id",
                finding["finding_id"].as_str().unwrap(),
                "--snapshot-seq",
                &snapshot,
            ]);
            assert_eq!(evidence["result"], cli["result"]);
            assert_eq!(evidence["result"]["items"].as_array().unwrap().len(), 5);
            for item in evidence["result"]["items"].as_array().unwrap() {
                assert_eq!(item["event_ref"]["lab_id"], *id);
                assert!(item.get("payload").is_none());
            }
            assert!(
                finding["limitations"]
                    .to_string()
                    .contains("client timeout")
            );
            assert!(!finding["statement"].as_str().unwrap().contains("timeout"));
        }
        let schema = p.ok(&["schema", "investigate.findings"]);
        assert!(
            schema["finding_schema"]
                .to_string()
                .contains("supporting_events")
        );
        assert_eq!(p.manifest().process, manifest.process);
        p.ok(if force {
            &["down", "--force"]
        } else {
            &["down"]
        });
        let journal = Workspace::new(p.path())
            .lab(id)
            .unwrap()
            .join("journal.sqlite");
        let before = std::fs::read(&journal).unwrap();
        let retained = p.ok(&[
            "investigate",
            "findings",
            "--lab-id",
            id,
            "--signature",
            &expected,
            "--snapshot-seq",
            &snapshot,
        ]);
        assert_eq!(retained["result"], findings["result"]);
        assert_eq!(
            retained["view"]["journal_state"],
            if force { "interrupted" } else { "finalized" }
        );
        let first = &findings["result"]["items"][0];
        let evidence = p.ok(&[
            "investigate",
            "evidence",
            "--lab-id",
            id,
            "--signature",
            &expected,
            "--finding-id",
            first["finding_id"].as_str().unwrap(),
            "--snapshot-seq",
            &snapshot,
        ]);
        assert_eq!(evidence["result"]["items"].as_array().unwrap().len(), 5);
        assert_eq!(std::fs::read(&journal).unwrap(), before);
    }
}
