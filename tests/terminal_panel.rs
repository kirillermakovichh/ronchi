use serde_json::Value;
use std::{
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

struct Session {
    project: TempDir,
    master: File,
    child: Child,
    parser: vt100::Parser,
    raw: Vec<u8>,
    paused: Option<u32>,
}
impl Session {
    fn start(width: u16, args: &[&str], env: &[(&str, &str)]) -> Self {
        let project = TempDir::new().unwrap();
        let mut master = -1;
        let mut slave = -1;
        let mut size = libc::winsize {
            ws_row: 28,
            ws_col: width,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: openpty writes two descriptors and reads an initialized size.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::addr_of_mut!(size),
                )
            },
            0
        );
        // SAFETY: openpty returned two separately owned descriptors.
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        let fd = slave.as_raw_fd();
        for descriptor in [master.as_raw_fd(), fd] {
            // SAFETY: both descriptors are valid and owned by this test.
            assert_eq!(
                unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) },
                0
            );
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_ronchi"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("RONCHI_") {
                command.env_remove(key);
            }
        }
        for key in [
            "CI",
            "NO_COLOR",
            "CLICOLOR",
            "CLICOLOR_FORCE",
            "FORCE_COLOR",
        ] {
            command.env_remove(key);
        }
        command
            .env("TERM", "xterm-256color")
            .envs(env.iter().copied())
            .arg("--project")
            .arg(project.path())
            .args([
                "up",
                "--rpc-listen",
                "127.0.0.1:0",
                "--control-listen",
                "127.0.0.1:0",
            ])
            .args(args)
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave.try_clone().unwrap());
        // SAFETY: the post-fork closure calls only async-signal-safe OS functions.
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 || libc::ioctl(fd, libc::TIOCSCTTY as libc::c_ulong, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        drop(command);
        drop(slave);
        Self {
            project,
            master,
            child,
            parser: vt100::Parser::new(28, width, 0),
            raw: Vec::new(),
            paused: None,
        }
    }
    fn cli(&self, args: &[&str]) -> Value {
        let result = Command::new(env!("CARGO_BIN_EXE_ronchi"))
            .arg("--project")
            .arg(self.project.path())
            .arg("--json")
            .args(args)
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        serde_json::from_slice(&result.stdout).unwrap()
    }
    fn read(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            let mut fd = libc::pollfd {
                fd: self.master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: fd points to one initialized poll record.
            if unsafe { libc::poll(&mut fd, 1, 30) } > 0 {
                let mut bytes = [0u8; 8192];
                match self.master.read(&mut bytes) {
                    Ok(0) => return,
                    Ok(n) => {
                        self.raw.extend_from_slice(&bytes[..n]);
                        self.parser.process(&bytes[..n]);
                    }
                    Err(e) if e.raw_os_error() == Some(libc::EIO) => return,
                    Err(e) => panic!("PTY read: {e}"),
                }
                assert!(self.raw.len() < 1024 * 1024, "unbounded terminal output");
            }
        }
    }
    fn until(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(12);
        while !self.parser.screen().contents().contains(text) {
            assert!(
                Instant::now() < deadline,
                "missing {text:?}:\n{}",
                self.parser.screen().contents()
            );
            self.read(Duration::from_millis(50));
        }
    }
    fn resize(&mut self, width: u16) {
        // Decode pending frames at their original width. The emulator does not reflow history.
        // Pause only this test's UI process while synchronizing the two terminal sizes.
        let pid = self.child.id() as i32;
        let mut status = 0;
        // SAFETY: pid is an unreaped child exclusively owned by this test.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        // SAFETY: status is initialized and waitpid observes a stop without reaping the child.
        assert_eq!(
            unsafe { libc::waitpid(pid, &mut status, libc::WUNTRACED) },
            pid
        );
        assert!(libc::WIFSTOPPED(status));
        self.read(Duration::from_millis(60));
        let size = libc::winsize {
            ws_row: 28,
            ws_col: width,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: master is an owned PTY descriptor and size is initialized.
        assert_eq!(
            unsafe {
                libc::ioctl(
                    self.master.as_raw_fd(),
                    libc::TIOCSWINSZ as libc::c_ulong,
                    &size,
                )
            },
            0
        );
        self.parser.screen_mut().set_size(28, width);
        // SAFETY: the owned UI child remains stopped; continue its normal event loop.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGCONT) }, 0);
    }
    fn capture(&self, name: &str) {
        let Some(directory) = std::env::var_os("RONCHI_TERMINAL_PREVIEW") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let (rows, columns) = self.parser.screen().size();
        let cells: Vec<_> = (0..rows).map(|row| (0..columns).map(|column| {
            let cell = self.parser.screen().cell(row, column).unwrap();
            serde_json::json!({"text":cell.contents(),"fg":format!("{:?}",cell.fgcolor()),"bold":cell.bold()})
        }).collect::<Vec<_>>()).collect();
        std::fs::write(
            directory.join(format!("{name}.json")),
            serde_json::to_vec(&serde_json::json!({"columns":columns,"cells":cells})).unwrap(),
        )
        .unwrap();
    }
    fn finish(&mut self) {
        self.master.write_all(&[3]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(35);
        loop {
            self.read(Duration::from_millis(50));
            if let Some(exit) = self.child.try_wait().unwrap() {
                assert!(
                    exit.success(),
                    "{exit}: {}",
                    self.parser.screen().contents()
                );
                break;
            }
            assert!(Instant::now() < deadline, "Ctrl-C did not stop Lab");
        }
        self.read(Duration::from_millis(50));
        assert_eq!(
            self.cli(&["status"])["result"]["journal_state"],
            "finalized"
        );
        assert!(!self.parser.screen().alternate_screen());
        assert!(!self.parser.screen().hide_cursor());
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(pid) = self.paused {
            // SAFETY: pid is the authenticated daemon of this isolated test project.
            unsafe {
                libc::kill(pid as i32, libc::SIGCONT);
            }
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = Command::new(env!("CARGO_BIN_EXE_ronchi"))
            .arg("--project")
            .arg(self.project.path())
            .args(["down", "--force"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
fn animated_panel_reads_real_case_and_status_gaps_and_survives_resize() {
    let mut session = Session::start(88, &[], &[]);
    session.until("Ready");
    // The brand is visible in its signal color before any Fault has been applied.
    let terminal = session.parser.screen();
    let mut brand_rows = 0;
    for row in 0..28 {
        let mark: String = (2..7)
            .map(|col| terminal.cell(row, col).unwrap().contents())
            .collect();
        if mark == "⣒⣒⠭⣒⣒" {
            brand_rows += 1;
            for col in 2..7 {
                assert_eq!(
                    terminal.cell(row, col).unwrap().fgcolor(),
                    vt100::Color::Idx(221)
                );
            }
        }
    }
    assert_eq!(
        brand_rows, 2,
        "startup header needs four bands packed into two yellow rows"
    );
    for label in [
        "Lab",
        "RPC",
        "Dashboard",
        "Available until this Lab stops.",
        "Show link:",
    ] {
        let label_row = (0..28)
            .find(|row| {
                (2..2 + label.len() as u16)
                    .map(|col| terminal.cell(*row, col).unwrap().contents())
                    .collect::<String>()
                    == label
            })
            .expect("runtime label must be visible");
        for col in 2..2 + label.len() as u16 {
            assert_eq!(
                terminal.cell(label_row, col).unwrap().fgcolor(),
                vt100::Color::Default,
                "runtime labels and hints must use the normal foreground"
            );
        }
        if matches!(label, "Lab" | "RPC" | "Dashboard") {
            assert_eq!(
                terminal.cell(label_row, 12).unwrap().fgcolor(),
                vt100::Color::Idx(221),
                "Lab name and endpoint values must be yellow"
            );
        }
        if label == "Lab" {
            assert_eq!(
                terminal.cell(label_row, 21).unwrap().fgcolor(),
                vt100::Color::Default,
                "the engine label must use the normal foreground"
            );
        }
    }
    let screen = session.parser.screen().contents();
    assert!(screen.contains("RONCHI") && screen.contains("Make hidden failures visible."));
    assert!(screen.contains("RPC") && screen.contains("http://127.0.0.1:"));
    assert!(
        !screen.contains("#ticket="),
        "launch credential must not occupy screen width"
    );
    assert!(String::from_utf8_lossy(&session.raw).contains("\x1b]8;;http://"));
    let before = session.raw.len();
    session.read(Duration::from_millis(1100));
    assert!(session.raw.len() > before, "no live animation");
    session.cli(&["case", "arm", "tx-response-lost", "--hold-ms", "200"]);
    session.until("Case tx-response-lost");
    session.capture("ready");
    let manifest = session.cli(&["status"])["result"].clone();
    let pid = manifest["process"]["pid"].as_u64().unwrap() as u32;
    session.paused = Some(pid);
    // SAFETY: this is the authenticated owned daemon from this private test project.
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGSTOP) }, 0);
    session.until("Status unavailable");
    assert!(session.parser.screen().contents().contains("last #"));
    // SAFETY: the same daemon remains paused and owned by this fixture.
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGCONT) }, 0);
    session.paused = None;
    session.until("Ready");
    session.resize(36);
    session.read(Duration::from_millis(1100));
    session.until("Ready");
    session.until("tx-response-lost");
    assert_eq!(
        session.parser.screen().contents().matches("Ready").count(),
        1,
        "resize left old status rows: {}",
        session.parser.screen().contents()
    );
    session.capture("narrow");
    session.resize(88);
    session.until("Ready");
    session.finish();
    session.until("Journal finalized");
    session.capture("stopped");
}

