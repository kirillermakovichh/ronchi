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
