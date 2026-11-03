pub mod cases;
pub mod checks;
pub mod config;
mod control;
mod dashboard;
pub mod demo;
mod files;
pub mod findings;
pub mod journal;
pub mod lab;
mod network;
pub mod process;
pub mod protocol;
pub mod query;
pub mod reference;
pub mod rpc;
pub mod simulator;
pub mod workspace;
use clap::{Parser, Subcommand};
use config::{Mode, Settings};
use schemars::JsonSchema;
use serde::Serialize;
use std::{
    ffi::OsString,
    io::{self, Write},
    net::SocketAddr,
    path::PathBuf,
};

#[derive(Parser)]
#[command(version, about = "Local reliability lab for blockchain applications")]
pub struct Cli {
    /// Emit one version-1 JSON response envelope
    #[arg(long, global = true)]
    json: bool,
    /// Select an existing project directory
    #[arg(long, global = true)]
    project: Option<PathBuf>,
    /// Select a project-local Lab name (default: default)
    #[arg(long, global = true, conflicts_with = "lab_id")]
    lab: Option<String>,
    /// Select an exact Lab ID; mutations require the current live target
    #[arg(long, global = true)]
    lab_id: Option<String>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Print a single-use browser launch URL for a ready Lab
    Dashboard,
    /// Run an isolated bundled public-path experiment
    Demo(demo::Args),
    /// Start a persistent Lab and wait for lifecycle readiness
    Up {
        #[arg(long, env = "RONCHI_MODE", hide_env_values = true)]
        mode: Option<Mode>,
        #[arg(long, env = "RONCHI_READ_UPSTREAM", hide_env_values = true)]
        read_upstream: Option<String>,
        #[arg(long, env = "RONCHI_SURFPOOL_DATASOURCE", hide_env_values = true)]
        surfpool_datasource: Option<String>,
        #[arg(long, env = "RONCHI_RPC_LISTEN", hide_env_values = true)]
        rpc_listen: Option<SocketAddr>,
        #[arg(long, env = "RONCHI_CONTROL_LISTEN", hide_env_values = true)]
        control_listen: Option<SocketAddr>,
        #[arg(long = "case", env = "RONCHI_INITIAL_CASE", hide_env_values = true)]
        initial_case: Option<String>,
        #[arg(long, env = "RONCHI_SLOT_DURATION_MS", hide_env_values = true)]
        slot_duration_ms: Option<u64>,
    },
    /// List and change the continuing Case in a live Lab
    Case {
        #[command(subcommand)]
        operation: CaseCommand,
    },
    /// Query one bounded page of existing Journal evidence
    Investigate(query::cli::Selection),
    /// Collect application observations into the live Journal
    Check {
        #[command(subcommand)]
        operation: CheckCommand,
    },
    /// Run the standalone bundled reference application
    Reference {
        #[command(subcommand)]
        operation: ReferenceCommand,
    },
    /// Inspect a live or retained Lab
    Status,
    /// Stop the current named Lab and retain evidence
    Down {
        #[arg(long)]
        force: bool,
    },
    #[command(name = "__daemon", hide = true)]
    Daemon,
    /// Show version-1 request and response schemas without accessing a project
    Schema {
        #[arg(value_parser = ["lab.up", "lab.status", "lab.down", "case.list", "case.arm", "case.disarm", "investigate.summary", "investigate.timeline", "investigate.evidence", "investigate.findings", "check.observe", "check.evaluate"])]
        operation: String,
    },
}
#[derive(Subcommand)]
enum CheckCommand {
    Observe(checks::cli::Selection),
    /// Evaluate a pinned retained snapshot without contacting its source
    Evaluate(checks::cli::Evaluate),
}
#[derive(Subcommand)]
enum ReferenceCommand {
    /// Serve a durable foreground relayer; requires --lab-id
    Relayer(reference::Args),
}
#[derive(Subcommand)]
enum CaseCommand {
    /// List available built-in Cases offline
    List,
    /// Arm or replace the continuing policy without restarting the Lab
    Arm {
        case: String,
        #[arg(long, default_value_t = 20_000)]
        hold_ms: u64,
        #[arg(long, default_value_t = 16)]
        max_held: u16,
        #[arg(long = "signature")]
        signatures: Vec<String>,
    },
    /// Disarm for later registrations; held requests retain their deadlines
    Disarm,
}
#[derive(Serialize, JsonSchema)]
struct CatalogReply {
    contract_version: u32,
    request_id: String,
    result: Vec<cases::CatalogEntry>,
}
#[derive(JsonSchema)]
#[allow(dead_code)]
struct CaseRequest {
    contract_version: u32,
    request_id: String,
    operation: String,
    lab_id: String,
    arguments: cases::Arm,
}
#[derive(Serialize, JsonSchema)]
pub struct OperationError {
    pub code: String,
    pub message: String,
    pub phase: String,
    pub retryable: bool,
    pub effects: String,
    pub recovery: String,
}
#[derive(Serialize, JsonSchema)]
pub struct ErrorEnvelope {
    pub contract_version: u32,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lab_id: Option<String>,
    pub error: OperationError,
}
#[derive(Serialize, JsonSchema)]
struct UpRequest {
    contract_version: u32,
    request_id: String,
    operation: String,
    arguments: Settings,
}

