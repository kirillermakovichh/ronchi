#![allow(dead_code)]
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
pub struct Lab {
    pub directory: tempfile::TempDir,
    pub id: String,
    pub rpc: SocketAddr,
}
impl Lab {
    pub fn new() -> Self {
        Self::new_with_slot(400)
    }
    pub fn new_with_slot(slot: u64) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let up = command(
            directory.path(),
            &[
                "up",
                "--rpc-listen",
                "127.0.0.1:0",
                "--control-listen",
                "127.0.0.1:0",
                "--slot-duration-ms",
                &slot.to_string(),
            ],
        );
        let id = up["lab_id"].as_str().unwrap().to_string();
        let rpc = up["result"]["rpc_address"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        Self { directory, id, rpc }
    }
    pub fn path(&self) -> &Path {
        self.directory.path()
    }
    pub fn command(&self, args: &[&str]) -> Value {
        command(self.path(), args)
    }
    pub fn reference_dir(&self) -> PathBuf {
        self.path().join("ronchi/reference/reference")
    }
    pub fn barrier(&self, phase: &str) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if std::fs::read(self.reference_dir().join("barrier.json"))
                .ok()
                .and_then(|s| serde_json::from_slice::<Value>(&s).ok())
                .is_some_and(|v| v["phase"] == phase)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "barrier {phase} was not observed"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    pub fn release(&self) {
        std::fs::write(self.reference_dir().join("barrier.release"), []).unwrap();
    }
    pub fn start(&self, profile: &str, barrier: Option<&str>) -> Relayer {
        self.start_options(profile, barrier, None, None)
    }
    pub fn start_options(
        &self,
        profile: &str,
        barrier: Option<&str>,
        horizon: Option<u32>,
        id: Option<&str>,
    ) -> Relayer {
        for file in ["barrier.json", "barrier.release"] {
            let _ = std::fs::remove_file(self.reference_dir().join(file));
        }
        let mut command = base(self.path());
        command.args([
            "--lab-id",
            id.unwrap_or(&self.id),
            "reference",
            "relayer",
            "--profile",
            profile,
        ]);
        if let Some(horizon) = horizon {
            command.args(["--horizon-seconds", &horizon.to_string()]);
        }
        if let Some(barrier) = barrier {
            command.env("RONCHI_REFERENCE_TEST_BARRIER", barrier);
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut line = String::new();
            let mut output = BufReader::new(stdout);
            let result = output.read_line(&mut line);
            let _ = tx.send((result, line, output));
        });
        let (line, output) = match rx.recv_timeout(Duration::from_secs(8)) {
            Ok((Ok(_), line, output)) => (line, output),
            _ => {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("relayer readiness was not bounded");
            }
        };
        reader.join().unwrap();
        let ready: Value = serde_json::from_str(&line).unwrap();
        if ready.get("error").is_some() {
            child.wait().unwrap();
            panic!("{ready}");
        }
        Relayer {
            address: ready["result"]["listen"].as_str().unwrap().parse().unwrap(),
            child,
            ready,
            output,
        }
    }
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = base(self.path()).args(["down", "--force"]).output();
    }
}
pub fn base(project: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ronchi"));
    for key in [
        "RONCHI_MODE",
        "RONCHI_RPC_LISTEN",
        "RONCHI_CONTROL_LISTEN",
        "RONCHI_INITIAL_CASE",
        "RONCHI_READ_UPSTREAM",
        "RONCHI_SURFPOOL_DATASOURCE",
        "RONCHI_SLOT_DURATION_MS",
        "RONCHI_REFERENCE_TEST_BARRIER",
    ] {
        c.env_remove(key);
    }
    c.args(["--project", project.to_str().unwrap(), "--json"]);
    c
}
pub fn command(project: &Path, args: &[&str]) -> Value {
    let output = base(project).args(args).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}
pub struct Relayer {
    pub address: SocketAddr,
    pub child: Child,
    pub ready: Value,
    pub output: BufReader<std::process::ChildStdout>,
}
impl Relayer {
    pub fn request(&self, method: &str, path: &str, value: Value) -> (u16, Value) {
        http(self.address, method, path, value)
    }
    pub fn intent(&self) -> Value {
        let (status, value) = self.request("GET", "/intents/W1", Value::Null);
        assert_eq!(status, 200, "{value}");
        value
    }
    pub fn wait(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let value = self.intent();
            if predicate(&value) {
                return value;
            }
            assert!(Instant::now() < deadline, "{value}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    pub fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            unsafe {
                libc::kill(self.child.id() as i32, libc::SIGTERM);
            }
        }
        let status = self.child.wait().unwrap();
        assert!(status.success(), "{status:?}");
    }
    pub fn crash(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
        }
        self.child.wait().unwrap();
    }
}
impl Drop for Relayer {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
pub fn http(address: SocketAddr, method: &str, path: &str, value: Value) -> (u16, Value) {
    let body = if method == "GET" {
        String::new()
    } else {
        value.to_string()
    };
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    stream.write_all(format!("{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
    let mut response = String::new();
    stream
        .take(300 * 1024)
        .read_to_string(&mut response)
        .unwrap();
    let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = response.split_once("\r\n\r\n").unwrap().1;
    (
        status,
        serde_json::from_str(body).unwrap_or_else(|_| json!({"body":body})),
    )
}
pub fn input() -> Value {
    json!({"intent_id":"W1","recipient":bs58::encode([1u8;32]).into_string(),"amount_lamports":7})
}
