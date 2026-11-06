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
        let mut command = Command::new(env!("CARGO_BIN_EXE_ronchi"));
        for key in [
            "RONCHI_MODE",
            "RONCHI_READ_UPSTREAM",
            "RONCHI_SURFPOOL_DATASOURCE",
            "RONCHI_RPC_LISTEN",
            "RONCHI_CONTROL_LISTEN",
            "RONCHI_INITIAL_CASE",
            "RONCHI_SLOT_DURATION_MS",
        ] {
            command.env_remove(key);
        }
        command
            .arg("--project")
            .arg(self.path())
            .arg("--json")
            .args(args)
            .stdin(Stdio::null());
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let output = self.run(args);
        assert!(output.status.success(), "{output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = self.run(&["down", "--force"]);
        if std::thread::panicking() {
            self.0.disable_cleanup(true);
            eprintln!("retained native fixture {}", self.path().display());
        }
    }
}
#[test]
fn offline_installer_rejects_unpinned_artifact_before_creating_executable() {
    let p = Project::new();
    let archive = p.path().join("untrusted.tar.gz");
    std::fs::write(&archive, "not the pinned artifact").unwrap();
    let output = p.run(&[
        "tool",
        "surfpool",
        "install",
        "--archive",
        archive.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(5), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "state_conflict");
    assert!(
        !p.path()
            .join("ronchi/tools/surfpool/1.7.0/surfpool")
            .exists()
    );
}
fn archive() -> Option<String> {
    std::env::var("RONCHI_TEST_SURFPOOL_ARCHIVE").ok()
}
fn install(p: &Project, archive: &str) {
    let result = p.ok(&["tool", "surfpool", "install", "--archive", archive]);
    assert_eq!(result["result"]["version"], "1.7.0");
}
fn rpc(address: &str, method: &str, params: Value) -> Value {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(12)))
        .unwrap();
    let body =
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();
    write!(stream,"POST / HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let body = response.split_once("\r\n\r\n").unwrap().1;
    if response
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut decoded = String::new();
        let mut rest = body;
        loop {
            let (size, next) = rest.split_once("\r\n").unwrap();
            let size = usize::from_str_radix(size, 16).unwrap();
            if size == 0 {
                break;
            }
            decoded.push_str(&next[..size]);
            rest = &next[size + 2..];
        }
        serde_json::from_str(&decoded).unwrap()
    } else {
        serde_json::from_str(body).unwrap()
    }
}
#[test]
fn real_native_backend_owns_all_rpc_and_cleanup_preserves_independent_listener() {
    let Some(archive) = archive() else {
        eprintln!(
            "native Surfpool fixture absent; set RONCHI_TEST_SURFPOOL_ARCHIVE to verify execution"
        );
        return;
    };
    let p = Project::new();
    install(&p, &archive);
    let independent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let result = p.ok(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let manifest = &result["result"];
    assert_eq!(manifest["configuration"]["mode"], "surfpool");
    assert_eq!(manifest["versions"]["surfpool"], "1.7.0");
    let owned: ronchi::process::ProcessIdentity =
        serde_json::from_value(manifest["owned_children"][0].clone()).unwrap();
    assert!(ronchi::process::matches(&owned).unwrap());
    let address = manifest["rpc_address"].as_str().unwrap();
    let version = rpc(address, "getVersion", serde_json::json!([]));
    assert!(
        !version["result"]["solana-core"]
            .as_str()
            .unwrap()
            .contains("ronchi")
    );
    let account = bs58::encode([91u8; 32]).into_string();
    let balance = rpc(address, "getBalance", serde_json::json!([account]));
    assert!(balance.get("result").is_some(), "{balance}");
    let discovery = rpc(
        address,
        "getProgramAccounts",
        serde_json::json!(["11111111111111111111111111111111", {"encoding":"base64","withContext":true,"filters":[{"dataSize":0}]}]),
    );
    assert!(
        discovery["result"]["context"]["slot"].is_number(),
        "{discovery}"
    );
    assert!(discovery["result"]["value"].is_array(), "{discovery}");
    let invalid = rpc(
        address,
        "getProgramAccounts",
        serde_json::json!(["invalid-program-key"]),
    );
    assert!(invalid.get("error").is_some(), "{invalid}");
    assert_eq!(
        rpc(address, "getHealth", serde_json::json!([]))["result"],
        "ok"
    );
    let unsupported = p.run(&["case", "arm", "tx-confirmation-stalls"]);
    assert_eq!(unsupported.status.code(), Some(4), "{unsupported:?}");
    let stopped = p.ok(&["down"]);
    assert_eq!(stopped["result"]["state"], "stopped");
    assert!(!ronchi::process::matches(&owned).unwrap());
    assert!(independent.local_addr().is_ok());
}
fn funded_snapshot(p: &Project, payer: &str) {
    let snapshot = serde_json::json!({payer:{"lamports":1_000_000_000u64,"owner":"11111111111111111111111111111111","executable":false,"rentEpoch":0,"data":"","parsedData":null}});
    std::fs::write(p.path().join("snapshot.json"), snapshot.to_string()).unwrap();
    std::fs::create_dir_all(p.path().join("ronchi")).unwrap();
    std::fs::write(
        p.path().join("ronchi/config.toml"),
        "schema_version = 1\nsurfpool_snapshot = 'snapshot.json'\n",
    )
    .unwrap();
}
fn transfer(hash: &str, seed: u8, recipient: [u8; 32]) -> (String, String) {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    let payer = SigningKey::from_bytes(&[seed; 32]);
    let mut message = vec![1, 0, 1, 3];
    message.extend(payer.verifying_key().to_bytes());
    message.extend(recipient);
    message.extend([0u8; 32]);
    message.extend(bs58::decode(hash).into_vec().unwrap());
    message.extend([1, 2, 2, 0, 1, 12]);
    message.extend(2u32.to_le_bytes());
    message.extend(1_000_000u64.to_le_bytes());
    let signature = payer.sign(&message);
    let mut wire = vec![1];
    wire.extend(signature.to_bytes());
    wire.extend(message);
    (
        bs58::encode(signature.to_bytes()).into_string(),
        base64::engine::general_purpose::STANDARD.encode(wire),
    )
}
fn lost_response(address: &str, wire: &str) {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(12)))
        .unwrap();
    let body=serde_json::json!({"jsonrpc":"2.0","id":1,"method":"sendTransaction","params":[wire,{"encoding":"base64"}]}).to_string();
    write!(stream,"POST / HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    assert!(
        response.is_empty(),
        "{}",
        String::from_utf8_lossy(&response)
    );
}
#[test]
fn real_execution_uses_explicit_snapshot_and_response_loss_never_restarts_identity() {
    use ed25519_dalek::SigningKey;
    use std::time::{Duration, Instant};
    let Some(archive) = archive() else {
        return;
    };
    let p = Project::new();
    let payer =
        bs58::encode(SigningKey::from_bytes(&[54; 32]).verifying_key().to_bytes()).into_string();
    let recipient = bs58::encode([55u8; 32]).into_string();
    funded_snapshot(&p, &payer);
    install(&p, &archive);
    let started = p.ok(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let id = started["lab_id"].as_str().unwrap();
    let address = started["result"]["rpc_address"].as_str().unwrap();
    assert_eq!(
        rpc(address, "getBalance", serde_json::json!([payer]))["result"]["value"],
        1_000_000_000u64
    );
    let hash =
        rpc(address, "getLatestBlockhash", serde_json::json!([]))["result"]["value"]["blockhash"]
            .as_str()
            .unwrap()
            .to_owned();
    let (signature, wire) = transfer(&hash, 54, [55; 32]);
    let simulation = rpc(
        address,
        "simulateTransaction",
        serde_json::json!([wire,{"encoding":"base64","sigVerify":true}]),
    );
    assert!(simulation["result"]["value"].is_object(), "{simulation}");
    assert!(
        simulation["result"]["value"]["err"].is_null(),
        "{simulation}"
    );
    assert_eq!(
        rpc(address, "getBalance", serde_json::json!([recipient]))["result"]["value"],
        0
    );
    p.ok(&["case", "arm", "tx-response-lost", "--hold-ms", "200"]);
    lost_response(address, &wire);
    let analysis = p.ok(&["investigate", "findings"]);
    let base = analysis["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["rule_id"] == "tx-response-lost.surfpool-response-withheld")
        .expect("native Surfpool withholding has a Finding before any lookup");
    assert_eq!(base["scope"]["signature"], signature);
    assert_eq!(base["observation_window"]["status_lookup_count"], 0);
    assert_eq!(
        p.ok(&["status"])["result"]["case_activity"]["applied_records"],
        1
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let status = rpc(
            address,
            "getSignatureStatuses",
            serde_json::json!([[signature]]),
        );
        if !status["result"]["value"][0].is_null() {
            assert!(status["result"]["value"][0]["err"].is_null(), "{status}");
            break;
        }
        assert!(Instant::now() < deadline, "{status}");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        rpc(address, "getBalance", serde_json::json!([recipient]))["result"]["value"],
        1_000_000u64
    );
    let discovery = rpc(
        address,
        "getProgramAccounts",
        serde_json::json!(["11111111111111111111111111111111", {"encoding":"base64","withContext":true,"dataSlice":{"offset":0,"length":0},"filters":[{"dataSize":0}]}]),
    );
    let accounts = discovery["result"]["value"]
        .as_array()
        .expect("context-wrapped program accounts");
    assert!(
        accounts
            .iter()
            .any(|a| a["pubkey"] == recipient && a["account"]["lamports"] == 1_000_000u64),
        "{discovery}"
    );
    let filtered = rpc(
        address,
        "getProgramAccounts",
        serde_json::json!(["11111111111111111111111111111111", {"encoding":"base64","filters":[{"dataSize":1}]}]),
    );
    assert!(
        !filtered["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["pubkey"] == recipient)
    );
    let after_lookup = p.ok(&["investigate", "findings"]);
    assert!(
        after_lookup["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["rule_id"] == "tx-response-lost.lookup-after-withholding")
    );
    for _ in 0..2 {
        p.ok(&["case", "disarm"]);
        p.ok(&["case", "arm", "tx-response-lost", "--hold-ms", "200"]);
        let retry = rpc(
            address,
            "sendTransaction",
            serde_json::json!([wire,{"encoding":"base64"}]),
        );
        assert!(
            retry["result"] == signature || retry.get("error").is_some(),
            "{retry}"
        );
    }
    let missing_hash = rpc(address, "getLatestBlockhash", serde_json::json!([]))["result"]["value"]
        ["blockhash"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, missing_wire) = transfer(&missing_hash, 59, [55; 32]);
    let missing = rpc(
        address,
        "sendTransaction",
        serde_json::json!([missing_wire,{"encoding":"base64"}]),
    );
    assert!(missing.get("error").is_some(), "{missing}");
    p.ok(&["down"]);
    let reader = ronchi::journal::JournalReader::open(
        &p.path().join("ronchi/labs").join(id).join("journal.sqlite"),
    )
    .unwrap();
    let mut accepted = 0;
    let mut selected = 0;
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let fact = reader.event(seq).unwrap().unwrap();
        assert_eq!(
            fact.execution_mode,
            ronchi::config::Mode::Surfpool,
            "{}",
            fact.kind
        );
        if fact.kind == "transaction.accepted" {
            accepted += 1;
            assert_eq!(fact.source, "surfpool");
            assert_eq!(fact.data["signature"], signature);
            assert!(fact.data["next_due_ns"].is_null());
        }
        if fact.kind == "fault.response_loss_selected" {
            selected += 1;
        }
        assert_ne!(fact.kind, "transaction.transition");
    }
    assert_eq!((accepted, selected), (1, 1));
    let next = p.ok(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let next = next["result"]["rpc_address"].as_str().unwrap();
    assert_eq!(
        rpc(next, "getBalance", serde_json::json!([payer]))["result"]["value"],
        1_000_000_000u64
    );
    assert_eq!(
        rpc(next, "getBalance", serde_json::json!([recipient]))["result"]["value"],
        0
    );
}
#[test]
fn missing_tool_and_confirmation_mode_are_rejected_before_lab_allocation() {
    let p = Project::new();
    let absent = p.run(&["up", "--detach", "--mode", "surfpool"]);
    assert_eq!(absent.status.code(), Some(4), "{absent:?}");
    assert!(!p.path().join("ronchi").exists());
    let unsupported = p.run(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--case",
        "tx-confirmation-stalls",
    ]);
    assert_eq!(unsupported.status.code(), Some(4), "{unsupported:?}");
    assert!(!p.path().join("ronchi").exists());
}
#[test]
fn real_native_readiness_failure_and_binary_tampering_preserve_ownership_evidence() {
    let Some(archive) = archive() else {
        return;
    };
    let p = Project::new();
    install(&p, &archive);
    std::fs::write(p.path().join("snapshot.json"), "{\"accounts\":{}}").unwrap();
    std::fs::write(
        p.path().join("ronchi/config.toml"),
        "schema_version=1\nsurfpool_snapshot='snapshot.json'\n",
    )
    .unwrap();
    let failed = p.run(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    assert_eq!(failed.status.code(), Some(6), "{failed:?}");
    let status = p.ok(&["status"]);
    assert_eq!(status["result"]["journal_state"], "interrupted");
    let ws = ronchi::workspace::Workspace::new(p.path());
    let manifest = ws.target("default", None).unwrap();
    for child in manifest.owned_children {
        assert!(!ronchi::process::matches(&child).unwrap());
    }
    let binary = p.path().join("ronchi/tools/surfpool/1.7.0/surfpool");
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(binary)
        .unwrap()
        .write_all(b"tampered")
        .unwrap();
    let changed = p.run(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    assert_eq!(changed.status.code(), Some(5), "{changed:?}");
}
#[test]
fn real_native_child_death_fails_lab_without_touching_independent_backend() {
    use std::time::{Duration, Instant};
    let Some(archive) = archive() else {
        return;
    };
    let p = Project::new();
    install(&p, &archive);
    let independent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let started = p.ok(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let manifest: ronchi::lab::Manifest =
        serde_json::from_value(started["result"].clone()).unwrap();
    // SAFETY: child PID comes from the ready disposable Lab's owned process identity.
    assert_eq!(
        unsafe { libc::kill(manifest.owned_children[0].pid as i32, libc::SIGKILL) },
        0
    );
    let daemon = manifest.process.unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while ronchi::process::matches(&daemon).unwrap() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let retained = p.ok(&["status"]);
    assert_eq!(retained["result"]["journal_state"], "interrupted");
    assert!(retained["result"]["rpc_address"].is_null());
    assert!(independent.local_addr().is_ok());
}
#[test]
fn real_verified_orphan_is_cleaned_before_fresh_lab_name_reuse() {
    use std::time::{Duration, Instant};
    let Some(archive) = archive() else {
        return;
    };
    let p = Project::new();
    install(&p, &archive);
    let started = p.ok(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let old = ronchi::workspace::Workspace::new(p.path())
        .manifest(started["lab_id"].as_str().unwrap())
        .unwrap();
    let daemon = old.process.clone().unwrap();
    // SAFETY: this is the daemon created and authenticated by this disposable project.
    assert_eq!(unsafe { libc::kill(daemon.pid as i32, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(3);
    while ronchi::process::matches(&daemon).unwrap() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(ronchi::process::matches(&old.owned_children[0]).unwrap());
    let restarted = p.ok(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    assert_ne!(restarted["lab_id"], old.lab_id);
    assert!(!ronchi::process::matches(&old.owned_children[0]).unwrap());
}

#[test]
fn offline_installer_rejects_fifo_without_waiting_for_a_writer() {
    use std::time::{Duration, Instant};
    let p = Project::new();
    let fifo = p.path().join("archive.fifo");
    let path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: path points to a nul-terminated owned test path.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let mut child = p
        .command(&[
            "tool",
            "surfpool",
            "install",
            "--archive",
            fifo.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("installer blocked on a non-regular input");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(5), "{output:?}");
    assert!(!p.path().join("ronchi").exists());
}

#[test]
fn real_status_cases_preserve_native_execution_and_first_observed_finality() {
    use ed25519_dalek::SigningKey;
    use serde_json::json;
    use std::time::{Duration, Instant};
    let Some(archive) = archive() else {
        eprintln!("native status evidence requires RONCHI_TEST_SURFPOOL_ARCHIVE");
        return;
    };
    let p = Project::new();
    let payer = bs58::encode(
        SigningKey::from_bytes(&[134; 32])
            .verifying_key()
            .to_bytes(),
    )
    .into_string();
    funded_snapshot(&p, &payer);
    install(&p, &archive);
    let ready = p.ok(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let id = ready["lab_id"].as_str().unwrap();
    let address = ready["result"]["rpc_address"].as_str().unwrap();
    p.ok(&["case", "arm", "signature-status-null"]);
    let hash = rpc(address, "getLatestBlockhash", json!([]))["result"]["value"]["blockhash"]
        .as_str()
        .unwrap()
        .to_string();
    let (signature, wire) = transfer(&hash, 134, [135; 32]);
    assert_eq!(
        rpc(
            address,
            "sendTransaction",
            json!([wire,{"encoding":"base64"}])
        )["result"],
        signature
    );
    let recipient = bs58::encode([135; 32]).into_string();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if rpc(address, "getBalance", json!([recipient]))["result"]["value"] == 1_000_000u64 {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(25));
    }
    let unknown = bs58::encode([136; 64]).into_string();
    assert_eq!(
        rpc(
            address,
            "getSignatureStatuses",
            json!([[signature, unknown, signature]])
        )["result"]["value"],
        json!([null, null, null])
    );
    assert!(
        rpc(
            address,
            "getTransaction",
            json!([signature,{"commitment":"confirmed"}])
        )["result"]
            .is_null()
    );
    let duplicate = rpc(
        address,
        "sendTransaction",
        json!([wire,{"encoding":"base64"}]),
    );
    assert!(duplicate["result"] == signature || duplicate.get("error").is_some());
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let status = rpc(address, "getSignatureStatuses", json!([[signature]]));
        if status["result"]["value"][0].is_object() {
            break;
        }
        assert!(Instant::now() < deadline, "{status}");
        std::thread::sleep(Duration::from_millis(100));
    }
    p.ok(&["case", "arm", "stale-transaction-read-after-submission"]);
    let hash = rpc(address, "getLatestBlockhash", json!([]))["result"]["value"]["blockhash"]
        .as_str()
        .unwrap()
        .to_string();
    let (stale, wire) = transfer(&hash, 134, [137; 32]);
    assert_eq!(
        rpc(
            address,
            "sendTransaction",
            json!([wire,{"encoding":"base64"}])
        )["result"],
        stale
    );
    let recipient = bs58::encode([137; 32]).into_string();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if rpc(address, "getBalance", json!([recipient]))["result"]["value"] == 1_000_000u64 {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(25));
    }
    let transaction = rpc(
        address,
        "getTransaction",
        json!([stale,{"commitment":"confirmed","encoding":"base64"}]),
    );
    assert!(
        transaction["result"].is_object(),
        "no status snapshot exists yet: {transaction}"
    );
    // Native account execution and transaction lookup can precede Surfpool's status-cache entry.
    // Capture the first actually observed status, rather than assuming those surfaces update together.
    let deadline = Instant::now() + Duration::from_secs(5);
    let first = loop {
        let response = rpc(address, "getSignatureStatuses", json!([[stale]]));
        let status = response["result"]["value"][0].clone();
        if status.is_object() {
            break status;
        }
        assert!(
            response.get("error").is_none() && Instant::now() < deadline,
            "native status unavailable: {response}"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    let again = rpc(
        address,
        "getSignatureStatuses",
        json!([[stale, unknown, stale]]),
    );
    assert_eq!(again["result"]["value"], json!([first, null, first]));
    let finalized = rpc(
        address,
        "getTransaction",
        json!([stale,{"commitment":"finalized","encoding":"base64"}]),
    );
    if first["confirmationStatus"] != "finalized" {
        assert!(finalized["result"].is_null());
    }
    p.ok(&["case", "disarm"]);
    assert_eq!(
        rpc(address, "getBalance", json!([payer]))["result"]["value"],
        997_990_000u64
    );
    p.ok(&["down"]);
    let reader = ronchi::journal::JournalReader::open(
        &p.path().join("ronchi/labs").join(id).join("journal.sqlite"),
    )
    .unwrap();
    let mut source = 0;
    let mut changed = 0;
    let mut missing_snapshot = false;
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "rpc.lookup_source" {
            source += 1;
            assert_eq!(event.source, "surfpool");
        }
        if event.kind == "fault.lookup_changed" {
            changed += 1;
            assert_eq!(event.execution_mode, ronchi::config::Mode::Surfpool);
        }
        if event.kind == "lookup.selection_skipped"
            && event.data["reason"] == "snapshot_unavailable"
        {
            missing_snapshot = true;
        }
    }
    assert!(source >= 4);
    assert!(changed >= 3);
    assert!(missing_snapshot);
    let findings = p.ok(&["--lab-id", id, "investigate", "findings", "--limit", "5"]);
    assert!(
        findings["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["rule_id"] == "rpc.lookup-view.changed"),
        "{findings}"
    );
}

#[test]
fn status_cli_limits_share_core_validation_and_leave_previous_policy_armed() {
    let p = Project::new();
    p.ok(&[
        "up",
        "--detach",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let armed = p.ok(&[
        "case",
        "arm",
        "signature-status-null",
        "--status-max-keys",
        "1",
        "--status-max-snapshot-bytes",
        "1024",
    ]);
    assert_eq!(armed["result"]["active_case"]["status"]["max_keys"], 1);
    assert_eq!(
        armed["result"]["active_case"]["status"]["max_snapshot_bytes"],
        1024
    );
    for args in [
        vec![
            "case",
            "arm",
            "signature-status-null",
            "--status-max-keys",
            "0",
        ],
        vec![
            "case",
            "arm",
            "signature-status-null",
            "--status-max-snapshot-bytes",
            "4194305",
        ],
        vec!["case", "arm", "tx-response-lost", "--status-max-keys", "1"],
    ] {
        let out = p.run(&args);
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        let error: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(error["error"]["code"], "invalid_argument");
    }
    let status = p.ok(&["status"]);
    assert_eq!(
        status["result"]["active_case"]["case_id"],
        "signature-status-null"
    );
    assert_eq!(status["result"]["active_case"]["status"]["max_keys"], 1);
    p.ok(&["down"]);
}

#[path = "cases/blockhash_native.rs"]
mod blockhash_cases;

#[test]
fn blockhash_cli_setting_selects_the_shared_commitment_policy() {
    let p = Project::new();
    p.ok(&[
        "up",
        "--detach",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let result = p.ok(&[
        "case",
        "arm",
        "tx-blockhash-expired",
        "--blockhash-commitment",
        "processed",
    ]);
    assert_eq!(
        result["result"]["active_case"]["blockhash"]["commitment"],
        "processed"
    );
    let invalid = p.run(&[
        "case",
        "arm",
        "tx-response-lost",
        "--blockhash-commitment",
        "processed",
    ]);
    assert_eq!(invalid.status.code(), Some(2), "{invalid:?}");
    assert_eq!(
        p.ok(&["status"])["result"]["active_case"]["case_id"],
        "tx-blockhash-expired"
    );
}

#[path = "cases/custom_native.rs"]
mod custom_cases;

#[tokio::test]
async fn program_account_discovery_can_be_prepared_for_the_owned_backend() {
    let params = serde_json::json!(["11111111111111111111111111111111", {
        "encoding":"base64", "withContext":true,
        "filters":[{"dataSize":752},{"memcmp":{"offset":400,"bytes":"11111111111111111111111111111111"}}]
    }]);
    let prepared = ronchi::backend::prepare("getProgramAccounts", &params)
        .await
        .unwrap();
    assert_eq!(prepared.params, params);
    assert!(prepared.signed.is_none());
}
