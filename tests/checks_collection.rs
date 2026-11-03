use ronchi::{
    checks::{Context, Definition, InvocationOutcome, Scope, collect_once},
    config::Mode,
    journal::{JournalReader, JournalStore, JournalWriter},
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
fn definition(url: String) -> Definition {
    serde_json::from_value(json!({"check_id":"state","version":1,"source":"independent-app","schema_version":1,"url":url,"facts":{"enabled":{"type":"boolean"}}})).unwrap()
}
fn read_request(stream: &mut std::net::TcpStream) -> Value {
    // macOS accepted sockets inherit O_NONBLOCK from the polling listener.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let length = loop {
        let mut b = [0u8; 1];
        assert_eq!(
            stream.read(&mut b).unwrap(),
            1,
            "source request closed before its headers"
        );
        bytes.push(b[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes);
            break headers
                .lines()
                .find_map(|s| {
                    s.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap();
        }
        assert!(bytes.len() < 32768);
    };
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}
fn response(request: &Value) -> Value {
    json!({"contract_version":1,"invocation_id":request["invocation_id"],"source":"independent-app","schema_version":1,"observed_at":"2026-11-03T00:00:00Z","revision":"source-17","facts":{"enabled":{"availability":"available","value":false}}})
}
#[tokio::test]
async fn complete_application_facts_are_committed_after_a_durable_start() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let writer = JournalWriter::start(JournalStore::create(&path, "L1").unwrap()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let db = path.clone();
    let source = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert_eq!(request["snapshot_seq"], 0);
        let reader = JournalReader::open(&db).unwrap();
        assert!(reader.snapshot(true).unwrap().snapshot_seq >= 1);
        let body = response(&request).to_string();
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
    let context = Context {
        writer: writer.handle(),
        lab_id: "L1".into(),
        mode: Mode::Simulator,
        started: Instant::now(),
    };
    let result = collect_once(
        &context,
        definition(format!("http://{address}/observe")),
        Scope {
            intent_ids: vec!["W1".into()],
            ..Default::default()
        },
        "C1",
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.outcome, InvocationOutcome::Observed);
    assert_eq!(result.start_ref.event_seq, 1);
    assert_eq!(result.completion_ref.event_seq, 2);
    source.join().unwrap();
    let reader = JournalReader::open(&path).unwrap();
    let saved = reader.event(2).unwrap().unwrap();
    assert_eq!(saved.source, "application");
    assert_eq!(saved.data["payload"]["facts"]["enabled"]["value"], false);
    assert_eq!(saved.data["payload"]["revision"], "source-17");
    assert!(saved.data.get("verdict").is_none());
    assert_ne!(saved.recorded_at, saved.data["payload"]["observed_at"]);
    tokio::task::spawn_blocking(move || writer.shutdown(None))
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn malformed_source_json_creates_a_gap_without_a_partial_observation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let writer = JournalWriter::start(JournalStore::create(&path, "L1").unwrap()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let source = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\n{broken}")
            .unwrap();
    });
    let context = Context {
        writer: writer.handle(),
        lab_id: "L1".into(),
        mode: Mode::Simulator,
        started: Instant::now(),
    };
    let result = collect_once(
        &context,
        definition(format!("http://{address}/observe")),
        Scope {
            intent_ids: vec!["W1".into()],
            ..Default::default()
        },
        "C1",
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.outcome, InvocationOutcome::SourceGap);
    source.join().unwrap();
    let reader = JournalReader::open(&path).unwrap();
    let saved = reader
        .event(result.completion_ref.event_seq)
        .unwrap()
        .unwrap();
    assert_eq!(saved.kind, "check.invocation_error");
    assert!(saved.data.get("payload").is_none());
    assert_eq!(saved.data["retention"], "no_validated_observation");
    tokio::task::spawn_blocking(move || writer.shutdown(None))
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn start_and_completion_commit_failures_are_product_failures() {
    for kind in ["check.invocation_started", "check.observation"] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.sqlite");
        let store = JournalStore::create(&path, "L1").unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch(&format!("CREATE TRIGGER reject_check BEFORE INSERT ON events WHEN json_extract(NEW.event_json,'$.kind')='{kind}' BEGIN SELECT RAISE(ABORT,'injected Check commit failure'); END;")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let received = count.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let source = std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    received.fetch_add(1, Ordering::SeqCst);
                    let request = read_request(&mut stream);
                    let body = response(&request).to_string();
                    let _=stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let writer = JournalWriter::start(store).unwrap();
        let context = Context {
            writer: writer.handle(),
            lab_id: "L1".into(),
            mode: Mode::Simulator,
            started: Instant::now(),
        };
        let error = collect_once(
            &context,
            definition(format!("http://{address}/observe")),
            Scope {
                intent_ids: vec!["W1".into()],
                ..Default::default()
            },
            "C1",
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code.as_ref(), "internal_error");
        assert_eq!(error.effects.as_ref(), "unknown");
        assert_eq!(
            count.load(Ordering::SeqCst),
            usize::from(kind == "check.observation")
        );
        stop.store(true, Ordering::SeqCst);
        source.join().unwrap();
        assert!(
            tokio::task::spawn_blocking(move || writer.shutdown(None))
                .await
                .unwrap()
                .is_err()
        );
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn collection_owner_cancels_inflight_http_and_excludes_the_same_scope() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("journal.sqlite");
    let writer = JournalWriter::start(JournalStore::create(&path, "L1").unwrap()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (started, seen) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let source = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        let _ = started.send(());
        released.recv_timeout(Duration::from_secs(3)).unwrap();
        let body = response(&request).to_string();
        let _ = stream.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        );
    });
    let context = Context {
        writer: writer.handle(),
        lab_id: "L1".into(),
        mode: Mode::Simulator,
        started: Instant::now(),
    };
    let (client, owner) = ronchi::checks::spawn(
        context,
        ronchi::checks::Config {
            sources: vec![definition(format!("http://{address}/observe"))],
            ..Default::default()
        },
    )
    .unwrap();
    let args = ronchi::checks::Arguments {
        collection_id: "C1".into(),
        check_id: "state".into(),
        scope: Scope {
            intent_ids: vec!["W1".into()],
            ..Default::default()
        },
        poll: true,
        horizon_seconds: Some(60),
    };
    client.start(args.clone()).await.unwrap();
    seen.await.unwrap();
    let mut duplicate = args;
    duplicate.collection_id = "C2".into();
    assert_eq!(
        client.start(duplicate).await.unwrap_err().code.as_ref(),
        "state_conflict"
    );
    client.cancel("C1".into()).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let report = client.status("C1".into()).await.unwrap();
        if report.completion_ref.is_some() {
            assert_eq!(report.status, "cancelled");
            assert_eq!(report.invocations, 1);
            assert_eq!(report.observations, 0);
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    release.send(()).unwrap();
    source.join().unwrap();
    owner
        .settle(tokio::time::Instant::now() + Duration::from_secs(1))
        .await
        .unwrap();
    tokio::task::spawn_blocking(move || writer.shutdown(None))
        .await
        .unwrap()
        .unwrap();
}

#[test]
fn source_fixture_waits_for_request_bytes_after_nonblocking_accept() {
    use std::net::TcpStream;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let (mut accepted, _) = loop {
        match listener.accept() {
            Ok(value) => break value,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            Err(error) => panic!("accept failed: {error}"),
        }
    };
    let (sent, received) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || sent.send(read_request(&mut accepted)).unwrap());
    let early = received.recv_timeout(Duration::from_millis(40));
    let body = r#"{"invocation_id":"I1"}"#;
    client
        .write_all(
            format!(
                "POST / HTTP/1.1\r\nHost: local\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .unwrap();
    let request = early
        .as_ref()
        .ok()
        .cloned()
        .unwrap_or_else(|| received.recv_timeout(Duration::from_secs(3)).unwrap());
    reader.join().unwrap();
    assert!(
        early.is_err(),
        "source fixture returned before request bytes: {request}"
    );
    assert_eq!(request["invocation_id"], "I1");
}
