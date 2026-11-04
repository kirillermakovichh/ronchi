use serde_json::Value;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    process::Command,
};

struct Lab {
    project: tempfile::TempDir,
    id: String,
    address: SocketAddr,
    token: String,
}
impl Lab {
    fn new() -> Self {
        let project = tempfile::tempdir().unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_ronchi"))
            .args(["--json", "--project"])
            .arg(project.path())
            .args([
                "up",
                "--detach",
                "--rpc-listen",
                "127.0.0.1:0",
                "--control-listen",
                "127.0.0.1:0",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let manifest = ronchi::workspace::Workspace::new(project.path())
            .target("default", None)
            .unwrap();
        let token = std::fs::read_to_string(
            project
                .path()
                .join("ronchi/labs")
                .join(&manifest.lab_id)
                .join("token"),
        )
        .unwrap();
        Self {
            project,
            id: manifest.lab_id,
            address: manifest.control_address.unwrap(),
            token,
        }
    }
    fn request(&self, method: &str, path: &str, headers: &str, body: &str) -> String {
        let mut stream = TcpStream::connect(self.address).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(4)))
            .unwrap();
        write!(stream, "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n{body}", self.address, body.len()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }
    fn ticket(&self) -> String {
        let response = self.request(
            "POST",
            &format!("/v1/labs/{}/dashboard-ticket", self.id),
            &format!("Authorization: Bearer {}\r\n", self.token.trim()),
            "",
        );
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let value: Value =
            serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        value["launch_url"]
            .as_str()
            .unwrap()
            .split("#ticket=")
            .nth(1)
            .unwrap()
            .into()
    }
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = Command::new(env!("CARGO_BIN_EXE_ronchi"))
            .arg("--project")
            .arg(self.project.path())
            .args(["--lab-id", &self.id, "down", "--force"])
            .output();
    }
}
#[test]
fn native_assets_and_single_use_lab_scoped_session_preserve_bearer_isolation() {
    let lab = Lab::new();
    let index = lab.request("GET", &format!("/ui/{}/", lab.id), "", "");
    assert!(index.starts_with("HTTP/1.1 200"));
    assert!(index.contains("Ronchi Investigation"));
    assert!(
        lab.request(
            "GET",
            &format!("/ui/{}/assets/../../Cargo.toml", lab.id),
            "",
            ""
        )
        .starts_with("HTTP/1.1 404")
    );
    let ticket = lab.ticket();
    let path = format!("/ui/{}/session", lab.id);
    let body = format!("{{\"ticket\":\"{ticket}\"}}");
    assert!(
        lab.request("POST", &path, "Origin: http://foreign.invalid\r\n", &body)
            .starts_with("HTTP/1.1 403")
    );
    let origin = format!("Origin: http://{}\r\n", lab.address);
    let exchanged = lab.request("POST", &path, &origin, &body);
    assert!(exchanged.starts_with("HTTP/1.1 200"), "{exchanged}");
    assert!(exchanged.contains("HttpOnly; SameSite=Strict"));
    assert!(
        lab.request("POST", &path, &origin, &body)
            .starts_with("HTTP/1.1 401")
    );
    let cookie = exchanged
        .lines()
        .find(|line| line.starts_with("set-cookie:"))
        .unwrap()
        .trim_start_matches("set-cookie: ")
        .split(';')
        .next()
        .unwrap();
    let headers = format!("{origin}Cookie: {cookie}\r\n");
    let status = lab.request("GET", &format!("/ui/{}/api/status", lab.id), &headers, "");
    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    assert!(!status.contains(lab.token.trim()));
    // Cookie auth cannot enter the bearer namespace, even with a valid Origin.
    assert!(
        lab.request("GET", &format!("/v1/labs/{}/status", lab.id), &headers, "")
            .starts_with("HTTP/1.1 403")
    );
    let other = Lab::new();
    assert!(
        other
            .request(
                "GET",
                &format!("/ui/{}/api/status", other.id),
                &format!("Origin: http://{}\r\nCookie: {cookie}\r\n", other.address),
                ""
            )
            .starts_with("HTTP/1.1 401")
    );
    assert!(
        lab.request(
            "GET",
            &format!("/ui/{}/api/status", lab.id),
            "Host: wrong.invalid\r\n",
            ""
        )
        .starts_with("HTTP/1.1 403")
    );
}

#[test]
fn retained_viewer_exits_after_stdout_closure_without_mutating_journal_or_other_lab() {
    use std::{
        io::{BufRead, BufReader},
        process::Stdio,
        time::{Duration, Instant},
    };
    struct Owned(std::process::Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    let lab = Lab::new();
    let stopped = Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(lab.project.path())
        .args(["--json", "--lab-id", &lab.id, "down"])
        .output()
        .unwrap();
    assert!(
        stopped.status.success(),
        "normal down failed: exit={:?} stdout={} stderr={} lab_id={}",
        stopped.status.code(),
        String::from_utf8_lossy(&stopped.stdout),
        String::from_utf8_lossy(&stopped.stderr),
        lab.id
    );
    let path = lab
        .project
        .path()
        .join("ronchi/labs")
        .join(&lab.id)
        .join("journal.sqlite");
    let before = std::fs::read(&path).unwrap();
    let other = Lab::new();
    let mut child = Owned(
        Command::new(env!("CARGO_BIN_EXE_ronchi"))
            .arg("--project")
            .arg(lab.project.path())
            .args(["--lab-id", &lab.id, "dashboard", "--serve", "--json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        send.send((line, reader)).unwrap();
    });
    let (line, pipe) = receive
        .recv_timeout(Duration::from_secs(5))
        .expect("viewer readiness deadline");
    reader.join().unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["result"]["read_only"], true);
    let url = url::Url::parse(value["result"]["launch_url"].as_str().unwrap()).unwrap();
    let address: SocketAddr = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap())
        .parse()
        .unwrap();
    assert!(TcpStream::connect(address).is_ok());
    drop(pipe);
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "viewer failed to exit within cleanup budget"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    assert_eq!(
        std::fs::read(path).unwrap(),
        before,
        "read-only viewer modified Journal"
    );
    assert!(
        other
            .request(
                "GET",
                &format!("/v1/labs/{}/status", other.id),
                &format!("Authorization: Bearer {}\r\n", other.token.trim()),
                ""
            )
            .starts_with("HTTP/1.1 200")
    );
}
