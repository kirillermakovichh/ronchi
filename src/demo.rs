//! Orchestrates known public components. It never writes application or RPC evidence.
mod child;
mod http;
use crate::{
    checks,
    config::Mode,
    lab::{self, Manifest},
    query::{self, Arguments, Operation, Request, Scope},
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
    pub check: checks::evaluate::Result,
    pub summary: Value,
    pub findings: Value,
    pub evidence_gaps: Vec<query::Gap>,
    pub commands: Vec<String>,
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
    pub collection_id: Option<String>,
    pub intent_id: Option<String>,
    pub recovery: String,
    pub recovery_argv: Vec<String>,
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
            collection_id: None,
            intent_id: None,
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
    child: Option<child::Owned>,
    reference_name: String,
    collection: Option<String>,
    intent_id: Option<String>,
    cancel: CancellationToken,
    stop_lab: bool,
}
impl Owner {
    fn error(&self, code: &str, phase: &str) -> Box<Error> {
        let mut error = Error::new(code, phase);
        error.reference_name = Some(self.reference_name.clone());
        error.collection_id = self.collection.clone();
        error.intent_id = self.intent_id.clone();
        if let Some(m) = &self.manifest {
            error.lab_id = Some(m.lab_id.clone());
            error.recovery_argv = vec![
                "ronchi".into(),
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
        if let Some(child) = &self.child {
            error.reference_pid = Some(child.pid);
            error.reference_stopped = child.stopped;
        }
        error
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
    async fn settle_collection(&self, cancel: bool) -> Result<checks::Report, Box<Error>> {
        let m = self
            .manifest
            .as_ref()
            .ok_or_else(|| self.error("internal_error", "collection"))?;
        let id = self
            .collection
            .as_ref()
            .ok_or_else(|| self.error("internal_error", "collection"))?;
        let ws = Workspace::new(&self.project);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        if cancel {
            tokio::time::timeout_at(
                deadline,
                crate::control::check_request(&ws, m, None, id, true),
            )
            .await
            .map_err(|_| self.error("cleanup_failed", "collection_cancel"))?
            .map_err(|_| self.error("cleanup_failed", "collection_cancel"))?;
        }
        loop {
            let reply = tokio::time::timeout_at(
                deadline,
                crate::control::check_request(&ws, m, None, id, false),
            )
            .await
            .map_err(|_| self.error("deadline_exceeded", "collection_terminal"))?
            .map_err(|e| self.error(&e.code, "collection_terminal"))?;
            if reply.result.status != "running" {
                return Ok(reply.result);
            }
            if self.cancel.is_cancelled() && !cancel {
                return Err(self.error("cancelled", "collection"));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    async fn collect(&mut self, intent: &str) -> Result<checks::Report, Box<Error>> {
        let m = self
            .manifest
            .as_ref()
            .ok_or_else(|| self.error("internal_error", "collection"))?;
        let id = format!(
            "demo-{}",
            crate::workspace::nonce().map_err(|_| self.error("internal_error", "collection"))?
        );
        self.collection = Some(id.clone());
        let request = checks::Request {
            contract_version: 1,
            operation: "check.observe".into(),
            request_id: id.clone(),
            lab_id: m.lab_id.clone(),
            arguments: checks::Arguments {
                collection_id: id.clone(),
                check_id: "reference-intent".into(),
                scope: checks::Scope {
                    intent_ids: vec![intent.into()],
                    ..Default::default()
                },
                poll: false,
                horizon_seconds: None,
            },
        };
        crate::control::check_request(
            &Workspace::new(&self.project),
            m,
            Some(&request),
            &id,
            false,
        )
        .await
        .map_err(|e| self.error(&e.code, "collection_start"))?;
        let result = self.settle_collection(false).await;
        if result.is_ok() {
            self.collection = None;
        }
        result
    }
    async fn cleanup(&mut self) -> Result<(), Box<Error>> {
        let mut failed = false;
        if self.collection.is_some() {
            if self.settle_collection(true).await.is_err() {
                failed = true;
            } else {
                self.collection = None;
            }
        }
        if let Some(child) = &mut self.child
            && child.stop().await.is_err()
        {
            failed = true;
        }
        if self.stop_lab
            && let Some(m) = &self.manifest
            && !matches!(
                tokio::time::timeout(
                    Duration::from_secs(8),
                    lab::down(self.project.clone(), &m.name, Some(&m.lab_id), true)
                )
                .await,
                Ok(Ok(_))
            )
        {
            failed = true;
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
        let definition =
            checks::reference::definition(format!("http://{source_address}/checks/observe"));
        let configuration = crate::config::from_settings(
            self.project.clone(),
            crate::config::Settings {
                checks: Some(checks::Config {
                    concurrency: 4,
                    sources: vec![definition],
                }),
                mode: Some(Mode::Simulator),
                rpc_listen: Some(([127, 0, 0, 1], 0).into()),
                control_listen: Some(([127, 0, 0, 1], 0).into()),
                initial_case: Some("tx-response-lost".into()),
                ..Default::default()
            },
        )
        .map_err(|_| self.error("invalid_argument", "configuration"))?;
        if self.cancel.is_cancelled() {
            return Err(self.error("cancelled", "startup"));
        }
        // The ordinary startup owns its lease, operation lock, readiness gate and SIGINT cleanup.
        match lab::up_fresh(configuration, self.cancel.clone()).await {
            Ok(result) => self.manifest = Some(result.manifest),
            Err(error) => {
                if let Some(id) = error.lab_id.as_deref() {
                    self.manifest = lab::status(self.project.clone(), "demo", Some(id))
                        .await
                        .ok()
                        .map(|r| r.manifest);
                }
                let mut result = self.error(error.code, "startup");
                result.exit = error.exit;
                result.effects = error.effects.into();
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
        let horizon = tokio::time::Instant::now() + Duration::from_secs(70);
        let mut digest = None;
        let (check, snapshot) = loop {
            if self.cancel.is_cancelled() {
                return Err(self.error("cancelled", "collection"));
            }
            let state = http::request(
                source_address,
                "GET",
                &format!("/intents/{intent_id}"),
                Value::Null,
                &self.cancel,
            )
            .await;
            let terminal = state
                .as_ref()
                .ok()
                .and_then(|s| s["state"].as_str())
                .is_some_and(|s| matches!(s, "succeeded" | "failed" | "unresolved"));
            let collected = self.collect(&intent_id).await?;
            if self.cancel.is_cancelled() {
                return Err(self.error("cancelled", "collection"));
            }
            if !self.child.as_mut().is_some_and(|c| c.alive()) {
                return Err(self.error("child_exited", "workload"));
            }
            if collected
                .last_invocation
                .as_ref()
                .is_some_and(|i| i.outcome == checks::InvocationOutcome::SourceGap)
                || state.is_err()
            {
                return Err(self.error("source_failure", "collection"));
            }
            let snapshot = collected.snapshot_seq;
            if digest.is_none() {
                let reference = collected
                    .last_invocation
                    .as_ref()
                    .ok_or_else(|| self.error("missing_evidence", "definition"))?
                    .start_ref
                    .clone();
                let evidence = self
                    .query(
                        Operation::Evidence,
                        snapshot,
                        Arguments {
                            scope: Scope {
                                event_refs: vec![reference],
                                ..Default::default()
                            },
                            fields: vec!["data".into()],
                            limit: 1,
                            ..Default::default()
                        },
                    )
                    .await?;
                let value = &evidence.result["items"][0]["data"]["definition"];
                if !value.is_object() {
                    return Err(self.error("missing_definition", "definition"));
                }
                digest =
                    Some(query::digest(&serde_json::to_vec(value).map_err(|_| {
                        self.error("missing_definition", "definition")
                    })?));
            }
            let evaluated = self
                .query(
                    Operation::Evaluate,
                    snapshot,
                    Arguments {
                        evaluation: Some(checks::evaluate::Selection {
                            check_id: "reference-intent".into(),
                            definition_version: 2,
                            definition_digest: digest.clone(),
                            scope: checks::Scope {
                                intent_ids: vec![intent_id.clone()],
                                ..Default::default()
                            },
                        }),
                        ..Default::default()
                    },
                )
                .await?;
            let check: checks::evaluate::Result = serde_json::from_value(evaluated.result)
                .map_err(|_| self.error("invalid_response", "evaluation"))?;
            if terminal {
                break (check, snapshot);
            }
            if tokio::time::Instant::now() >= horizon {
                return Err(self.error("deadline_exceeded", "workload"));
            }
            tokio::select! {_=self.cancel.cancelled()=>return Err(self.error("cancelled","workload")),_=tokio::time::sleep(Duration::from_secs(1))=>()}
        };
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
        let commands = drill_down(&self.project, &m.lab_id, snapshot, &intent_id);
        Ok(Report {
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
            check,
            summary: summary.result,
            findings: findings.result,
            evidence_gaps: summary.view.evidence_gaps,
            commands: commands.iter().map(|args| quoted(args)).collect(),
        })
    }
}
fn quoted(args: &[String]) -> String {
    args.iter()
        .map(|arg| format!("'{}'", arg.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}
fn drill_down(
    project: &std::path::Path,
    id: &str,
    snapshot: u64,
    intent: &str,
) -> Vec<Vec<String>> {
    let prefix = vec![
        "ronchi".into(),
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
    let mut args = prefix;
    args.extend([
        "check".into(),
        "evaluate".into(),
        "reference-intent".into(),
        "--definition-version".into(),
        "2".into(),
        "--intent-id".into(),
        intent.into(),
        "--snapshot-seq".into(),
        snapshot.to_string(),
    ]);
    result.push(args);
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
            child: None,
            reference_name: name,
            collection: None,
            intent_id: None,
            cancel,
            stop_lab: args.stop_after_demo,
        };
        let mut result = owner.work(args.profile).await;
        let cleanup = owner.cleanup().await;
        if let Err(error) = cleanup {
            result = Err(error);
        } else {
            match &mut result {
                Ok(report) => {
                    report.reference_stopped = true;
                    if args.stop_after_demo {
                        report.lab_state = "stopped".into();
                        report.rpc_address = None;
                        report.control_address = None;
                    }
                }
                Err(error) => {
                    error.reference_stopped = true;
                    if args.stop_after_demo && owner.manifest.is_some() {
                        error.lab_state = "stopped".into();
                    }
                }
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
        serde_json::to_string_pretty(&value).expect("typed result serializes")
    };
    match writeln!(io::stdout().lock(), "{text}") {
        Ok(()) => exit,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(_) => 6,
    }
}
