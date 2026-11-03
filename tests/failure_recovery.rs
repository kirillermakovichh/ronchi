//! Whole-process storage failures preserve acknowledged evidence and independent work.
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use ronchi::{journal::JournalReader, process::ProcessIdentity};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::Path,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

struct Lab {
    directory: Option<tempfile::TempDir>,
    id: String,
    rpc: String,
    process: ProcessIdentity,
}
impl Lab {
    fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let value = ok(command(
            directory.path(),
            &[
                "up",
                "--rpc-listen",
                "127.0.0.1:0",
                "--control-listen",
                "127.0.0.1:0",
            ],
        ));
        Self {
            id: value["lab_id"].as_str().unwrap().into(),
            rpc: value["result"]["rpc_address"].as_str().unwrap().into(),
            process: serde_json::from_value(value["result"]["process"].clone()).unwrap(),
            directory: Some(directory),
        }
    }
    fn path(&self) -> &Path {
        self.directory.as_ref().unwrap().path()
    }
    fn journal(&self) -> std::path::PathBuf {
        self.path()
            .join("ronchi/labs")
            .join(&self.id)
            .join("journal.sqlite")
    }
    fn rpc(&self, method: &str, params: Value) -> Value {
        http(
            &self.rpc,
            "POST",
            "/",
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}),
        )
        .unwrap()
    }
    fn wait_stopped(&self) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while ronchi::process::matches(&self.process).unwrap() {
            assert!(
                Instant::now() < deadline,
                "Lab failed to stop: {}",
                self.path().display()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            TcpStream::connect(&self.rpc).is_err(),
            "RPC still accepts connections"
        );
    }
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = command(self.path(), &["--lab-id", &self.id, "down", "--force"]);
        if std::thread::panicking() || !matches!(ronchi::process::matches(&self.process), Ok(false))
        {
            let retained = self.directory.take().unwrap().keep();
            eprintln!(
                "Failure or incomplete cleanup; retained fixture {}",
                retained.display()
            );
        }
    }
}
fn command(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ronchi"));
    for (name, _) in std::env::vars().filter(|(name, _)| name.starts_with("RONCHI_")) {
        command.env_remove(name);
    }
    command
        .args(["--json", "--project"])
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
fn ok(output: Output) -> Value {
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}
fn http(address: &str, method: &str, path: &str, value: Value) -> std::io::Result<Value> {
    let body = if method == "GET" {
        String::new()
    } else {
        value.to_string()
    };
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    let mut response = String::new();
    stream.take(300 * 1024).read_to_string(&mut response)?;
    serde_json::from_str(response.split_once("\r\n\r\n").map_or("", |(_, b)| b))
        .map_err(std::io::Error::other)
}
struct Reference {
    child: Child,
    address: String,
}
impl Reference {
    fn start(lab: &Lab) -> Self {
        let log = std::fs::File::create(lab.path().join("reference-ready.jsonl")).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_ronchi"))
            .arg("--json")
            .arg("--project")
            .arg(lab.path())
            .args([
                "--lab-id",
                &lab.id,
                "reference",
                "relayer",
                "--profile",
                "resilient",
                "--listen",
                "127.0.0.1:0",
            ])
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Ok(value) = serde_json::from_slice::<Value>(
                &std::fs::read(lab.path().join("reference-ready.jsonl")).unwrap(),
            ) && let Some(address) = value["result"]["listen"].as_str()
            {
                return Self {
                    child,
                    address: address.into(),
                };
            }
            if child.try_wait().unwrap().is_some() || Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Reference readiness failed");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn assert_alive(&mut self) {
        assert!(self.child.try_wait().unwrap().is_none());
        assert_eq!(
            http(&self.address, "GET", "/intents/missing", Value::Null).unwrap()["availability"],
            "unavailable"
        );
    }
}
impl Drop for Reference {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
fn signed(hash: &str, instructions: bool) -> (String, Vec<u8>) {
    let signer = SigningKey::from_bytes(&[37; 32]);
    let mut message = if instructions {
        vec![1, 0, 1, 2]
    } else {
        vec![1, 0, 0, 1]
    };
    message.extend(signer.verifying_key().to_bytes());
    if instructions {
        message.extend([11; 32]);
    }
    message.extend(bs58::decode(hash).into_vec().unwrap());
    if instructions {
        // 200 unknown-program instructions remain below the decoder allocation bound.
        // Padding one instruction fills the 1,232-byte packet.
        message.extend([0xc8, 0x01]);
        message.extend([1, 0, 0xd0, 0x03]);
        message.extend([7; 464]);
        for _ in 0..199 {
            message.extend([1, 0, 0]);
        }
    } else {
        message.push(0);
    }
    let signature = signer.sign(&message).to_bytes();
    let mut wire = vec![1];
    wire.extend(signature);
    wire.extend(message);
    (bs58::encode(signature).into_string(), wire)
}
fn accepted(lab: &Lab, instructions: bool) -> (String, Vec<u8>) {
    let hash = lab.rpc("getLatestBlockhash", json!([{"commitment":"processed"}]))["result"]["value"]["blockhash"].as_str().unwrap().to_string();
    let (signature, wire) = signed(&hash, instructions);
    let reply = lab.rpc(
        "sendTransaction",
        json!([STANDARD.encode(&wire),{"encoding":"base64","skipPreflight":true}]),
    );
    assert_eq!(reply["result"], signature, "{reply}");
    (signature, wire)
}
fn storage_failure(disk: bool) {
    let lab = Lab::start();
    let mut reference = Reference::start(&lab);
    reference.assert_alive();
    let (signature, wire) = accepted(&lab, false);
    if disk {
        // Sparse fixture exceeds only this Lab's quota; it does not fill the host disk.
        std::fs::File::create(lab.journal().parent().unwrap().join("retained.data"))
            .unwrap()
            .set_len(1024 * 1024 * 1024)
            .unwrap();
    } else {
        let connection = rusqlite::Connection::open(lab.journal()).unwrap();
        connection.busy_timeout(Duration::from_secs(2)).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_public_append BEFORE INSERT ON events WHEN json_extract(NEW.event_json,'$.kind')='rpc.registered' BEGIN SELECT RAISE(ABORT,'injected storage failure'); END;").unwrap();
    }
    let _ = http(
        &lab.rpc,
        "POST",
        "/",
        json!({"jsonrpc":"2.0","id":2,"method":"getSlot","params":[]}),
    );
    lab.wait_stopped();
    reference.assert_alive();
    let status = ok(command(lab.path(), &["--lab-id", &lab.id, "status"]));
    assert_eq!(status["result"]["journal_state"], "interrupted", "{status}");
    assert_eq!(status["result"]["health"], "failed", "{status}");
    let reader = JournalReader::open(&lab.journal()).unwrap();
    assert_eq!(
        reader.transaction(&signature).unwrap().unwrap().wire_base64,
        STANDARD.encode(wire)
    );
    assert!(reader.snapshot(false).unwrap().snapshot_seq >= 2);
}
#[test]
fn writer_failure_stops_whole_lab_and_preserves_acknowledged_bytes_and_reference() {
    storage_failure(false);
}
#[test]
fn disk_failure_stops_whole_lab_and_preserves_acknowledged_bytes_and_reference() {
    storage_failure(true);
}

#[test]
fn parsed_packet_batch_returns_bounded_errors_without_killing_lab() {
    let lab = Lab::start();
    let (signature, wire) = accepted(&lab, true);
    assert_eq!(wire.len(), 1232);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let value = lab.rpc("getSignatureStatuses", json!([[signature]]));
        if matches!(
            value["result"]["value"][0]["confirmationStatus"].as_str(),
            Some("confirmed" | "finalized")
        ) {
            break;
        }
        assert!(Instant::now() < deadline, "{value}");
        std::thread::sleep(Duration::from_millis(50));
    }
    let params = json!([signature,{"encoding":"jsonParsed","commitment":"confirmed"}]);
    assert!(lab.rpc("getTransaction", params.clone())["result"].is_object());
    let batch: Vec<_> = (0..64)
        .map(|id| json!({"jsonrpc":"2.0","id":id,"method":"getTransaction","params":params}))
        .collect();
    let reply = http(&lab.rpc, "POST", "/", json!(batch));
    std::fs::write(lab.path().join("probe-response.txt"), format!("{reply:?}")).unwrap();
    let status = command(lab.path(), &["status"]);
    std::fs::write(lab.path().join("probe-status.json"), &status.stdout).unwrap();
    assert!(
        ronchi::process::matches(&lab.process).unwrap(),
        "valid parsed batch killed Lab; original response={reply:?}; fixture={}",
        lab.path().display()
    );
    let reply = reply.unwrap();
    let items = reply.as_array().unwrap_or_else(|| panic!("{reply}"));
    assert_eq!(items.len(), 64);
    for item in items {
        assert_eq!(item["error"]["data"]["code"], "result_too_large", "{item}");
    }
    assert_eq!(ok(status)["result"]["state"], "ready");
}
