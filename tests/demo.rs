mod reference_support;
use reference_support::{base, command};
use serde_json::Value;
#[test]
fn resilient_demo_collects_public_proof_and_retains_only_its_lab() {
    let project = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .current_dir(project.path())
        .args(["--json", "demo", "tx-response-lost"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let demo: Value = serde_json::from_slice(&output.stdout).unwrap();
    let result = &demo["result"];
    assert_eq!(result["check"]["verdict"], "pass", "{demo}");
    assert_eq!(result["check"]["definition_version"], 2);
    assert_eq!(result["reference_stopped"], true);
    assert_eq!(result["lab_name"], "demo");
    assert_eq!(result["lab_state"], "ready");
    assert!(
        ronchi::process::identity(result["reference_pid"].as_u64().unwrap() as u32)
            .unwrap()
            .is_none()
    );
    use base64::Engine as _;
    let seed = std::fs::read(
        project
            .path()
            .join("ronchi/reference")
            .join(result["reference_name"].as_str().unwrap())
            .join("signing.seed"),
    )
    .unwrap();
    let token = std::fs::read_to_string(
        project
            .path()
            .join("ronchi/labs")
            .join(result["lab_id"].as_str().unwrap())
            .join("token"),
    )
    .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for secret in [
        token,
        seed.iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        base64::engine::general_purpose::STANDARD.encode(&seed),
        bs58::encode(&seed).into_string(),
    ] {
        assert!(
            !combined.contains(&secret),
            "Demo output contained private material"
        );
    }
    let id = result["lab_id"].as_str().unwrap();
    let snapshot = result["snapshot_seq"].as_u64().unwrap().to_string();
    let intent = result["intent_id"].as_str().unwrap();
    let replay = command(
        project.path(),
        &[
            "--lab-id",
            id,
            "check",
            "evaluate",
            "reference-intent",
            "--definition-version",
            "2",
            "--snapshot-seq",
            &snapshot,
            "--intent-id",
            intent,
        ],
    );
    assert_eq!(replay["result"], result["check"]);
    assert!(!project.path().join("ronchi/config.toml").exists());
    command(project.path(), &["--lab-id", id, "down"]);
    let stopped = command(
        project.path(),
        &[
            "--lab-id",
            id,
            "investigate",
            "summary",
            "--snapshot-seq",
            &snapshot,
        ],
    );
    assert!(!stopped["result"]["findings"].as_array().unwrap().is_empty());
    let output = base(project.path())
        .args([
            "demo",
            "tx-response-lost",
            "--profile",
            "unsafe",
            "--stop-after-demo",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let unsafe_demo: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        unsafe_demo["result"]["check"]["verdict"], "fail",
        "{unsafe_demo}"
    );
    assert_eq!(unsafe_demo["result"]["lab_name"], "demo-2");
    assert_eq!(unsafe_demo["result"]["lab_state"], "stopped");
    assert_ne!(
        unsafe_demo["result"]["reference_name"],
        result["reference_name"]
    );
}

fn stage(project: &std::path::Path, phase: &str) -> (std::path::PathBuf, Value) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
    loop {
        if let Ok(entries) = std::fs::read_dir(project.join("ronchi/demos")) {
            for entry in entries.flatten() {
                let dir = entry.path();
                if let Ok(bytes) = std::fs::read(dir.join("state.json"))
                    && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                    && value["phase"] == phase
                {
                    return (dir, value);
                }
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Demo did not reach {phase}"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn wait(mut child: std::process::Child) -> std::process::Output {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(18);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Demo did not settle");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}
#[test]
fn startup_failures_never_submit_an_intent_and_report_owned_identity() {
    use std::{net::TcpListener, process::Stdio};
    let project = tempfile::tempdir().unwrap();
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let output = base(project.path())
        .env(
            "RONCHI_DEMO_TEST_RPC_PORT",
            occupied.local_addr().unwrap().port().to_string(),
        )
        .args(["demo", "tx-response-lost", "--stop-after-demo"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let failure: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(failure["error"]["phase"], "startup");
    assert!(failure["error"]["lab_id"].is_string());
    assert!(failure["error"]["intent_id"].is_null());
    let child = base(project.path())
        .env("RONCHI_DEMO_TEST_BARRIER", "source_released")
        .args(["demo", "tx-response-lost", "--stop-after-demo"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (directory, metadata) = stage(project.path(), "source_released");
    let socket = TcpListener::bind(metadata["source_address"].as_str().unwrap()).unwrap();
    std::fs::write(directory.join("release"), []).unwrap();
    let output = wait(child);
    assert!(!output.status.success());
    let failure: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(failure["error"]["phase"], "reference_readiness");
    assert!(failure["error"]["intent_id"].is_null());
    assert_eq!(failure["error"]["reference_stopped"], true);
    assert_eq!(failure["error"]["lab_stopped"], true);
    drop(socket);
}
#[test]
fn cancellation_and_child_death_preserve_exact_owned_resources() {
    use std::process::Stdio;
    for mode in ["signal", "pipe", "death", "retain"] {
        let project = tempfile::tempdir().unwrap();
        let mut launch = base(project.path());
        launch
            .env("RONCHI_DEMO_TEST_BARRIER", "intent_created")
            .args(["demo", "tx-response-lost"]);
        if mode != "retain" {
            launch.arg("--stop-after-demo");
        }
        let mut child = launch
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (directory, metadata) = stage(project.path(), "intent_created");
        let reference_pid = metadata["reference_pid"].as_u64().unwrap() as u32;
        if mode == "death" {
            assert_eq!(
                unsafe { libc::kill(reference_pid as i32, libc::SIGKILL) },
                0
            );
            std::fs::write(directory.join("release"), []).unwrap();
        } else if matches!(mode, "signal" | "retain") {
            assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
        } else {
            drop(child.stdout.take());
        }
        let output = wait(child);
        assert_eq!(
            output.status.code(),
            Some(match mode {
                "signal" | "retain" => 130,
                "pipe" => 0,
                _ => 6,
            }),
            "{output:?}"
        );
        if mode != "pipe" {
            let failure: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(failure["error"]["lab_id"], metadata["lab_id"]);
            assert_eq!(failure["error"]["intent_id"], metadata["intent_id"]);
            assert_eq!(failure["error"]["reference_stopped"], true);
            assert_eq!(failure["error"]["lab_stopped"], mode != "retain");
        }
        assert!(ronchi::process::identity(reference_pid).unwrap().is_none());
        let status = command(
            project.path(),
            &["--lab-id", metadata["lab_id"].as_str().unwrap(), "status"],
        );
        if mode == "retain" {
            assert_eq!(status["result"]["state"], "ready");
            command(
                project.path(),
                &["--lab-id", metadata["lab_id"].as_str().unwrap(), "down"],
            );
        } else {
            assert!(matches!(
                status["result"]["state"].as_str(),
                Some("stopped" | "interrupted")
            ));
        }
    }
}

#[test]
fn source_timeout_remains_an_observation_gap_and_cleanup_is_bounded() {
    use std::process::Stdio;
    let project = tempfile::tempdir().unwrap();
    let child = base(project.path())
        .env("RONCHI_DEMO_TEST_BARRIER", "intent_created")
        .args(["demo", "tx-response-lost", "--stop-after-demo"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (directory, metadata) = stage(project.path(), "intent_created");
    let pid = metadata["reference_pid"].as_u64().unwrap() as u32;
    assert!(ronchi::process::identity(pid).unwrap().is_some());
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGSTOP) }, 0);
    std::fs::write(directory.join("release"), []).unwrap();
    let output = wait(child);
    assert!(!output.status.success());
    let failure: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(failure["error"]["code"], "source_failure", "{failure}");
    assert!(failure.get("result").is_none());
    assert_eq!(failure["error"]["reference_stopped"], true);
    assert_eq!(failure["error"]["lab_stopped"], true);
    assert!(ronchi::process::identity(pid).unwrap().is_none());
    let errors = command(
        project.path(),
        &[
            "--lab-id",
            metadata["lab_id"].as_str().unwrap(),
            "investigate",
            "timeline",
            "--kind",
            "check.invocation_error",
        ],
    );
    assert!(!errors["result"]["items"].as_array().unwrap().is_empty());
}
#[test]
fn startup_cancellation_awaits_the_normal_operation_lock_path() {
    use std::process::Stdio;
    let project = tempfile::tempdir().unwrap();
    let ws = ronchi::workspace::Workspace::new(project.path());
    ws.initialize().unwrap();
    let lock = ws.lock("demo", "operation").unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    for mode in ["signal", "pipe"] {
        let mut child = base(project.path())
            .args(["demo", "tx-response-lost", "--stop-after-demo"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let _ = stage(project.path(), "configured");
        if mode == "signal" {
            assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
        } else {
            drop(child.stdout.take());
        }
        let output = wait(child);
        assert_eq!(
            output.status.code(),
            Some(if mode == "signal" { 130 } else { 0 })
        );
        assert!(
            std::fs::read_dir(project.path().join("ronchi/labs"))
                .unwrap()
                .next()
                .is_none()
        );
        // Remove only test-owned metadata so the next process barrier is unambiguous.
        std::fs::remove_dir_all(project.path().join("ronchi/demos")).unwrap();
    }
}

#[test]
fn concurrent_demos_preserve_user_default_config_and_text_lifecycle() {
    use std::process::{Command, Stdio};
    let project = tempfile::Builder::new()
        .prefix("ronchi demo ' ")
        .tempdir()
        .unwrap();
    let existing = command(
        project.path(),
        &[
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ],
    );
    let default_id = existing["lab_id"].as_str().unwrap();
    let _cleanup = LabCleanup {
        project: project.path().to_path_buf(),
        id: default_id.into(),
    };
    let user_config =
        b"This intentionally invalid user configuration must remain byte-identical.\n";
    std::fs::write(project.path().join("ronchi/config.toml"), user_config).unwrap();
    let launch = |json: bool| {
        let mut process = Command::new(env!("CARGO_BIN_EXE_ronchi"));
        process.args(["--project", project.path().to_str().unwrap()]);
        if json {
            process.arg("--json");
        }
        process
            .env("RONCHI_MODE", "surfpool")
            .env(
                "RONCHI_READ_UPSTREAM",
                "http://must-not-be-contacted.invalid",
            )
            .env(
                "RONCHI_SURFPOOL_DATASOURCE",
                "http://must-not-be-contacted.invalid",
            )
            .args(["demo", "tx-response-lost", "--stop-after-demo"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let first = launch(true);
    let second = launch(false);
    let first = wait(first);
    let second = wait(second);
    assert!(first.status.success(), "{first:?}");
    assert!(second.status.success(), "{second:?}");
    let a: Value = serde_json::from_slice(&first.stdout).unwrap();
    let b: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_ne!(a["result"]["lab_id"], b["result"]["lab_id"]);
    assert_ne!(a["result"]["reference_name"], b["result"]["reference_name"]);
    for result in [&a["result"], &b["result"]] {
        assert_eq!(result["check"]["verdict"], "pass");
        assert_eq!(result["lab_state"], "stopped");
        assert_eq!(result["reference_stopped"], true);
        let argv = result["command_argv"][0]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        let output = Command::new(argv[0]).args(&argv[1..]).output().unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    assert_eq!(
        std::fs::read(project.path().join("ronchi/config.toml")).unwrap(),
        user_config
    );
    let status = command(project.path(), &["--lab-id", default_id, "status"]);
    assert_eq!(status["result"]["state"], "ready");
    assert_eq!(
        status["result"]["snapshot_seq"],
        existing["result"]["snapshot_seq"]
    );
    command(project.path(), &["--lab-id", default_id, "down"]);
    let forbidden = base(project.path())
        .args(["--lab", "default", "demo", "tx-response-lost"])
        .output()
        .unwrap();
    assert_eq!(forbidden.status.code(), Some(2));
}

#[test]
fn abnormal_exit_after_final_proof_is_an_operational_error() {
    use std::process::Stdio;
    let project = tempfile::tempdir().unwrap();
    let child = base(project.path())
        .env("RONCHI_DEMO_TEST_BARRIER", "proof_complete")
        .args(["demo", "tx-response-lost", "--stop-after-demo"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (directory, metadata) = stage(project.path(), "proof_complete");
    let pid = metadata["reference_pid"].as_u64().unwrap() as u32;
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGKILL) }, 0);
    std::fs::write(directory.join("release"), []).unwrap();
    let output = wait(child);
    assert!(!output.status.success());
    let failure: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(failure["error"]["code"], "cleanup_failed");
    assert_eq!(failure["error"]["reference_stopped"], true);
    assert_eq!(failure["error"]["lab_stopped"], true);
    assert!(failure["error"]["snapshot_seq"].is_u64());
}

struct LabCleanup {
    project: std::path::PathBuf,
    id: String,
}
impl Drop for LabCleanup {
    fn drop(&mut self) {
        let _ = base(&self.project)
            .args(["--lab-id", &self.id, "down", "--force"])
            .output();
    }
}
