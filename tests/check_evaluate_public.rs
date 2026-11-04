mod reference_support;
use reference_support::{Lab, base, command};
use ronchi::checks::{Config, Definition};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};
fn incoming(stream: &mut TcpStream) -> Value {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        headers.push(byte[0]);
        assert!(headers.len() < 16384);
    }
    let length = String::from_utf8(headers)
        .unwrap()
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|s| s.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}
fn control(lab: &Lab, snapshot: u64, selected_lab: &str) -> Value {
    let status = lab.command(&["status"]);
    let address = status["result"]["control_address"].as_str().unwrap();
    let token = std::fs::read_to_string(lab.path().join("ronchi/labs").join(&lab.id).join("token"))
        .unwrap();
    let body=json!({"contract_version":1,"request_id":"api-evaluation","lab_id":selected_lab,"operation":"check.evaluate","arguments":{"snapshot_seq":snapshot,"evaluation":{"check_id":"custom-state","definition_version":7,"scope":{"intent_ids":["W1"]}}}}).to_string();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(format!("POST /v1/labs/{}/checks/evaluate HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",lab.id,token.trim(),body.len()).as_bytes()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let position = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
    serde_json::from_slice(&bytes[position..]).unwrap()
}
#[test]
fn public_evaluation_is_credential_free_read_only_and_never_calls_the_http_source() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let source = listener.try_clone().unwrap();
    let blob = format!("{}END-OF-COMPLETE-FACT", "Ж".repeat(5500));
    let expected = blob.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = source.accept().unwrap();
        let request = incoming(&mut stream);
        let body=json!({"contract_version":1,"invocation_id":request["invocation_id"],"source":"unrelated-application","schema_version":3,"observed_at":"2026-11-03T00:00:00Z","revision":"r1","facts":{"blob":{"availability":"available","value":blob},"terminal":{"availability":"available","value":true}}}).to_string();
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
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("ronchi")).unwrap();
    let definition:Definition=serde_json::from_value(json!({"check_id":"custom-state","version":7,"source":"unrelated-application","schema_version":3,"url":format!("http://{address}/readonly"),"facts":{"blob":{"type":"string","max_bytes":14000},"terminal":{"type":"boolean"}},"requirements":[{"rule_id":"exact-complete-value","version":1,"pass":{"compare":{"path":"blob","relation":"eq","value":expected}},"fail":{"compare":{"path":"blob","relation":"eq","value":"forbidden"}},"complete":{"compare":{"path":"terminal","relation":"eq","value":true}}}]})).unwrap();
    #[derive(serde::Serialize)]
    struct Configuration {
        schema_version: u32,
        checks: Config,
    }
    std::fs::write(
        directory.path().join("ronchi/config.toml"),
        toml::to_string(&Configuration {
            schema_version: 1,
            checks: Config {
                concurrency: 4,
                sources: vec![definition],
            },
        })
        .unwrap(),
    )
    .unwrap();
    let up = command(
        directory.path(),
        &[
            "up",
            "--detach",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ],
    );
    let lab = Lab {
        directory,
        id: up["lab_id"].as_str().unwrap().into(),
        rpc: up["result"]["rpc_address"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
    };
    let initial = lab.command(&[
        "check",
        "evaluate",
        "custom-state",
        "--definition-version",
        "7",
        "--snapshot-seq",
        &up["result"]["snapshot_seq"].as_u64().unwrap().to_string(),
        "--intent-id",
        "W1",
    ]);
    assert_eq!(initial["result"]["verdict"], "inconclusive");
    let observation = lab.command(&["check", "observe", "custom-state", "--intent-id", "W1"]);
    assert_eq!(observation["result"]["status"], "completed");
    server.join().unwrap();
    listener.set_nonblocking(true).unwrap();
    let snapshot = observation["result"]["snapshot_seq"].as_u64().unwrap();
    let args = [
        "check",
        "evaluate",
        "custom-state",
        "--definition-version",
        "7",
        "--snapshot-seq",
        &snapshot.to_string(),
        "--intent-id",
        "W1",
    ];
    let cli = lab.command(&args);
    assert_eq!(cli["result"]["verdict"], "pass", "{cli}");
    assert_eq!(control(&lab, snapshot, &lab.id)["result"], cli["result"]);
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    let wrong_lab = control(&lab, snapshot, "other-lab");
    assert_eq!(wrong_lab["error"]["code"], "invalid_argument");
    assert!(wrong_lab.get("result").is_none());
    // Current configuration and credentials must not participate in retained replay.
    std::fs::write(
        lab.path().join("ronchi/config.toml"),
        "invalid current configuration; credentials unavailable",
    )
    .unwrap();
    assert_eq!(lab.command(&args)["result"], cli["result"]);
    let status = lab.command(&["status"]);
    assert_eq!(status["result"]["snapshot_seq"], snapshot);
    let unknown = base(lab.path())
        .args([
            "check",
            "evaluate",
            "custom-state",
            "--definition-version",
            "8",
            "--snapshot-seq",
            &snapshot.to_string(),
            "--intent-id",
            "W1",
        ])
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(3));
    let wrong_digest = base(lab.path())
        .args(args)
        .args(["--definition-digest", &"0".repeat(64)])
        .output()
        .unwrap();
    assert_eq!(wrong_digest.status.code(), Some(3));
    let missing = base(lab.path())
        .args(["check", "evaluate", "custom-state", "--intent-id", "W1"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
    let schema = command(
        &lab.path().join("missing-project"),
        &["schema", "check.evaluate"],
    );
    assert_eq!(schema["implemented"], true);
    assert!(schema.get("evaluation_schema").is_some());
    cancellation(&lab, &args);
    lab.command(&["down"]);
    assert_eq!(lab.command(&args)["result"], cli["result"]);
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

fn cancellation(lab: &Lab, args: &[&str]) {
    use std::{process::Stdio, time::Instant};
    let db = rusqlite::Connection::open(
        lab.path()
            .join("ronchi/labs")
            .join(&lab.id)
            .join("journal.sqlite"),
    )
    .unwrap();
    db.busy_timeout(Duration::ZERO).unwrap();
    db.execute_batch("ALTER TABLE query_events RENAME TO retained_navigation; CREATE VIEW query_events AS WITH RECURSIVE work(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM work WHERE x<100000000) SELECT retained_navigation.* FROM retained_navigation CROSS JOIN work;").unwrap();
    let health = || {
        let (status, value) = reference_support::http(
            lab.rpc,
            "POST",
            "/",
            json!({"jsonrpc":"2.0","id":1,"method":"getHealth"}),
        );
        assert_eq!(status, 200);
        assert_eq!(value["result"], "ok");
    };
    for mode in ["signal", "pipe", "deadline"] {
        let mut child = base(lab.path())
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            health();
            let (_, frames, done): (i64, i64, i64) = db
                .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap();
            if done < frames {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Evaluation never acquired a read snapshot"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        if mode == "signal" {
            assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
        } else if mode == "pipe" {
            drop(child.stdout.take());
        }
        let deadline =
            Instant::now() + Duration::from_secs(if mode == "deadline" { 12 } else { 3 });
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Evaluation did not settle after {mode}");
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(match mode {
                "signal" => 130,
                "pipe" => 0,
                _ => 124,
            }),
            "{output:?}"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let (busy, _, _): (i64, i64, i64) = db
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap();
            if busy == 0 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Evaluation retained its cancelled snapshot"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        health();
    }
    db.execute_batch(
        "DROP VIEW query_events; ALTER TABLE retained_navigation RENAME TO query_events;",
    )
    .unwrap();
}
