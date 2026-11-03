use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::TempDir;

fn cli(root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .current_dir(root)
        .args(["--json", "--project", root.to_str().unwrap()])
        .args(args)
        .env_remove("RONCHI_MODE")
        .env_remove("RONCHI_READ_UPSTREAM")
        .env_remove("RONCHI_SURFPOOL_DATASOURCE")
        .env_remove("RONCHI_INITIAL_CASE")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}
struct Lab {
    root: TempDir,
    id: String,
    address: String,
}
impl Lab {
    fn start(source: &str) -> Self {
        let root = TempDir::new().unwrap();
        let reply = cli(
            root.path(),
            &[
                "up",
                "--read-upstream",
                source,
                "--rpc-listen",
                "127.0.0.1:0",
                "--control-listen",
                "127.0.0.1:0",
            ],
        );
        Self {
            id: reply["lab_id"].as_str().unwrap().into(),
            address: reply["result"]["rpc_address"].as_str().unwrap().into(),
            root,
        }
    }
    fn rpc(&self, value: Value) -> Value {
        let bytes = serde_json::to_vec(&value).unwrap();
        let mut stream = TcpStream::connect(&self.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(stream, "POST / HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n", self.address, bytes.len()).unwrap();
        stream.write_all(&bytes).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
    }
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = Command::new(env!("CARGO_BIN_EXE_ronchi"))
            .args([
                "--json",
                "--project",
                self.root.path().to_str().unwrap(),
                "--lab-id",
                &self.id,
                "down",
            ])
            .output();
    }
}
struct Source {
    address: String,
    seen: Arc<Mutex<Vec<(String, Value)>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Source {
    fn start(reply: &'static str) -> Self {
        Self::http(200, reply.to_string(), String::new())
    }
    fn http(status: u16, reply: String, extra: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let saved = seen.clone();
        let thread = std::thread::spawn(move || {
            for incoming in listener.incoming() {
                let mut stream = incoming.unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap_or(0) == 0 {
                        return;
                    }
                    bytes.push(byte[0]);
                }
                let headers = String::from_utf8(bytes).unwrap();
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                saved
                    .lock()
                    .unwrap()
                    .push((headers, serde_json::from_slice(&body).unwrap()));
                write!(stream,"HTTP/1.1 {status} Status\r\nConnection: close\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\n\r\n{}",reply.len(),reply).unwrap();
            }
        });
        Self {
            address,
            seen,
            thread: Some(thread),
        }
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        let _ = TcpStream::connect(&self.address);
        self.thread.take().unwrap().join().unwrap();
    }
}
#[test]
fn explicit_read_through_and_final_boundary_write_firewall() {
    let source = Source::start(
        r#"{"jsonrpc":"2.0","id":"read","result":{"context":{"slot":987},"value":123}}"#,
    );
    let lab = Lab::start(&format!("http://{}/rpc", source.address));
    let result=lab.rpc(json!({"jsonrpc":"2.0","id":"read","method":"getBalance","params":["11111111111111111111111111111111"]}));
    assert_eq!(
        result["result"],
        json!({"context":{"slot":987},"value":123})
    );
    let denied = [
        "requestAirdrop",
        "simulateTransaction",
        "sendTransaction",
        "providerWrite",
        "getTransaction",
        "getSignatureStatuses",
        "getLatestBlockhash",
        "getFeeForMessage",
    ];
    let batch: Vec<_> = denied
        .iter()
        .enumerate()
        .map(|(i, method)| json!({"jsonrpc":"2.0","id":i,"method":method,"params":[]}))
        .collect();
    assert_eq!(
        lab.rpc(json!(batch)).as_array().unwrap().len(),
        denied.len()
    );
    let seen = source.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1["method"], "getBalance");
}