#[test]
fn narrow_start_preserves_endpoints_and_real_holds_use_the_fault_accent() {
    let mut session = Session::start(36, &[], &[]);
    session.until("Ready");
    let screen = session.parser.screen().contents();
    assert!(screen.contains("Make hidden failures visible."));
    assert!(screen.contains("Show link: ronchi dashboard"));
    assert!(screen.contains("Ctrl-C"));
    session.capture("narrow-start");
    session.resize(88);
    session.cli(&[
        "case",
        "arm",
        "provider-timeout-burst",
        "--hold-ms",
        "4000",
        "--rpc-method",
        "getLatestBlockhash",
    ]);
    let address = session.cli(&["status"])["result"]["rpc_address"]
        .as_str()
        .unwrap()
        .to_string();
    let request = std::thread::spawn(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"getLatestBlockhash"}"#;
        write!(stream, "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        response
    });
    session.until("Held 1");
    let screen = session.parser.screen();
    assert!(
        (0..28).any(|row| (0..88)
            .any(|col| screen.cell(row, col).unwrap().fgcolor() == vt100::Color::Idx(221))),
        "held Fault has no signal accent"
    );
    session.capture("fault");
    session.finish();
    assert!(String::from_utf8_lossy(&session.raw).contains("Stopping Lab"));
    assert!(
        request.join().unwrap().is_empty(),
        "withheld RPC received a fabricated response"
    );
    session.until("Journal finalized");
}

