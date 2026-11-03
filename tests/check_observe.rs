mod reference_support;
use reference_support::{base, command};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Stdio,
    time::Duration,
};
fn request(stream: &mut std::net::TcpStream) -> Value {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        stream.read_exact(&mut b).unwrap();
        headers.push(b[0]);
        assert!(headers.len() < 32768);
    }
    let length = String::from_utf8(headers)
        .unwrap()
        .lines()
        .find_map(|l| {
            l.to_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
#[test]
fn cli_collects_application_evidence_and_retains_it_after_stop() {
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = source.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = source.accept().unwrap();
        let req = request(&mut stream);
        assert_eq!(req["facts"], json!(["enabled"]));
        let body=json!({"contract_version":1,"invocation_id":req["invocation_id"],"source":"independent","schema_version":1,"observed_at":"2026-11-03T00:00:00Z","revision":"r1","facts":{"enabled":{"availability":"available","value":false}}}).to_string();
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .unwrap();
    });
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("ronchi")).unwrap();
    std::fs::write(temp.path().join("ronchi/config.toml"), format!("schema_version = 1\n[[checks.sources]]\ncheck_id = 'state'\nversion = 1\nsource = 'independent'\nschema_version = 1\nurl = 'http://{address}/observe'\n[checks.sources.facts.enabled]\ntype = 'boolean'\n")).unwrap();
    let up = command(
        temp.path(),
        &[
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ],
    );
    let result = command(
        temp.path(),
        &["check", "observe", "state", "--intent-id", "W1"],
    );
    assert_eq!(result["result"]["status"], "completed", "{result}");
    assert_eq!(result["result"]["observations"], 1);
    let seq = result["result"]["last_invocation"]["completion_ref"]["event_seq"]
        .as_u64()
        .unwrap()
        .to_string();
    let evidence = command(
        temp.path(),
        &[
            "investigate",
            "evidence",
            "--event-seq",
            &seq,
            "--include-payload",
        ],
    );
    assert!(evidence.to_string().contains("independent"), "{evidence}");
    server.join().unwrap();
    command(temp.path(), &["down"]);
    let retained = command(
        temp.path(),
        &[
            "--lab-id",
            up["lab_id"].as_str().unwrap(),
            "investigate",
            "evidence",
            "--event-seq",
            &seq,
            "--include-payload",
        ],
    );
    assert!(retained.to_string().contains("enabled"));
    let rejected = base(temp.path())
        .args(["check", "observe", "state", "--intent-id", "W1"])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    let schema = base(temp.path())
        .args(["schema", "check.observe"])
        .stdout(Stdio::piped())
        .output()
        .unwrap();
    assert!(schema.status.success());
}