#[test]
fn credentials_are_used_only_for_source_transport_and_never_retained() {
    use base64::Engine;
    let source = Source::start(
        r#"{"jsonrpc":"2.0","id":"read","result":{"context":{"slot":42},"value":{"credential":"fixture:p@ss","token":"fixture-query-token","path":"fixture-path-token","truth":true,"empty":null}}}"#,
    );
    let url = format!(
        "http://fixture-user:fixture%3Ap%40ss@{}/fixture-path-token?api-key=fixture-query-token",
        source.address
    );
    let lab = Lab::start(&url);
    let reused = cli(lab.root.path(), &["up", "--read-upstream", &url]);
    assert_eq!(reused["lab_id"], lab.id);
    assert_eq!(
        reused["result"]["configuration"]["read_upstream"],
        ronchi::upstream::source_identity(&url)
    );
    let result=lab.rpc(json!({"jsonrpc":"2.0","id":999,"method":"getAccountInfo","params":["11111111111111111111111111111111"]}));
    assert_eq!(result["id"], 999);
    assert_eq!(result["result"]["value"]["credential"], "fixture:p@ss");
    {
        let seen = source.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let headers = &seen[0].0;
        assert!(
            headers.starts_with("POST /fixture-path-token?api-key=fixture-query-token HTTP/1.1")
        );
        assert!(!headers.contains("fixture-user:"));
        let expected = format!(
            "authorization: Basic {}",
            base64::engine::general_purpose::STANDARD.encode("fixture-user:fixture:p@ss")
        );
        assert!(headers.contains(&expected));
    }
    cli(lab.root.path(), &["--lab-id", &lab.id, "down"]);
    let reader = ronchi::journal::JournalReader::open(
        &lab.root
            .path()
            .join("ronchi/labs")
            .join(&lab.id)
            .join("journal.sqlite"),
    )
    .unwrap();
    let snapshot = reader.snapshot(false).unwrap();
    let mut source_facts = 0;
    for seq in 1..=snapshot.snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        let encoded = serde_json::to_string(&event).unwrap();
        for secret in [
            "fixture-user",
            "fixture:p@ss",
            "fixture%3Ap%40ss",
            "fixture-query-token",
            "fixture-path-token",
        ] {
            assert!(
                !encoded.contains(secret),
                "unredacted fixture credential at {seq}"
            );
        }
        if event.kind == "rpc.upstream" {
            source_facts += 1;
            assert_eq!(event.source, "read_upstream");
            assert_eq!(event.data["redacted"], true);
            assert_eq!(event.data["result"]["value"]["credential"], "[redacted]");
            assert_eq!(event.data["result"]["context"]["slot"], 42);
        }
    }
    assert_eq!(source_facts, 1);
    for file in ["launch.json", "manifest.json"] {
        let content =
            std::fs::read_to_string(lab.root.path().join("ronchi/labs").join(&lab.id).join(file))
                .unwrap();
        assert!(!content.contains("fixture-"));
    }
}

#[test]
fn each_allowlisted_read_preserves_source_context_and_local_history_never_falls_back() {
    let source = Source::start(
        r#"{"jsonrpc":"2.0","id":"read","result":{"context":{"slot":700,"apiVersion":"fixture"},"value":null}}"#,
    );
    let lab = Lab::start(&format!("http://{}/", source.address));
    let request = |method: &str, params: Value| json!({"jsonrpc":"2.0","id":method,"method":method,"params":params});
    for method in ronchi::upstream::READ_METHODS {
        let response = lab.rpc(request(method, json!([])));
        assert_eq!(response["id"], method);
        assert_eq!(response["result"]["context"]["slot"], 700);
    }
    let unknown = bs58::encode([77u8; 64]).into_string();
    assert!(
        lab.rpc(request(
            "getTransaction",
            json!([unknown,{"commitment":"confirmed"}])
        ))["result"]
            .is_null()
    );
    assert_eq!(
        lab.rpc(request(
            "getSignatureStatuses",
            json!([[unknown],{"searchTransactionHistory":true}])
        ))["result"]["value"],
        json!([null])
    );
    let genesis = lab.rpc(request("getGenesisHash", json!([])));
    assert_eq!(
        bs58::decode(genesis["result"].as_str().unwrap())
            .into_vec()
            .unwrap()
            .len(),
        32
    );
    assert_eq!(
        lab.rpc(request("getVersion", json!([])))["result"]["ronchi"]["profile"],
        "local-simulator-v1"
    );
    assert!(lab.rpc(request("getLatestBlockhash",json!([])))["result"]["value"]["lastValidBlockHeight"].is_u64());
    assert_eq!(source.seen.lock().unwrap().len(), 7);
}