#[test]
fn static_panel_disables_redraw_and_color_and_reports_external_stop() {
    let mut session = Session::start(44, &["--no-animation"], &[("NO_COLOR", "1")]);
    session.until("Lab ready");
    let before = session.raw.len();
    session.read(Duration::from_millis(1100));
    assert_eq!(session.raw.len(), before, "static presentation redrew");
    assert!(
        !String::from_utf8_lossy(&session.raw).contains("\x1b["),
        "NO_COLOR static output included CSI"
    );
    session.cli(&["down"]);
    session.until("Journal finalized");
    assert!(session.child.wait().unwrap().success());
    assert!(!session.parser.screen().alternate_screen());
}

#[test]
fn json_ci_and_dumb_terminals_have_no_presentation_escapes() {
    for (args, env) in [
        (vec!["--json"], vec![]),
        (vec![], vec![("TERM", "dumb")]),
        (vec![], vec![("CI", "true")]),
    ] {
        let mut session = Session::start(90, &args, &env);
        if args.contains(&"--json") {
            let deadline = Instant::now() + Duration::from_secs(12);
            while !session.raw.contains(&b'\n') {
                assert!(Instant::now() < deadline, "JSON readiness not flushed");
                session.read(Duration::from_millis(50));
            }
        } else {
            session.until("Press Ctrl-C to stop.");
        }
        if args.contains(&"--json") {
            let value: Value = serde_json::from_slice(&session.raw).unwrap();
            assert_eq!(value["result"]["state"], "ready");
        }
        assert!(!session.raw.contains(&0x1b));
        session.finish();
        assert!(!session.raw.contains(&0x1b));
    }
}

#[test]
fn interrupted_lab_does_not_get_a_successful_terminal_completion() {
    let mut session = Session::start(88, &[], &[]);
    session.until("Ready");
    let pid = session.cli(&["status"])["result"]["process"]["pid"]
        .as_u64()
        .unwrap() as u32;
    // SAFETY: this is the authenticated daemon belonging to this isolated fixture.
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        session.read(Duration::from_millis(50));
    }
    session.read(Duration::from_millis(100));
    assert_eq!(session.child.wait().unwrap().code(), Some(6));
    assert!(
        !session
            .parser
            .screen()
            .contents()
            .contains("Journal finalized")
    );
    assert!(!session.parser.screen().hide_cursor());
}
