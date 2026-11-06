//! Orchestrates known public components. It never writes application or RPC evidence.
mod child;
mod http;
use crate::{
    config::Mode,
    lab::{self, Manifest},
    query::{self, Arguments, Operation, Request},
    workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
#[derive(clap::Args)]
pub struct Args {
    #[arg(value_parser=["tx-response-lost"])]
    case: String,
    #[arg(long, value_enum, default_value = "resilient")]
    profile: crate::reference::Profile,
    #[arg(long)]
    stop_after_demo: bool,
}
#[derive(Serialize, Deserialize)]
pub struct Report {
    pub project: PathBuf,
    pub command_argv: Vec<Vec<String>>,
    pub profile: crate::reference::Profile,
    pub lab_name: String,
    pub lab_id: String,
    pub rpc_address: Option<std::net::SocketAddr>,
    pub control_address: Option<std::net::SocketAddr>,
    pub reference_name: String,
    pub reference_pid: u32,
    pub reference_stopped: bool,
    pub lab_state: String,
    pub intent_id: String,
    pub snapshot_seq: u64,
    pub summary: Value,
    pub findings: Value,
    pub evidence_gaps: Vec<query::Gap>,
    pub commands: Vec<String>,
    pub artifacts: PathBuf,
    pub dashboard_launch: Option<crate::dashboard::Launch>,
    pub dashboard_argv: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Error {
    pub code: String,
    pub phase: String,
    pub effects: String,
    pub message: String,
    pub lab_id: Option<String>,
    pub lab_name: Option<String>,
    pub reference_name: Option<String>,
    pub reference_pid: Option<u32>,
    pub reference_stopped: bool,
    pub lab_state: String,
    pub lab_stopped: bool,
    pub intent_id: Option<String>,
    pub snapshot_seq: Option<u64>,
    pub recovery: String,
    pub recovery_argv: Vec<String>,
    pub cleanup_error: Option<String>,
    pub artifacts: Option<PathBuf>,
    #[serde(skip)]
    exit: i32,
}
impl Error {
    fn new(code: &str, phase: &str) -> Box<Self> {
        Box::new(Self {
            code: code.into(),
            phase: phase.into(),
            effects: "none".into(),
            message: "Demo operation did not complete; inspect retained resources".into(),
            lab_id: None,
            lab_name: None,
            reference_name: None,
            reference_pid: None,
            reference_stopped: true,
            lab_state: "not_started".into(),
            lab_stopped: false,
            intent_id: None,
            snapshot_seq: None,
            cleanup_error: None,
            artifacts: None,
            recovery_argv: vec![],
            recovery:
                "Inspect the selected project and reported immutable Lab ID before another mutation"
                    .into(),
            exit: match code {
                "cancelled" => 130,
                "deadline_exceeded" => 124,
                "invalid_argument" => 2,
                "state_conflict" => 5,
                _ => 6,
            },
        })
    }
}

fn profile_name(profile: crate::reference::Profile) -> &'static str {
    match profile {
        crate::reference::Profile::Resilient => "resilient",
        crate::reference::Profile::Unsafe => "unsafe",
    }
}
struct Owner {
    project: PathBuf,
    manifest: Option<Manifest>,
    known_lab_id: Option<String>,
    child: Option<child::Owned>,
    reference_name: String,
    intent_id: Option<String>,
    snapshot_seq: Option<u64>,
    cancel: CancellationToken,
    stop_lab: bool,
    stage_initialized: bool,
    lab_stopped: bool,
}
impl Owner {
    fn error(&self, code: &str, phase: &str) -> Box<Error> {
        let mut error = Error::new(code, phase);
        error.reference_name = Some(self.reference_name.clone());
        error.artifacts = Some(self.project.join("ronchi/demos").join(&self.reference_name));
        error.intent_id = self.intent_id.clone();
        error.snapshot_seq = self.snapshot_seq;
        if let Some(id) = &self.known_lab_id {
            error.lab_id = Some(id.clone());
            error.effects = "unknown".into();
            error.lab_state = "inspect".into();
            error.recovery_argv = vec![
                executable(),
                "--project".into(),
                self.project.to_string_lossy().into_owned(),
                "--lab-id".into(),
                id.clone(),
                "status".into(),
            ];
            error.recovery = quoted(&error.recovery_argv);
        }
        if let Some(m) = &self.manifest {
            error.lab_id = Some(m.lab_id.clone());
            error.recovery_argv = vec![
                executable(),
                "--project".into(),
                self.project.to_string_lossy().into_owned(),
                "--lab-id".into(),
                m.lab_id.clone(),
                "status".into(),
            ];
            error.recovery = quoted(&error.recovery_argv);
            error.lab_name = Some(m.name.clone());
            error.effects = "applied".into();
            error.lab_state = "ready_or_inspect".into();
        }
        error.lab_stopped = self.lab_stopped;
        if let Some(child) = &self.child {
            error.reference_pid = Some(child.pid);
            error.reference_stopped = child.stopped;
        }
        error
    }
    async fn stage(&mut self, phase: &str, source: std::net::SocketAddr) -> Result<(), Box<Error>> {
        let root = self.project.join("ronchi/demos");
        crate::workspace::private_directory(&root)
            .map_err(|_| self.error("metadata_failed", phase))?;
        let directory = root.join(&self.reference_name);
        if !self.stage_initialized {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&directory)
                .map_err(|_| self.error("metadata_failed", phase))?;
            self.stage_initialized = true;
        }
        crate::workspace::atomic_json(&directory.join("state.json"),&json!({"phase":phase,"reference_name":self.reference_name,"source_address":source,"lab_id":self.manifest.as_ref().map(|m|&m.lab_id),"lab_name":self.manifest.as_ref().map(|m|&m.name),"reference_pid":self.child.as_ref().map(|c|c.pid),"intent_id":self.intent_id})).map_err(|_|self.error("metadata_failed",phase))?;
        #[cfg(debug_assertions)]
        if std::env::var("RONCHI_DEMO_TEST_BARRIER").ok().as_deref() == Some(phase) {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            while !directory.join("release").exists() {
                tokio::select! {_=self.cancel.cancelled()=>return Err(self.error("cancelled",phase)),_=tokio::time::sleep(Duration::from_millis(10))=>()}
                if tokio::time::Instant::now() >= deadline {
                    return Err(self.error("deadline_exceeded", phase));
                }
            }
        }
        Ok(())
    }
    async fn query(
        &self,
        operation: Operation,
        snapshot: u64,
        arguments: Arguments,
    ) -> Result<query::Response, Box<Error>> {
        let m = self
            .manifest
            .as_ref()
            .ok_or_else(|| self.error("internal_error", "selection"))?;
        let request = Request {
            contract_version: 1,
            request_id: format!(
                "demo-{}",
                crate::workspace::nonce().map_err(|_| self.error("internal_error", "identity"))?
            ),
            lab_id: m.lab_id.clone(),
            operation,
            arguments: Arguments {
                snapshot_seq: Some(snapshot),
                ..arguments
            },
        };
        crate::control::query_request(
            &Workspace::new(&self.project),
            m,
            &request,
            self.cancel.clone(),
        )
        .await
        .map_err(|e| self.error(&e.code, "investigation"))
    }
    async fn cleanup(&mut self, force: bool) -> Result<(), Box<Error>> {
        let mut failed = false;
        if let Some(child) = &mut self.child
            && child.stop(force).await.is_err()
        {
            failed = true;
        }
        if self.stop_lab
            && let Some(id) = self.known_lab_id.clone()
        {
            let name = self.manifest.as_ref().map_or("demo", |m| m.name.as_str());
            match tokio::time::timeout(
                Duration::from_secs(8),
                lab::down(self.project.clone(), name, Some(&id), force),
            )
            .await
            {
                Ok(Ok(result)) => {
                    self.manifest = Some(result.manifest);
                    self.lab_stopped = true;
                }
                _ => {
                    match lab::status(self.project.clone(), name, Some(&id)).await {
                        Ok(result)
                            if matches!(
                                result.manifest.state,
                                lab::Lifecycle::Stopped
                                    | lab::Lifecycle::Interrupted
                                    | lab::Lifecycle::Failed
                            ) && result.manifest.process.as_ref().is_none_or(|p| {
                                crate::process::matches(p).is_ok_and(|live| !live)
                            }) =>
                        {
                            self.manifest = Some(result.manifest);
                            self.lab_stopped = true;
                        }
                        _ => failed = true,
                    }
                }
            }
        }
        if !self.stop_lab
            && let Some(id) = self.known_lab_id.clone()
        {
            match tokio::time::timeout(
                Duration::from_secs(5),
                lab::status(self.project.clone(), "demo", Some(&id)),
            )
            .await
            {
                Ok(Ok(result)) => {
                    self.lab_stopped = matches!(
                        result.manifest.state,
                        lab::Lifecycle::Stopped
                            | lab::Lifecycle::Interrupted
                            | lab::Lifecycle::Failed
                    ) && result
                        .manifest
                        .process
                        .as_ref()
                        .is_none_or(|p| crate::process::matches(p).is_ok_and(|live| !live));
                    self.manifest = Some(result.manifest);
                }
                _ => failed = true,
            }
        }
        if failed {
            Err(self.error("cleanup_failed", "cleanup"))
        } else {
            Ok(())
        }
    }
    async fn work(&mut self, profile: crate::reference::Profile) -> Result<Report, Box<Error>> {
        let reservation = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|_| self.error("source_port_unavailable", "configuration"))?;
        let source_address = reservation
            .local_addr()
            .map_err(|_| self.error("source_port_unavailable", "configuration"))?;
        let rpc_listen = ([127, 0, 0, 1], 0).into();
        #[cfg(debug_assertions)]
        let rpc_listen = if let Ok(port) = std::env::var("RONCHI_DEMO_TEST_RPC_PORT") {
            (
                [127, 0, 0, 1],
                port.parse::<u16>()
                    .map_err(|_| self.error("invalid_argument", "configuration"))?,
            )
                .into()
        } else {
            rpc_listen
        };
        let configuration = crate::config::from_settings(
            self.project.clone(),
            crate::config::Settings {
                mode: Some(Mode::Simulator),
                rpc_listen: Some(rpc_listen),
                control_listen: Some(([127, 0, 0, 1], 0).into()),
                initial_case: Some("tx-response-lost".into()),
                ..Default::default()
            },
        )
        .map_err(|_| self.error("invalid_argument", "configuration"))?;
        Workspace::new(&self.project)
            .initialize()
            .map_err(|_| self.error("metadata_failed", "configuration"))?;
        self.stage("configured", source_address).await?;
        if self.cancel.is_cancelled() {
            return Err(self.error("cancelled", "startup"));
        }
        // The ordinary startup owns its lease, operation lock, readiness gate and SIGINT cleanup.
        match lab::up_fresh(configuration, self.cancel.clone()).await {
            Ok(result) => {
                self.known_lab_id = Some(result.manifest.lab_id.clone());
                self.manifest = Some(result.manifest);
            }
            Err(error) => {
                if let Some(id) = error.lab_id.as_deref() {
                    self.known_lab_id = Some(id.into());
                    self.manifest = lab::status(self.project.clone(), "demo", Some(id))
                        .await
                        .ok()
                        .map(|r| r.manifest)
                        .or_else(|| Workspace::new(&self.project).manifest(id).ok());
                }
                let mut result = self.error(error.code, "startup");
                result.exit = error.exit;
                result.effects = if self.known_lab_id.is_some() && error.effects == "none" {
                    "unknown".into()
                } else {
                    error.effects.into()
                };
                return Err(result);
            }
        }
        if self.cancel.is_cancelled() {
            return Err(self.error("cancelled", "startup"));
        }
        let m = self
            .manifest
            .as_ref()
            .ok_or_else(|| self.error("internal_error", "startup"))?
            .clone();
        let reference_root = self.project.join("ronchi/reference");
        crate::workspace::private_directory(&reference_root)
            .map_err(|_| self.error("reference_storage_failed", "reference_startup"))?;
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(reference_root.join(&self.reference_name))
                .map_err(|_| self.error("reference_storage_conflict", "reference_startup"))?;
        }
        drop(reservation); // A bind race now fails normal child readiness; no Workload is sent.
        self.stage("source_released", source_address).await?;
        let (child, readiness) = child::Owned::spawn(
            &self.project,
            &m.lab_id,
            &self.reference_name,
            profile_name(profile),
            source_address,
        )
        .map_err(|_| self.error("child_start_failed", "reference_startup"))?;
        self.child = Some(child);
        let ready = child::Owned::ready(readiness, &self.cancel)
            .await
            .map_err(|_| {
                self.error(
                    if self.cancel.is_cancelled() {
                        "cancelled"
                    } else {
                        "child_start_failed"
                    },
                    "reference_readiness",
                )
            })?;
        if ready["contract_version"] != 1
            || ready["lab_id"] != m.lab_id
            || ready["result"]["listen"] != source_address.to_string()
            || ready["result"]["submission"] != "enabled"
            || !self.child.as_mut().is_some_and(|c| c.alive())
        {
            return Err(self.error("child_start_failed", "reference_readiness"));
        }
        self.stage("reference_ready", source_address).await?;
        let intent_id = format!(
            "I{}",
            &crate::workspace::nonce().map_err(|_| self.error("internal_error", "intent"))?[..24]
        );
        self.intent_id = Some(intent_id.clone());
        let input = json!({"intent_id":intent_id,"recipient":bs58::encode([7u8;32]).into_string(),"amount_lamports":1});
        http::request(source_address, "POST", "/intents", input, &self.cancel)
            .await
            .map_err(|code| {
                let mut error = self.error(code, "intent");
                error.effects = "unknown".into();
                error
            })?;
        self.stage("intent_created", source_address).await?;
        let horizon = tokio::time::Instant::now() + Duration::from_secs(70);
        // Read only the bundled relayer's ordinary intent API to await the workload.
        // This state is not ingested into the Journal or used to decide application correctness.
        loop {
            if self.cancel.is_cancelled() {
                return Err(self.error("cancelled", "workload"));
            }
            if !self.child.as_mut().is_some_and(|c| c.alive()) {
                return Err(self.error("child_unavailable", "workload"));
            }
            let state = http::request(
                source_address,
                "GET",
                &format!("/intents/{intent_id}"),
                Value::Null,
                &self.cancel,
            )
            .await
            .map_err(|code| self.error(code, "workload"))?;
            if state["state"]
                .as_str()
                .is_some_and(|state| matches!(state, "succeeded" | "failed" | "unresolved"))
            {
                break;
            }
            if tokio::time::Instant::now() >= horizon {
                return Err(self.error("deadline_exceeded", "workload"));
            }
            tokio::select! {_=self.cancel.cancelled()=>return Err(self.error("cancelled","workload")),_=tokio::time::sleep(Duration::from_secs(1))=>()}
        }
        let current = lab::status(self.project.clone(), "demo", Some(&m.lab_id))
            .await
            .map_err(|_| self.error("lab_unavailable", "investigation"))?;
        let snapshot = current.manifest.snapshot_seq;
        self.snapshot_seq = Some(snapshot);
        let summary = self
            .query(Operation::Summary, snapshot, Arguments::default())
            .await?;
        let findings = self
            .query(
                Operation::Findings,
                snapshot,
                Arguments {
                    limit: 5,
                    ..Default::default()
                },
            )
            .await?;
        self.stage("evidence_ready", source_address).await?;
        let commands = drill_down(&self.project, &m.lab_id, snapshot);
        Ok(Report {
            dashboard_launch: None,
            dashboard_argv: vec![],
            project: self.project.clone(),
            command_argv: commands.clone(),
            profile,
            lab_name: m.name.clone(),
            lab_id: m.lab_id.clone(),
            rpc_address: m.rpc_address,
            control_address: m.control_address,
            reference_name: self.reference_name.clone(),
            reference_pid: self.child.as_ref().map_or(0, |c| c.pid),
            reference_stopped: false,
            lab_state: "ready".into(),
            intent_id: intent_id.clone(),
            snapshot_seq: snapshot,
            summary: summary.result,
            findings: findings.result,
            evidence_gaps: summary.view.evidence_gaps,
            commands: commands.iter().map(|args| quoted(args)).collect(),
            artifacts: self.project.join("ronchi/demos").join(&self.reference_name),
        })
    }
}
fn executable() -> String {
    std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "ronchi".into())
}
fn quoted(args: &[String]) -> String {
    args.iter()
        .map(|arg| format!("'{}'", arg.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}
fn drill_down(project: &std::path::Path, id: &str, snapshot: u64) -> Vec<Vec<String>> {
    let prefix = vec![
        executable(),
        "--project".into(),
        project.to_string_lossy().into_owned(),
        "--lab-id".into(),
        id.into(),
    ];
    let mut result = Vec::new();
    for op in ["summary", "findings"] {
        let mut args = prefix.clone();
        args.extend([
            "investigate".into(),
            op.into(),
            "--snapshot-seq".into(),
            snapshot.to_string(),
        ]);
        result.push(args);
    }
    result
}
pub fn run(project: PathBuf, args: Args, json_output: bool) -> i32 {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(_) => return 6,
    };
    let outcome = runtime.block_on(async {
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let watcher = tokio::spawn(async move {
            tokio::select! {_=tokio::signal::ctrl_c()=>(),_=crate::query::cli::stdout_closed()=>()};
            signal.cancel();
        });
        let name = format!(
            "demo-{}",
            crate::workspace::nonce().map_err(|_| Error::new("internal_error", "identity"))?
        );
        let mut owner = Owner {
            project,
            manifest: None,
            known_lab_id: None,
            child: None,
            reference_name: name,
            intent_id: None,
            snapshot_seq: None,
            cancel,
            stop_lab: args.stop_after_demo,
            stage_initialized: false,
            lab_stopped: false,
        };
        let mut result = owner.work(args.profile).await;
        let cleanup = owner.cleanup(result.is_err()).await;
        if let Err(cleanup_error) = cleanup {
            match &mut result {
                Err(error) => {
                    error.cleanup_error = Some(cleanup_error.code.clone());
                    error.reference_stopped =
                        owner.child.as_ref().is_none_or(|child| child.stopped);
                    error.lab_stopped = owner.lab_stopped;
                    if owner.manifest.is_some() {
                        error.lab_state = owner
                            .manifest
                            .as_ref()
                            .and_then(|m| serde_json::to_value(&m.state).ok())
                            .and_then(|v| v.as_str().map(String::from))
                            .unwrap_or_else(|| "stopped".into());
                    }
                }
                Ok(_) => result = Err(cleanup_error),
            }
        } else {
            match &mut result {
                Ok(report) => {
                    report.reference_stopped = true;
                    if let Some(m) = &owner.manifest {
                        report.lab_state = serde_json::to_value(&m.state)
                            .ok()
                            .and_then(|v| v.as_str().map(String::from))
                            .unwrap_or_else(|| "inspect".into());
                        report.rpc_address = if m.state == lab::Lifecycle::Ready {
                            m.rpc_address
                        } else {
                            None
                        };
                        report.control_address = if matches!(
                            m.state,
                            lab::Lifecycle::Ready | lab::Lifecycle::Stopping
                        ) {
                            m.control_address
                        } else {
                            None
                        };
                    }
                }
                Err(error) => {
                    error.reference_stopped = true;
                    error.lab_stopped = owner.lab_stopped;
                    if owner.manifest.is_some() {
                        error.lab_state = owner
                            .manifest
                            .as_ref()
                            .and_then(|m| serde_json::to_value(&m.state).ok())
                            .and_then(|v| v.as_str().map(String::from))
                            .unwrap_or_else(|| "stopped".into());
                    }
                }
            }
        }
        if let Ok(report) = &mut result {
            report.dashboard_argv = vec![
                executable(),
                "--project".into(),
                report.project.to_string_lossy().into_owned(),
                "--lab-id".into(),
                report.lab_id.clone(),
                "dashboard".into(),
            ];
            if report.lab_state == "ready" {
                report.dashboard_launch = crate::dashboard::launch(
                    report.project.clone(),
                    &report.lab_name,
                    Some(&report.lab_id),
                )
                .await
                .ok();
            } else {
                report.dashboard_argv.push("--serve".into());
            }
        }
        watcher.abort();
        let _ = watcher.await;
        result
    });
    let (exit, value) = match outcome {
        Ok(report) => (
            0,
            json!({"contract_version":1,"operation":"demo.tx-response-lost","result":report}),
        ),
        Err(error) => (
            error.exit,
            json!({"contract_version":1,"operation":"demo.tx-response-lost","error":error}),
        ),
    };
    let text = if json_output {
        value.to_string()
    } else {
        serde_json::to_string_pretty(&value)
            .expect("typed result serializes")
            .lines()
            .map(crate::query::cli::safe_text)
            .collect::<Vec<_>>()
            .join("\n")
    };
    match writeln!(io::stdout().lock(), "{text}") {
        Ok(()) => exit,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(_) => 6,
    }
}