#[test]
fn redirects_are_not_followed_and_source_failures_do_not_poison_the_local_lab() {
    let target = Source::start(r#"{"jsonrpc":"2.0","id":"read","result":100}"#);
    let source = Source::http(
        307,
        "private fixture upstream error".into(),
        format!("Location: http://{}/\r\n", target.address),
    );
    let lab = Lab::start(&format!("http://{}/", source.address));
    let response = lab.rpc(json!({"jsonrpc":"2.0","id":1,"method":"getBalance","params":[]}));
    assert_eq!(response["error"]["data"]["code"], "upstream_redirect");
    assert_eq!(source.seen.lock().unwrap().len(), 1);
    assert!(target.seen.lock().unwrap().is_empty());
    assert_eq!(
        lab.rpc(json!({"jsonrpc":"2.0","id":2,"method":"getHealth"}))["result"],
        "ok"
    );
    assert_eq!(
        cli(lab.root.path(), &["status"])["result"]["health"],
        "healthy"
    );
}

#[test]
fn large_source_results_are_preserved_in_bounded_chunks_and_byte_overflow_is_explicit() {
    use base64::Engine;
    let result = json!({"context":{"slot":555},"value":"x".repeat(120*1024)});
    let source = Source::http(
        200,
        json!({"jsonrpc":"2.0","id":"read","result":result}).to_string(),
        String::new(),
    );
    let lab = Lab::start(&format!("http://{}/", source.address));
    assert_eq!(
        lab.rpc(json!({"jsonrpc":"2.0","id":1,"method":"getMultipleAccounts","params":[]}))["result"],
        result
    );
    cli(lab.root.path(), &["--lab-id", &lab.id, "down"]);
    let reader = ronchi::journal::JournalReader::open(
        &lab.root
            .path()
            .join("ronchi/labs")
            .join(&lab.id)
            .join("journal.sqlite"),
    )
    .unwrap();
    let mut bytes = Vec::new();
    let mut count = 0;
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        assert!(serde_json::to_vec(&event).unwrap().len() <= ronchi::journal::MAX_EVENT_BYTES);
        if event.kind == "rpc.upstream"
            && let Some(encoded) = event.data["bytes"].as_str()
        {
            assert_eq!(event.data["result_chunk"], count);
            count += 1;
            bytes.extend(
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .unwrap(),
            );
        }
    }
    assert!(count > 1);
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), result);
    let too_large = Source::http(
        200,
        json!({"jsonrpc":"2.0","id":"read","result":"y".repeat(ronchi::rpc::MAX_RESPONSE_BYTES)})
            .to_string(),
        String::new(),
    );
    let lab = Lab::start(&format!("http://{}/", too_large.address));
    let error = lab.rpc(json!({"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":[]}));
    assert_eq!(error["error"]["data"]["code"], "upstream_response_bytes");
    assert_eq!(
        lab.rpc(json!({"jsonrpc":"2.0","id":1,"method":"getHealth"}))["result"],
        "ok"
    );
}

