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