fn write_output(json: bool, value: &impl Serialize, text: &str, error: bool) -> io::Result<()> {
    if json {
        serde_json::to_writer(io::stdout().lock(), value)?;
        writeln!(io::stdout())
    } else if error {
        writeln!(io::stderr(), "{text}")
    } else {
        writeln!(io::stdout(), "{text}")
    }
}
fn failure(json: bool, exit: i32, code: &str, message: String, phase: &str, recovery: &str) -> i32 {
    let envelope = ErrorEnvelope {
        contract_version: 1,
        request_id: format!("cli-{}", std::process::id()),
        lab_id: None,
        error: OperationError {
            code: code.into(),
            message: message.clone(),
            phase: phase.into(),
            retryable: false,
            effects: "none".into(),
            recovery: recovery.into(),
        },
    };
    match write_output(
        json,
        &envelope,
        &format!("{code}: {message}. {recovery}"),
        true,
    ) {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(_) => 6,
        Ok(()) => exit,
    }
}
pub fn run(args: impl IntoIterator<Item = OsString>) -> i32 {
    let args: Vec<_> = args.into_iter().collect();
    let json = args
        .iter()
        .skip(1)
        .take_while(|a| *a != "--")
        .any(|a| a == "--json");
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e)
            if matches!(
                e.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            return match e.print() {
                Ok(()) => 0,
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
                Err(_) => 6,
            };
        }
        Err(_) => {
            return failure(
                json,
                2,
                "invalid_argument",
                "Invalid command arguments".into(),
                "parse",
                "Run ronchi --help",
            );
        }
    };
    let name = cli.lab.as_deref().unwrap_or("default");
    if !workspace::identifier(name) {
        return render_lab_error(cli.json, lab::LabError::invalid("Invalid Lab name"));
    }
    match cli.command {
        Command::Demo(args) => {
            if cli.lab.is_some() || cli.lab_id.is_some() {
                return render_lab_error(
                    cli.json,
                    lab::LabError::invalid(
                        "Demo allocates its own fresh Lab; --lab and --lab-id are not accepted",
                    ),
                );
            }
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            demo::run(root, args, cli.json)
        }
        Command::Schema { operation } => {
            let request = match operation.as_str() {
                op if op.starts_with("investigate.") => schemars::schema_for!(query::Request),
                "check.observe" => schemars::schema_for!(checks::Request),
                "check.evaluate" => schemars::schema_for!(query::Request),
                "lab.up" => schemars::schema_for!(UpRequest),
                "lab.status" => schemars::schema_for!(StatusRequest),
                "lab.down" => schemars::schema_for!(DownRequest),
                "case.arm" => schemars::schema_for!(CaseRequest),
                _ => schemars::schema_for!(StatusRequest),
            };
            let response = if operation.starts_with("investigate.") || operation == "check.evaluate"
            {
                schemars::schema_for!(query::Envelope)
            } else if operation == "check.observe" {
                schemars::schema_for!(checks::Envelope)
            } else if operation == "case.list" {
                schemars::schema_for!(CatalogReply)
            } else {
                schemars::schema_for!(LabEnvelope)
            };
            let mut value = serde_json::json!({"contract_version":1,"operation":operation,"implemented":true,"request":request,"response":response});
            if operation == "check.evaluate" {
                value["evaluation_schema"] =
                    serde_json::to_value(schemars::schema_for!(checks::evaluate::Result))
                        .expect("typed schema serializes");
                value["rule_schema"] =
                    serde_json::to_value(schemars::schema_for!(checks::rules::Requirement))
                        .expect("typed schema serializes");
                value["reference_definition"] = serde_json::to_value(
                    checks::reference::definition("http://127.0.0.1:9001/checks/observe".into()),
                )
                .expect("typed fixture serializes");
            }
            if operation.starts_with("investigate.") {
                value["finding_schema"] =
                    serde_json::to_value(schemars::schema_for!(findings::Finding))
                        .expect("typed schema serializes");
                value["rule_schema"] =
                    serde_json::to_value(schemars::schema_for!(findings::Definition))
                        .expect("typed schema serializes");
            }
            match write_output(true, &value, "", false) {
                Ok(()) => 0,
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
                Err(_) => 6,
            }
        }
        Command::Reference {
            operation: ReferenceCommand::Relayer(args),
        } => {
            let Some(id) = cli.lab_id.as_deref() else {
                return failure(
                    cli.json,
                    2,
                    "invalid_argument",
                    "reference relayer requires an explicit --lab-id".into(),
                    "reference",
                    "Select the immutable local Lab ID",
                );
            };
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            match run_async(async {
                reference::run(root, id, args, cli.json)
                    .await
                    .map_err(|error| lab::LabError {
                        code: error.code(),
                        message: error.to_string(),
                        phase: "reference",
                        effects: "unknown",
                        exit: error.exit(),
                        lab_id: Some(id.into()),
                    })
            }) {
                Ok(()) => 0,
                Err(error) => render_lab_error(cli.json, error),
            }
        }
        Command::Check {
            operation: CheckCommand::Evaluate(args),
        } => {
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            checks::cli::evaluate(cli.json, root, name, cli.lab_id.as_deref(), args)
        }
        Command::Check {
            operation: CheckCommand::Observe(selection),
        } => {
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            checks::cli::run(cli.json, root, name, cli.lab_id.as_deref(), selection)
        }
        Command::Investigate(selection) => {
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            query::cli::run(cli.json, root, name, cli.lab_id.as_deref(), selection)
        }
        Command::Dashboard => {
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            match run_async(dashboard::launch(root, name, cli.lab_id.as_deref())) {
                Err(error) => render_lab_error(cli.json, error),
                Ok(result) => {
                    let text = result.launch_url.clone();
                    let envelope = serde_json::json!({"contract_version":1,"request_id":format!("cli-{}",std::process::id()),"result":result});
                    match write_output(cli.json, &envelope, &text, false) {
                        Ok(()) => 0,
                        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
                        Err(_) => 6,
                    }
                }
            }
        }
        Command::Up {
            mode,
            read_upstream,
            surfpool_datasource,
            rpc_listen,
            control_listen,
            initial_case,
            slot_duration_ms,
        } => {
            if cli.lab_id.is_some() {
                return render_lab_error(
                    cli.json,
                    lab::LabError::invalid("up selects a name, not a retained Lab ID"),
                );
            }
            let settings = Settings {
                simulator_state: None,
                checks: None,
                schema_version: None,
                mode,
                read_upstream,
                surfpool_datasource,
                rpc_listen,
                control_listen,
                initial_case,
                slot_duration_ms,
            };
            let project = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            let (configuration, settings) = match config::resolve_requested(project, settings) {
                Ok(c) => c,
                Err(message) => return render_lab_error(cli.json, lab::LabError::invalid(message)),
            };
            render_lab_result(cli.json, run_async(lab::up(configuration, settings, name)))
        }
        Command::Case { operation } => {
            if matches!(operation, CaseCommand::List) {
                let catalog = CatalogReply {
                    contract_version: 1,
                    request_id: format!("cli-{}", std::process::id()),
                    result: cases::catalog(),
                };
                return match write_output(
                    cli.json,
                    &catalog,
                    "tx-response-lost · simulator · default hold 20000 ms",
                    false,
                ) {
                    Ok(()) => 0,
                    Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
                    Err(_) => 6,
                };
            }
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            let args = match operation {
                CaseCommand::Arm {
                    case,
                    hold_ms,
                    max_held,
                    signatures,
                } => Some(cases::Arm {
                    case_id: case,
                    hold_ms,
                    max_held,
                    signatures,
                }),
                _ => None,
            };
            render_lab_result(
                cli.json,
                run_async(lab::case_change(root, name, cli.lab_id.as_deref(), args)),
            )
        }
        Command::Status => {
            let project = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            render_lab_result(
                cli.json,
                run_async(lab::status(project, name, cli.lab_id.as_deref())),
            )
        }
        Command::Down { force } => {
            let project = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            render_lab_result(
                cli.json,
                run_async(lab::down(project, name, cli.lab_id.as_deref(), force)),
            )
        }
        Command::Daemon => {
            let Some(id) = cli.lab_id else {
                return 2;
            };
            let project = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(_) => return 2,
            };
            if process::detach().is_err() {
                return 6;
            }
            match run_async(lab::daemon::run(project, id)) {
                Ok(()) => 0,
                Err(error) => error.exit,
            }
        }
    }
}

