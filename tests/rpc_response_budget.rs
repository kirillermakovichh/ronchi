//! Public process regressions for derived response evidence that outgrows a Journal unit.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
struct Lab {
    dir: Option<tempfile::TempDir>,
    id: String,
    address: String,
}
impl Lab {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let result = ok(command(
            dir.path(),
            &[
                "up",
                "--detach",
                "--rpc-listen",
                "127.0.0.1:0",
                "--control-listen",
                "127.0.0.1:0",
            ],
        ));
        Self {
            id: result["lab_id"].as_str().unwrap().into(),
            address: result["result"]["rpc_address"].as_str().unwrap().into(),
            dir: Some(dir),
        }
    }
    fn root(&self) -> &Path {
        self.dir.as_ref().unwrap().path()
    }
    fn rpc(&self, method: &str, params: Value) -> Value {
        http(
            &self.address,
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}),
        )
    }
    fn journal(&self) -> std::path::PathBuf {
        self.root()
            .join("ronchi/labs")
            .join(&self.id)
            .join("journal.sqlite")
    }
    fn accept(&self, v1: bool) -> String {
        let hash =
            self.rpc("getLatestBlockhash", json!([{"commitment":"processed"}]))["result"]["value"]
                ["blockhash"]
                .as_str()
                .unwrap()
                .to_owned();
        let (signature, wire) = signed(&hash, v1);
        assert_eq!(
            self.rpc(
                "sendTransaction",
                json!([STANDARD.encode(wire),{"encoding":"base64","skipPreflight":true}])
            )["result"],
            signature
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let state = self.rpc("getSignatureStatuses", json!([[signature]]));
            if matches!(
                state["result"]["value"][0]["confirmationStatus"].as_str(),
                Some("confirmed" | "finalized")
            ) {
                break;
            }
            assert!(Instant::now() < deadline, "{state}");
            std::thread::sleep(Duration::from_millis(20));
        }
        signature
    }
    fn record_probe(&self, response: &Value) {
        std::fs::write(
            self.root().join("response-budget-probe.json"),
            response.to_string(),
        )
        .unwrap();
        let status = command(self.root(), &["status"]);
        std::fs::write(
            self.root().join("response-budget-status.json"),
            &status.stdout,
        )
        .unwrap();
    }
    fn ready(&self) {
        assert_eq!(
            ok(command(self.root(), &["status"]))["result"]["state"],
            "ready"
        );
        assert_eq!(self.rpc("getHealth", json!([]))["result"], "ok");
    }
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = command(self.root(), &["--lab-id", &self.id, "down", "--force"]);
        if std::thread::panicking() {
            eprintln!(
                "retained response budget fixture {}",
                self.dir.take().unwrap().keep().display()
            );
        }
    }
}
fn command(root: &Path, args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ronchi"));
    for (name, _) in std::env::vars().filter(|(name, _)| name.starts_with("RONCHI_")) {
        c.env_remove(name);
    }
    c.args(["--json", "--project"])
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
fn ok(out: Output) -> Value {
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}
fn http(address: &str, value: Value) -> Value {
    let body = value.to_string();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(stream,"POST / HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
    let mut response = String::new();
    stream
        .take(300 * 1024)
        .read_to_string(&mut response)
        .unwrap();
    let body = response.split_once("\r\n\r\n").unwrap().1;
    if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body).unwrap()
    }
}
fn signed(hash: &str, v1: bool) -> (String, Vec<u8>) {
    let signer = SigningKey::from_bytes(&[37; 32]);
    let mut message = if v1 {
        vec![0x81, 1, 0, 1]
    } else {
        vec![1, 0, 1, 2]
    };
    if v1 {
        message.extend(0u32.to_le_bytes());
        message.extend(bs58::decode(hash).into_vec().unwrap());
        message.extend([100, 2]);
    }
    message.extend(signer.verifying_key().to_bytes());
    message.extend([11; 32]);
    if v1 {
        for _ in 0..100 {
            message.extend([1, 16, 0, 0]);
        }
        message.extend([0; 1600]);
    } else {
        message.extend(bs58::decode(hash).into_vec().unwrap());
        message.extend([0xc8, 0x01]);
        message.extend([1, 0, 0xd0, 0x03]);
        message.extend([7; 464]);
        for _ in 0..199 {
            message.extend([1, 0, 0]);
        }
    }
    let sig = signer.sign(&message).to_bytes();
    let wire = if v1 {
        message.extend(sig);
        message
    } else {
        let mut wire = vec![1];
        wire.extend(sig);
        wire.extend(message);
        assert_eq!(wire.len(), 1232);
        wire
    };
    (bs58::encode(sig).into_string(), wire)
}
#[test]
fn parsed_batch_aggregate_budget_returns_errors_without_poisoning_journal() {
    let lab = Lab::new();
    let signature = lab.accept(false);
    let params = json!([signature,{"encoding":"jsonParsed","commitment":"confirmed"}]);
    let single = lab.rpc("getTransaction", params.clone());
    assert!(single["result"].is_object());
    let batch: Vec<_> = (0..64)
        .map(|id| json!({"jsonrpc":"2.0","id":id,"method":"getTransaction","params":params}))
        .collect();
    let reply = http(&lab.address, json!(batch));
    lab.record_probe(&reply);
    let items = reply.as_array().unwrap();
    assert_eq!(items.len(), 64);
    for item in items {
        assert_eq!(item["error"]["data"]["code"], "result_too_large", "{item}");
    }
    lab.ready();
    let expected = serde_json::to_vec(&single["result"]).unwrap();
    use sha2::{Digest, Sha256};
    let digest: String = Sha256::digest(&expected)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let reader = ronchi::journal::JournalReader::open(&lab.journal()).unwrap();
    let events: Vec<_> = (1..=reader.snapshot(true).unwrap().snapshot_seq)
        .map(|seq| reader.event(seq).unwrap().unwrap())
        .collect();
    let omitted: Vec<_> = events
        .iter()
        .filter(|event| {
            event.kind == "rpc.response_prepared" && event.data.get("omitted_result").is_some()
        })
        .collect();
    assert_eq!(omitted.len(), 64);
    for event in omitted {
        assert_eq!(
            event.data["omitted_result"]["availability"],
            "omitted_by_policy"
        );
        assert_eq!(
            event.data["omitted_result"]["encoded_bytes"],
            expected.len()
        );
        assert_eq!(event.data["omitted_result"]["sha256"], digest);
        assert_eq!(
            event.data["omitted_result"]["value_role"],
            "prepared_rpc_result"
        );
        assert!(event.data.get("source_result").is_none());
        assert_eq!(event.data["payload_status"], "omitted_by_policy");
        assert!(serde_json::to_vec(event).unwrap().len() < ronchi::journal::MAX_EVENT_BYTES);
    }
    drop(reader);
    let notifications: Vec<_> = (0..64)
        .map(|_| json!({"jsonrpc":"2.0","method":"getTransaction","params":params}))
        .collect();
    assert_eq!(http(&lab.address, json!(notifications)), Value::Null);
    lab.ready();
    let reader = ronchi::journal::JournalReader::open(&lab.journal()).unwrap();
    let mut notifications = 0;
    for seq in 1..=reader.snapshot(true).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "rpc.response_prepared"
            && event.data["responds"] == false
            && event.data.get("omitted_result").is_some()
        {
            notifications += 1;
            assert!(event.data.get("prepared_error_code").is_none());
        }
    }
    assert_eq!(notifications, 64);
}

#[test]
fn single_parsed_evidence_over_event_limit_is_a_bounded_rpc_error() {
    let lab = Lab::new();
    let signature = lab.accept(true);
    let binary=lab.rpc("getTransaction",json!([signature,{"encoding":"base64","commitment":"confirmed","maxSupportedTransactionVersion":1}]));
    assert!(binary["result"].is_object());
    let reply=lab.rpc("getTransaction",json!([signature,{"encoding":"jsonParsed","commitment":"confirmed","maxSupportedTransactionVersion":1}]));
    lab.record_probe(&reply);
    assert_eq!(
        reply["error"]["data"]["code"], "result_too_large",
        "{reply}"
    );
    lab.ready();
    assert_eq!(lab.rpc("getTransaction",json!([signature,{"encoding":"base64","commitment":"confirmed","maxSupportedTransactionVersion":1}]))["result"],binary["result"]);
}
