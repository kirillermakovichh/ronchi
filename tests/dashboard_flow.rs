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