fn configured_project(address: std::net::SocketAddr) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("ronchi")).unwrap();
    std::fs::write(temp.path().join("ronchi/config.toml"), format!("schema_version = 1\n[[checks.sources]]\ncheck_id = 'state'\nversion = 1\nsource = 'independent'\nschema_version = 1\nurl = 'http://{address}/observe'\n[checks.sources.facts.enabled]\ntype = 'boolean'\n")).unwrap();
    command(
        temp.path(),
        &[
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ],
    );
    temp
}
fn wait_exit(child: &mut std::process::Child) -> std::process::ExitStatus {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("collection process did not exit within its cleanup budget");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn process_cancellation_stops_future_http_polls_and_lab_stop_joins_inflight_work() {
    for cause in ["signal", "pipe", "stop"] {
        let source = TcpListener::bind("127.0.0.1:0").unwrap();
        let temp = configured_project(source.local_addr().unwrap());
        let mut child = base(temp.path())
            .args([
                "check",
                "observe",
                "state",
                "--intent-id",
                "W1",
                "--poll",
                "--horizon-seconds",
                "60",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // Receiving the real POST is the barrier. Keep its body response blocked.
        let (mut stream, _) = source.accept().unwrap();
        let req = request(&mut stream);
        assert_eq!(req["check_id"], "state");
        match cause {
            "signal" => {
                assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
            }
            "pipe" => {
                drop(child.stdout.take());
            }
            "stop" => {
                command(temp.path(), &["down"]);
            }
            _ => unreachable!(),
        }
        let status = wait_exit(&mut child);
        if cause == "signal" {
            assert_eq!(status.code(), Some(130));
        }
        if cause == "pipe" {
            assert!(status.success());
        }
        // Cancellation closes the owned transport, rather than only stopping CLI polling.
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
        // Observe a full minimum poll interval after terminal cleanup. The source stays alive.
        source.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_millis(1200);
        while std::time::Instant::now() < deadline {
            assert!(
                matches!(source.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let evidence = command(
            temp.path(),
            &[
                "investigate",
                "timeline",
                "--kind",
                "check.collection_completed",
            ],
        );
        assert!(
            evidence.to_string().contains("cancelled"),
            "{cause}: {evidence}"
        );
        if cause != "stop" {
            command(temp.path(), &["down"]);
        }
    }
}

#[test]
fn public_source_failures_are_gaps_without_retries_partial_facts_or_verdicts() {
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = source.local_addr().unwrap();
    let temp = configured_project(address);
    // Reconfigure through the existing config and a new Lab, before any observations.
    command(temp.path(), &["down"]);
    let config = temp.path().join("ronchi/config.toml");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str("\n[checks.sources.limits]\ntimeout_ms = 250\n");
    std::fs::write(config, text).unwrap();
    command(
        temp.path(),
        &[
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ],
    );
    let scenarios = [
        "unavailable",
        "json",
        "type",
        "identity",
        "source",
        "schema",
        "redirect",
        "chunked",
        "slow",
    ];
    let server = std::thread::spawn(move || {
        for scenario in scenarios {
            let (mut stream, _) = source.accept().unwrap();
            let req = request(&mut stream);
            // Exactly one network attempt per explicit collection, in this order.
            assert_eq!(req["scope"]["intent_ids"][0], scenario);
            let mut body = json!({"contract_version":1,"invocation_id":req["invocation_id"],"source":"independent","schema_version":1,"observed_at":"2026-11-03T00:00:00Z","facts":{"enabled":{"availability":"available","value":false}}});
            match scenario {
                "unavailable" => {
                    body["facts"]["enabled"] =
                        json!({"availability":"unavailable","reason":"not_observed"})
                }
                "type" => body["facts"]["enabled"]["value"] = json!("false"),
                "identity" => body["invocation_id"] = json!("someone-else"),
                "source" => body["source"] = json!("other"),
                "schema" => body["schema_version"] = json!(2),
                "redirect" => {
                    stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://{address}/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
                    continue;
                }
                "chunked" => {
                    stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").unwrap();
                    for _ in 0..10 {
                        if stream
                            .write_all(format!("1000\r\n{}\r\n", "x".repeat(4096)).as_bytes())
                            .is_err()
                        {
                            break;
                        }
                    }
                    continue;
                }
                "slow" => {
                    stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").unwrap();
                    for _ in 0..8 {
                        if stream.write_all(b"1\r\n \r\n").is_err() {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(80));
                    }
                    continue;
                }
                _ => (),
            }
            let body = if scenario == "json" {
                "{partial".into()
            } else {
                body.to_string()
            };
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .unwrap();
        }
        source.set_nonblocking(true).unwrap();
        assert!(source.accept().is_err());
    });
    for scenario in scenarios {
        let result = command(
            temp.path(),
            &["check", "observe", "state", "--intent-id", scenario],
        );
        assert_eq!(
            result["result"]["status"], "completed_with_gaps",
            "{scenario}: {result}"
        );
        assert_eq!(result["result"]["invocations"], 1);
        assert_eq!(
            result["result"]["observations"],
            if scenario == "unavailable" { 1 } else { 0 }
        );
        assert_eq!(result["result"]["gaps"], 1);
        assert!(result.get("verdict").is_none());
        let gap = result["result"]["last_invocation"]["evidence_gap"].to_string();
        if scenario == "slow" {
            assert!(gap.contains("source_timeout"), "{gap}");
        }
        if scenario == "chunked" {
            assert!(gap.contains("source_response_bytes"), "{gap}");
        }
        if scenario == "redirect" {
            assert!(gap.contains("source_redirect"), "{gap}");
        }
    }
    server.join().unwrap();
    let observations = command(
        temp.path(),
        &["investigate", "timeline", "--kind", "check.observation"],
    );
    assert_eq!(observations["result"]["items"].as_array().unwrap().len(), 1);
    command(temp.path(), &["down"]);
}

#[test]
fn generic_collector_reads_real_relayer_mapping_without_advancing_its_worker() {
    use reference_support::Lab;
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserved.local_addr().unwrap();
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("ronchi")).unwrap();
    std::fs::write(temp.path().join("ronchi/config.toml"), format!("schema_version = 1\n[[checks.sources]]\ncheck_id = 'mapping'\nversion = 1\nsource = 'reference-relayer'\nschema_version = 1\nurl = 'http://{address}/checks/observe'\n[checks.sources.facts.intent]\ntype = 'record'\n[checks.sources.facts.intent.fields.intent_id]\ntype = 'string'\nmax_bytes = 128\n[checks.sources.facts.intent.fields.original_signature]\ntype = 'availability'\n[checks.sources.facts.intent.fields.original_signature.value]\ntype = 'string'\nmax_bytes = 88\n")).unwrap();
    let up = command(
        temp.path(),
        &[
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ],
    );
    let lab = Lab {
        directory: temp,
        id: up["lab_id"].as_str().unwrap().into(),
        rpc: up["result"]["rpc_address"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
    };
    drop(reserved);
    let mut relayer = lab.start_at("resilient", Some("prepared"), None, None, Some(address));
    assert_eq!(
        relayer
            .request("POST", "/intents", reference_support::input())
            .0,
        200
    );
    lab.barrier("prepared");
    let before = relayer.intent();
    let signature = before["original_signature"]["value"].as_str().unwrap();
    let result = lab.command(&["check", "observe", "mapping", "--intent-id", "W1"]);
    assert_eq!(result["result"]["status"], "completed", "{result}");
    let seq = result["result"]["last_invocation"]["completion_ref"]["event_seq"]
        .as_u64()
        .unwrap()
        .to_string();
    let evidence = lab.command(&[
        "investigate",
        "evidence",
        "--event-seq",
        &seq,
        "--include-payload",
    ]);
    assert!(evidence.to_string().contains(signature), "{evidence}");
    assert!(evidence.to_string().contains("reference-relayer"));
    assert_eq!(relayer.intent(), before);
    let old_seq = result["result"]["start_ref"]["event_seq"]
        .as_u64()
        .unwrap()
        .to_string();
    let old = lab.command(&[
        "investigate",
        "timeline",
        "--kind",
        "check.observation",
        "--snapshot-seq",
        &old_seq,
    ]);
    assert_eq!(old["result"]["items"], json!([]));
    assert!(
        !old["view"]["observation_boundary"]
            .as_array()
            .unwrap()
            .contains(&json!("application_check"))
    );
    assert!(
        evidence["view"]["observation_boundary"]
            .as_array()
            .unwrap()
            .contains(&json!("application_check"))
    );
    assert!(!evidence.to_string().contains("signed_bytes"));
    relayer.stop();
}

fn control(
    project: &std::path::Path,
    up: &Value,
    method: &str,
    suffix: &str,
    body: Value,
) -> Value {
    let id = up["lab_id"].as_str().unwrap();
    let address = up["result"]["control_address"].as_str().unwrap();
    let token =
        std::fs::read_to_string(project.join("ronchi/labs").join(id).join("token")).unwrap();
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let text = body.to_string();
    stream.write_all(format!("{method} /v1/labs/{id}/{suffix} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", token.trim(), text.len()).as_bytes()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let split = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
    serde_json::from_slice(&bytes[split..]).unwrap()
}
#[test]
fn authenticated_collection_enforces_capacity_normalized_scope_and_skips_busy_polls() {
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    let temp = configured_project(source.local_addr().unwrap());
    command(temp.path(), &["down"]);
    let config = temp.path().join("ronchi/config.toml");
    let text = std::fs::read_to_string(&config).unwrap().replace(
        "[[checks.sources]]",
        "[checks]\nconcurrency = 1\n[[checks.sources]]",
    );
    std::fs::write(config, text).unwrap();
    let up = command(
        temp.path(),
        &[
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ],
    );
    let start = |collection: &str, ids: Value, poll: bool| {
        control(
            temp.path(),
            &up,
            "POST",
            "checks/observe",
            json!({"contract_version":1,"operation":"check.observe","request_id":collection,"lab_id":up["lab_id"],"arguments":{"collection_id":collection,"check_id":"state","scope":{"intent_ids":ids},"poll":poll,"horizon_seconds":if poll {Some(2)} else {None}}}),
        )
    };
    assert_eq!(
        start("C1", json!(["W2", "W1"]), false)["result"]["status"],
        "running"
    );
    let (mut stream, _) = source.accept().unwrap();
    request(&mut stream);
    assert_eq!(
        start("C2", json!(["W1", "W2", "W1"]), false)["error"]["code"],
        "state_conflict"
    );
    assert_eq!(
        start("C3", json!(["W3"]), false)["error"]["code"],
        "capacity_exceeded"
    );
    assert_eq!(
        start("C4", json!(["W4"]), true)["result"]["status"],
        "running"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    loop {
        let report = control(temp.path(), &up, "GET", "check-collections/C4", Value::Null);
        if report["result"]["status"] != "running" {
            assert_eq!(report["result"]["invocations"], 0);
            assert!(report["result"]["skipped"].as_u64().unwrap() >= 1);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    control(
        temp.path(),
        &up,
        "POST",
        "check-collections/C1/cancel",
        json!({}),
    );
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut b = [0];
    assert_eq!(stream.read(&mut b).unwrap(), 0);
    source.set_nonblocking(true).unwrap();
    assert!(source.accept().is_err());
    command(temp.path(), &["down"]);
}

#[test]
fn runtime_credentials_and_raw_source_errors_never_enter_public_evidence() {
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = source.local_addr().unwrap();
    let secret = "private-check-token-987654321";
    let server = std::thread::spawn(move || {
        for number in 0..2 {
            let (mut stream, _) = source.accept().unwrap();
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                stream.read_exact(&mut b).unwrap();
                bytes.push(b[0]);
            }
            let headers = String::from_utf8(bytes).unwrap();
            assert!(
                headers
                    .to_lowercase()
                    .contains(&format!("authorization: bearer {secret}"))
            );
            let length = headers
                .lines()
                .find_map(|s| {
                    s.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            let req: Value = serde_json::from_slice(&body).unwrap();
            let body = if number == 0 {
                json!({"contract_version":1,"invocation_id":req["invocation_id"],"source":"independent","schema_version":1,"observed_at":"2026-11-03T00:00:00Z","revision":secret,"facts":{"echo":{"availability":"available","value":secret}}}).to_string()
            } else {
                format!("raw failure body {secret}")
            };
            let status = if number == 0 {
                "200 OK"
            } else {
                "500 Internal Server Error"
            };
            stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
        }
    });
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("ronchi")).unwrap();
    std::fs::write(temp.path().join("ronchi/config.toml"),format!("schema_version = 1\n[[checks.sources]]\ncheck_id = 'state'\nversion = 1\nsource = 'independent'\nschema_version = 1\nurl = 'http://{address}/observe'\nauthorization_env = 'RONCHI_CHECK_TEST_AUTH'\n[checks.sources.facts.echo]\ntype = 'string'\nmax_bytes = 128\n")).unwrap();
    let up = base(temp.path())
        .env("RONCHI_CHECK_TEST_AUTH", format!("Bearer {secret}"))
        .args([
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ])
        .output()
        .unwrap();
    assert!(
        up.status.success(),
        "{}",
        String::from_utf8_lossy(&up.stdout)
    );
    for id in ["W1", "W2"] {
        let result = command(
            temp.path(),
            &["check", "observe", "state", "--intent-id", id],
        );
        assert_eq!(result["result"]["status"], "completed_with_gaps");
        assert!(!result.to_string().contains(secret));
    }
    command(temp.path(), &["down"]);
    server.join().unwrap();
    let evidence = command(
        temp.path(),
        &[
            "investigate",
            "evidence",
            "--include-payload",
            "--limit",
            "100",
        ],
    );
    let text = evidence.to_string();
    assert!(!text.contains(secret));
    assert!(!text.contains("raw failure body"));
    assert!(text.contains("redacted"));
    assert!(!String::from_utf8_lossy(&up.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&up.stderr).contains(secret));
}

#[test]
fn complete_observation_can_be_read_in_bounded_evidence_chunks() {
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    let temp = configured_project(source.local_addr().unwrap());
    command(temp.path(), &["down"]);
    let config = temp.path().join("ronchi/config.toml");
    let text = std::fs::read_to_string(&config)
        .unwrap()
        .replace("type = 'boolean'", "type = 'string'\nmax_bytes = 20000");
    std::fs::write(config, text).unwrap();
    command(
        temp.path(),
        &[
            "up",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ],
    );
    let value = "Я".repeat(6000);
    let expected = value.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = source.accept().unwrap();
        let req = request(&mut stream);
        let body = json!({"contract_version":1,"invocation_id":req["invocation_id"],"source":"independent","schema_version":1,"observed_at":"2026-11-03T00:00:00Z","facts":{"enabled":{"availability":"available","value":value}}}).to_string();
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .unwrap();
    });
    let result = command(
        temp.path(),
        &["check", "observe", "state", "--intent-id", "W1"],
    );
    assert_eq!(result["result"]["status"], "completed");
    let seq = result["result"]["last_invocation"]["completion_ref"]["event_seq"]
        .as_u64()
        .unwrap()
        .to_string();
    let mut offset = 0;
    let mut retained = String::new();
    loop {
        let evidence = command(
            temp.path(),
            &[
                "investigate",
                "evidence",
                "--event-seq",
                &seq,
                "--include-payload",
                "--payload-offset",
                &offset.to_string(),
            ],
        );
        let payload = &evidence["result"]["items"][0]["payload"];
        let chunk = payload["preview"].as_str().unwrap();
        assert!(chunk.len() <= 4096);
        retained.push_str(chunk);
        if let Some(next) = payload["next_offset"].as_u64() {
            assert!(next > offset);
            offset = next;
        } else {
            break;
        }
    }
    let saved: Value = serde_json::from_str(&retained).unwrap();
    assert_eq!(saved["facts"]["enabled"]["value"], expected);
    server.join().unwrap();
    command(temp.path(), &["down"]);
}

#[test]
fn explicit_polling_uses_distinct_invocations_and_fixed_horizon_without_overlap() {
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    let temp = configured_project(source.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut ids = Vec::new();
        let mut watermarks = Vec::new();
        let mut previous_completed = None;
        for _ in 0..2 {
            let (mut stream, _) = source.accept().unwrap();
            let now = std::time::Instant::now();
            if let Some(completed) = previous_completed {
                assert!(now.duration_since(completed) >= Duration::from_millis(900));
            }
            let req = request(&mut stream);
            ids.push(req["invocation_id"].clone());
            watermarks.push(req["snapshot_seq"].as_u64().unwrap());
            // Hold the source response to make overlapping polls observable.
            std::thread::sleep(Duration::from_millis(1100));
            let body = json!({"contract_version":1,"invocation_id":req["invocation_id"],"source":"independent","schema_version":1,"observed_at":"2026-11-03T00:00:00Z","facts":{"enabled":{"availability":"available","value":true}}}).to_string();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .unwrap();
            previous_completed = Some(std::time::Instant::now());
        }
        assert_ne!(ids[0], ids[1]);
        assert!(watermarks[1] > watermarks[0]);
        source.set_nonblocking(true).unwrap();
        assert!(source.accept().is_err());
    });
    let start = std::time::Instant::now();
    let result = command(
        temp.path(),
        &[
            "check",
            "observe",
            "state",
            "--intent-id",
            "W1",
            "--poll",
            "--horizon-seconds",
            "4",
        ],
    );
    assert_eq!(result["result"]["status"], "completed", "{result}");
    assert_eq!(result["result"]["invocations"], 2);
    assert_eq!(result["result"]["observations"], 2);
    assert!(start.elapsed() < Duration::from_secs(6));
    server.join().unwrap();
    command(temp.path(), &["down"]);
}
