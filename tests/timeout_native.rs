use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::{Command, Output, Stdio},
    time::Duration,
};
struct Project(tempfile::TempDir);
impl Project {
    fn run(&self, args: &[&str]) -> Output {
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
            .arg(self.0.path())
            .arg("--json")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
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
    }
}
fn rpc(address: &str, method: &str, params: Value) -> Vec<u8> {
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let b = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();
    write!(
        stream,
        "POST / HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{b}",
        b.len()
    )
    .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    response
}
#[test]
fn native_surfpool_timeout_prevents_dispatch_then_recovers_with_source_authority() {
    let Ok(archive) = std::env::var("RONCHI_TEST_SURFPOOL_ARCHIVE") else {
        eprintln!("native archive absent; Surfpool timeout coverage unverified");
        return;
    };
    let p = Project(tempfile::tempdir().unwrap());
    p.ok(&["tool", "surfpool", "install", "--archive", &archive]);
    let up = p.ok(&[
        "up",
        "--detach",
        "--mode",
        "surfpool",
        "--rpc-listen",
        "127.0.0.1:0",
        "--control-listen",
        "127.0.0.1:0",
    ]);
    let id = up["lab_id"].as_str().unwrap();
    let address = up["result"]["rpc_address"].as_str().unwrap();
    let reply = rpc(address, "getLatestBlockhash", json!([]));
    let source: Value = serde_json::from_str(
        std::str::from_utf8(&reply)
            .unwrap()
            .split_once("\r\n\r\n")
            .unwrap()
            .1,
    )
    .unwrap();
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    let payer = SigningKey::from_bytes(&[66u8; 32]);
    let mut message = vec![1, 0, 0, 1];
    message.extend(payer.verifying_key().to_bytes());
    message.extend(
        bs58::decode(source["result"]["value"]["blockhash"].as_str().unwrap())
            .into_vec()
            .unwrap(),
    );
    message.push(0);
    let signature = payer.sign(&message);
    let mut wire = vec![1];
    wire.extend(signature.to_bytes());
    wire.extend(message);
    let arm = p.ok(&[
        "case",
        "arm",
        "provider-timeout-burst",
        "--hold-ms",
        "120",
        "--rpc-method",
        "getLatestBlockhash",
        "--rpc-method",
        "getBalance",
        "--rpc-method",
        "sendTransaction",
    ]);
    assert_eq!(arm["result"]["active_case"]["timeout"]["outage_ms"], 5000);
    let malformed = rpc(address, "getBalance", json!(["bad"]));
    assert!(String::from_utf8_lossy(&malformed).contains("-32602"));
    let held = rpc(address, "getLatestBlockhash", json!([]));
    assert!(held.is_empty());
    let account = bs58::encode([29u8; 32]).into_string();
    assert!(rpc(address, "getBalance", json!([account])).is_empty());
    assert!(rpc(address,"sendTransaction",json!([base64::engine::general_purpose::STANDARD.encode(wire),{"encoding":"base64","skipPreflight":true}])).is_empty());
    p.ok(&["case", "disarm"]);
    let recovered = rpc(address, "getLatestBlockhash", json!([]));
    assert!(String::from_utf8_lossy(&recovered).contains("blockhash"));
    p.ok(&["down"]);
    let findings = p.ok(&["--lab-id", id, "investigate", "findings"]);
    let finding = findings["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["rule_id"] == "provider-timeout-burst.later-rpc-attempt")
        .unwrap_or_else(|| panic!("{findings}"));
    assert_eq!(finding["scope"]["rpc_method"], "getLatestBlockhash");
    assert!(finding["scope"].get("accepted_request_id").is_none());
    let reader = ronchi::journal::JournalReader::open(
        &p.0.path()
            .join("ronchi/labs")
            .join(id)
            .join("journal.sqlite"),
    )
    .unwrap();
    let facts: Vec<_> = (1..=reader.snapshot(false).unwrap().snapshot_seq)
        .map(|s| reader.event(s).unwrap().unwrap())
        .collect();
    let prevented: Vec<_> = facts
        .iter()
        .filter(|f| f.kind == "rpc.dispatch_prevented")
        .collect();
    assert_eq!(prevented.len(), 3);
    assert!(
        prevented
            .iter()
            .all(|f| f.execution_mode == ronchi::config::Mode::Surfpool)
    );
    for held in prevented {
        let req = &held.data["request"]["request_id"];
        assert!(!facts.iter().any(|f| {
            [
                "rpc.backend_started",
                "transaction.submission",
                "transaction.accepted",
            ]
            .contains(&f.kind.as_str())
                && f.data["request"]["request_id"] == *req
        }));
    }
    assert!(
        facts
            .iter()
            .any(|f| f.kind == "rpc.backend" && f.source == "surfpool")
    );
}