#[test]
fn malformed_and_rpc_errors_remain_classified_source_facts_without_arbitrary_text() {
    for (body, code) in [
        ("not json", "upstream_json"),
        (
            r#"{"jsonrpc":"2.0","id":"wrong","result":null}"#,
            "upstream_envelope",
        ),
        (
            r#"{"jsonrpc":"2.0","id":"read","error":{"code":-32016,"message":"fixture credential-bearing arbitrary error","data":{"private":"fixture private text"}}}"#,
            "upstream_rpc_error",
        ),
    ] {
        let source = Source::http(200, body.into(), String::new());
        let lab = Lab::start(&format!("http://{}/", source.address));
        let response = lab.rpc(
            json!({"jsonrpc":"2.0","id":1,"method":"getRecentPrioritizationFees","params":[]}),
        );
        assert_eq!(response["error"]["data"]["code"], code);
        assert!(!response.to_string().contains("fixture"));
        cli(lab.root.path(), &["--lab-id", &lab.id, "down"]);
        let reader = ronchi::journal::JournalReader::open(
            &lab.root
                .path()
                .join("ronchi/labs")
                .join(&lab.id)
                .join("journal.sqlite"),
        )
        .unwrap();
        for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
            let event = reader.event(seq).unwrap().unwrap();
            assert!(
                !serde_json::to_string(&event)
                    .unwrap()
                    .contains("fixture credential")
            );
        }
    }
}

#[test]
fn force_stop_cancels_an_inflight_source_and_retains_the_known_dispatch_boundary() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sent, received) = std::sync::mpsc::channel();
    let source = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        let mut headers = Vec::new();
        let mut byte = [0];
        while !headers.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            headers.push(byte[0]);
        }
        let headers = String::from_utf8(headers).unwrap();
        let length: usize = headers
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap();
        let mut body = vec![0; length];
        stream.read_exact(&mut body).unwrap();
        sent.send(()).unwrap();
        let mut remaining = Vec::new();
        stream.read_to_end(&mut remaining).unwrap();
        assert!(remaining.is_empty());
    });
    let lab = Lab::start(&format!("http://{address}/"));
    let mut rpc = TcpStream::connect(&lab.address).unwrap();
    let body = json!({"jsonrpc":"2.0","id":1,"method":"getBalance","params":[]}).to_string();
    write!(
        rpc,
        "POST / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
        lab.address,
        body.len(),
        body
    )
    .unwrap();
    received.recv_timeout(Duration::from_secs(4)).unwrap();
    let before = std::time::Instant::now();
    cli(lab.root.path(), &["--lab-id", &lab.id, "down", "--force"]);
    assert!(before.elapsed() < Duration::from_secs(5));
    source.join().unwrap();
    let reader = ronchi::journal::JournalReader::open(
        &lab.root
            .path()
            .join("ronchi/labs")
            .join(&lab.id)
            .join("journal.sqlite"),
    )
    .unwrap();
    let mut dispatched = false;
    let mut finished = false;
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        if event.kind == "rpc.upstream_started" {
            dispatched = true;
            assert_eq!(event.data["authority"], "read_upstream");
        }
        if event.kind == "rpc.upstream" {
            finished = true;
        }
    }
    assert!(dispatched);
    assert!(!finished, "cancelled source must not invent its result");
}

#[test]
fn a_tls_handshake_failure_has_no_plain_http_or_public_fallback() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let source = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = stream.write_all(b"HTTP/1.1 400 not TLS\r\n\r\n");
    });
    let lab = Lab::start(&format!(
        "https://{address}/private-tls-path?token=private-token"
    ));
    let response = lab.rpc(json!({"jsonrpc":"2.0","id":1,"method":"getBalance","params":[]}));
    assert_eq!(response["error"]["data"]["code"], "upstream_transport");
    assert!(!response.to_string().contains("private-token"));
    source.join().unwrap();
    assert_eq!(
        lab.rpc(json!({"jsonrpc":"2.0","id":1,"method":"getHealth"}))["result"],
        "ok"
    );
}