#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
enum LabEnvelope {
    Success {
        contract_version: u32,
        request_id: String,
        lab_id: String,
        result: Box<lab::LabResult>,
    },
    Error(ErrorEnvelope),
}
#[derive(JsonSchema)]
#[allow(dead_code)]
struct StatusRequest {
    contract_version: u32,
    request_id: String,
    lab_id: Option<String>,
    name: Option<String>,
}
#[derive(JsonSchema)]
#[allow(dead_code)]
struct DownRequest {
    contract_version: u32,
    request_id: String,
    name: String,
    force: bool,
}
fn project(explicit: Option<&std::path::Path>) -> Result<PathBuf, lab::LabError> {
    config::discover(explicit, &std::env::current_dir()?).map_err(lab::LabError::invalid)
}
fn run_async<T>(
    future: impl std::future::Future<Output = Result<T, lab::LabError>>,
) -> Result<T, lab::LabError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|_| lab::LabError::internal("Cannot initialize runtime"))?;
    let result = runtime.block_on(future);
    runtime.shutdown_timeout(std::time::Duration::from_millis(100));
    result
}
fn render_lab_error(json: bool, error: lab::LabError) -> i32 {
    let envelope = ErrorEnvelope {
        contract_version: 1,
        request_id: format!("cli-{}", std::process::id()),
        lab_id: error.lab_id,
        error: OperationError {
            code: error.code.into(),
            message: error.message.clone(),
            phase: error.phase.into(),
            retryable: false,
            effects: error.effects.into(),
            recovery:
                "Inspect ronchi status and correct the reported configuration or resource failure"
                    .into(),
        },
    };
    let envelope = LabEnvelope::Error(envelope);
    match write_output(
        json,
        &envelope,
        &format!(
            "{}: {}. Inspect ronchi status",
            error.code,
            error.message.escape_default()
        ),
        true,
    ) {
        Ok(()) => error.exit,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(_) => 6,
    }
}
fn render_lab_result(json: bool, result: Result<lab::LabResult, lab::LabError>) -> i32 {
    match result {
        Err(error) => render_lab_error(json, error),
        Ok(result) => {
            let text = format!(
                "{} · {:?} · {:?} · journal {:?} at {} · RPC {} · control {}",
                result.manifest.lab_id,
                result.manifest.configuration.mode,
                result.manifest.state,
                result.manifest.journal_state,
                result.manifest.snapshot_seq,
                result
                    .manifest
                    .rpc_address
                    .map(|a| a.to_string())
                    .unwrap_or_else(|| "unavailable".into()),
                result
                    .manifest
                    .control_address
                    .map(|a| a.to_string())
                    .unwrap_or_else(|| "unavailable".into())
            );
            let text = format!(
                "{text} · case {}",
                result
                    .manifest
                    .active_case
                    .as_ref()
                    .map(|policy| policy.settings.case_id.as_str())
                    .unwrap_or("none")
            );
            let envelope = LabEnvelope::Success {
                contract_version: 1,
                request_id: format!("cli-{}", std::process::id()),
                lab_id: result.manifest.lab_id.clone(),
                result: Box::new(result),
            };
            match write_output(json, &envelope, &text, false) {
                Ok(()) => 0,
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
                Err(_) => 6,
            }
        }
    }
}