#[test]
fn short_path_and_query_credentials_cannot_expand_or_leak_retained_evidence() {
    let mut values = vec![json!("z"); 50_000];
    values[0] = json!("secret");
    let result = json!(values);
    let source = Source::http(
        200,
        json!({"jsonrpc":"2.0","id":"read","result":result}).to_string(),
        String::new(),
    );
    let lab = Lab::start(&format!("http://{}/secret?token=z", source.address));
    assert_eq!(
        lab.rpc(json!({"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":[]}))["result"],
        result
    );
    assert_eq!(
        cli(lab.root.path(), &["status"])["result"]["health"],
        "healthy"
    );
    cli(lab.root.path(), &["--lab-id", &lab.id, "down"]);
    let reader = ronchi::journal::JournalReader::open(
        &lab.root
            .path()
            .join("ronchi/labs")
            .join(&lab.id)
            .join("journal.sqlite"),
    )
    .unwrap();
    let mut facts = 0;
    for seq in 1..=reader.snapshot(false).unwrap().snapshot_seq {
        let event = reader.event(seq).unwrap().unwrap();
        let encoded = serde_json::to_vec(&event).unwrap();
        assert!(encoded.len() < ronchi::journal::MAX_EVENT_BYTES);
        if event.kind == "rpc.upstream" {
            facts += 1;
            assert_eq!(event.data["redacted"], true);
            assert_eq!(event.data["result"]["availability"], "redacted");
            assert_eq!(
                event.data["result"]["reason"],
                "credential_redaction_byte_budget"
            );
            assert!(!String::from_utf8(encoded).unwrap().contains("secret"));
        }
    }
    assert_eq!(facts, 1);
}

#[test]
fn local_signed_acceptance_and_mixed_batches_cannot_acquire_write_forwarding_permission() {
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    let source = Source::start(
        r#"{"jsonrpc":"2.0","id":"read","result":{"context":{"slot":901},"value":22}}"#,
    );
    let lab = Lab::start(&format!("http://{}/", source.address));
    let hash=lab.rpc(json!({"jsonrpc":"2.0","id":1,"method":"getLatestBlockhash","params":[{"commitment":"processed"}]}))["result"]["value"]["blockhash"].as_str().unwrap().to_string();
    let signer = SigningKey::from_bytes(&[119; 32]);
    let mut message = vec![1, 0, 0, 1];
    message.extend_from_slice(&signer.verifying_key().to_bytes());
    message.extend(bs58::decode(hash).into_vec().unwrap());
    message.push(0);
    let signature = signer.sign(&message).to_bytes();
    let mut wire = vec![1];
    wire.extend_from_slice(&signature);
    wire.extend(message);
    let signature = bs58::encode(signature).into_string();
    let batch = json!([
        {"jsonrpc":"2.0","method":"getBalance","params":["11111111111111111111111111111111"]},
        {"jsonrpc":"2.0","id":"write","method":"sendTransaction","params":[base64::engine::general_purpose::STANDARD.encode(wire),{"encoding":"base64","skipPreflight":true}]},
        {"jsonrpc":"2.0","id":"account","method":"getBalance","params":["11111111111111111111111111111111"]},
        {"jsonrpc":"2.0","id":"unknown","method":"providerWrite","params":[]}
    ]);
    let reply = lab.rpc(batch);
    assert_eq!(reply.as_array().unwrap().len(), 3);
    assert_eq!(reply[0]["id"], "write");
    assert_eq!(reply[0]["result"], signature);
    assert_eq!(reply[1]["id"], "account");
    assert_eq!(reply[1]["result"]["value"], 22);
    assert_eq!(reply[2]["error"]["code"], -32601);
    let statuses=lab.rpc(json!({"jsonrpc":"2.0","id":"local","method":"getSignatureStatuses","params":[[signature],{"searchTransactionHistory":true}]}));
    assert_eq!(statuses["result"]["value"], json!([null]));
    let seen = source.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter()
            .all(|(_, request)| request["method"] == "getBalance")
    );
    drop(seen);
    cli(lab.root.path(), &["--lab-id", &lab.id, "down", "--force"]);
}
