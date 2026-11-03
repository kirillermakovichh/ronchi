pub mod backend;
pub mod cases;
pub mod checks;
pub mod config;
mod control;
mod dashboard;
pub mod demo;
pub mod export;
mod files;
pub mod findings;
pub mod journal;
pub mod lab;
pub mod maintenance;
mod mcp;
mod network;
pub mod process;
pub mod protocol;
pub mod query;
pub mod reference;
pub mod rpc;
pub mod simulator;
pub mod tools;
pub mod upstream;
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
    /// Select an exact retained or live Lab ID
    #[arg(long, global = true)]
    lab_id: Option<String>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Install a checksum-pinned native tool into this project's workspace
    Tool {
        #[command(subcommand)]
        tool: ToolCommand,
    },
    /// Maintain explicitly selected project-local evidence
    Lab {
        #[command(subcommand)]
        operation: MaintenanceCommand,
    },
    /// Attach a local MCP adapter without owning the persistent Lab
    Mcp {
        #[command(subcommand)]
        transport: McpTransport,
    },
    /// Print a single-use browser launch URL for a ready Lab
    Dashboard {
        /// Serve a retained Journal read-only in the foreground until Ctrl-C
        #[arg(long)]
        serve: bool,
        /// Restore the query from a credential-free pinned dashboard bookmark
        #[arg(long, default_value = "")]
        selection: String,
    },
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
        #[arg(value_parser = ["lab.up", "lab.status", "lab.down", "lab.delete", "lab.export", "case.list", "case.arm", "case.disarm", "case.validate", "investigate.summary", "investigate.timeline", "investigate.evidence", "investigate.findings", "check.observe", "check.evaluate"])]
        operation: String,
    },
}
#[derive(Subcommand)]
enum ToolCommand {
    Surfpool {
        #[command(subcommand)]
        operation: SurfpoolToolCommand,
    },
}
#[derive(Subcommand)]
enum SurfpoolToolCommand {
    /// Install the pinned official platform archive without downloading it
    Install {
        #[arg(long)]
        archive: PathBuf,
    },
}
#[derive(Subcommand)]
enum MaintenanceCommand {
    /// Delete one stopped Lab and its artifacts; requires --lab-id
    Delete,
    /// Export a bounded selected evidence stream; requires --lab-id
    Export(Box<maintenance::cli::ExportArgs>),
}
#[derive(Subcommand)]
enum McpTransport {
    /// Serve official MCP messages on stdio until EOF
    Stdio,
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
    /// Validate a bounded Custom Case without starting or contacting a Lab
    Validate {
        file: PathBuf,
        #[arg(long, default_value = "simulator")]
        mode: Mode,
    },
    /// Activate one validated Custom Case on the selected live Lab
    Activate { file: PathBuf },
    /// Arm or replace the continuing policy without restarting the Lab
    Arm {
        case: String,
        #[arg(long, default_value_t = 20_000)]
        hold_ms: u64,
        #[arg(long, default_value_t = 16)]
        max_held: u16,
        #[arg(long = "signature")]
        signatures: Vec<String>,
        /// Lower the status Case identity ceiling (maximum 1024)
        #[arg(long)]
        status_max_keys: Option<u16>,
        /// Lower the status Case serialized state budget (1024..4194304 bytes)
        #[arg(long)]
        status_max_snapshot_bytes: Option<usize>,
        /// Select supported timeout methods; omitted means the maintained default set
        #[arg(long = "rpc-method")]
        methods: Vec<String>,
        #[arg(long)]
        outage_ms: Option<u64>,
        #[arg(long)]
        recovery_ms: Option<u64>,
        /// Select the blockhash Case's source commitment (default: finalized)
        #[arg(long)]
        blockhash_commitment: Option<simulator::clock::Commitment>,
    },
    /// Disarm for later registrations; held requests retain their deadlines
    Disarm,
}
#[derive(Serialize, JsonSchema)]
struct CatalogReply {
    contract_version: u32,
    request_id: String,
    result: Vec<cases::CatalogEntry>,
    custom: serde_json::Value,
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
    let mcp_stdio = args
        .windows(2)
        .any(|pair| pair[0] == "mcp" && pair[1] == "stdio");
    let json = !mcp_stdio
        && args
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
    if matches!(cli.command, Command::Mcp { .. }) && (cli.lab.is_some() || cli.lab_id.is_some()) {
        return render_lab_error(
            false,
            lab::LabError::invalid(
                "MCP arguments select a Project Workspace; each tool selects its explicit Lab",
            ),
        );
    }
    let name = cli.lab.as_deref().unwrap_or("default");
    if !workspace::identifier(name) {
        return render_lab_error(cli.json, lab::LabError::invalid("Invalid Lab name"));
    }
    match cli.command {
        Command::Tool {
            tool:
                ToolCommand::Surfpool {
                    operation: SurfpoolToolCommand::Install { archive },
                },
        } => {
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            match run_async(tools::install(root, archive)) {
                Err(error) => render_lab_error(cli.json, error),
                Ok(result) => {
                    let text = format!(
                        "Installed Surfpool {} ({}) at {}",
                        result.version,
                        result.platform,
                        result.path.display()
                    );
                    let envelope = serde_json::json!({"contract_version":1,"request_id":format!("cli-{}",std::process::id()),"result":result});
                    match write_output(cli.json, &envelope, &text, false) {
                        Ok(()) => 0,
                        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 0,
                        Err(_) => 6,
                    }
                }
            }
        }
        Command::Lab {
            operation: MaintenanceCommand::Delete,
        } => {
            let Some(id) = cli.lab_id.as_deref() else {
                return render_lab_error(
                    cli.json,
                    lab::LabError::invalid("Lab deletion requires an exact --lab-id"),
                );
            };
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            match run_async(maintenance::delete(root, id)) {
                Err(error) => render_lab_error(cli.json, error),
                Ok(result) => {
                    let text = format!(
                        "Deleted Lab {} and its owned artifacts; evidence references are invalid",
                        result.deleted_lab_id
                    );
                    let envelope = serde_json::json!({"contract_version":1,"request_id":format!("cli-{}",std::process::id()),"lab_id":id,"result":result});
                    match write_output(cli.json, &envelope, &text, false) {
                        Ok(()) => 0,
                        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 0,
                        Err(_) => 6,
                    }
                }
            }
        }
        Command::Lab {
            operation: MaintenanceCommand::Export(args),
        } => {
            let Some(id) = cli.lab_id.as_deref() else {
                return render_lab_error(
                    cli.json,
                    lab::LabError::invalid("Evidence export requires an exact --lab-id"),
                );
            };
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            let token = tokio_util::sync::CancellationToken::new();
            let result = run_async(async {
                let mut signal =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
                let cancelled = token.clone();
                let export = maintenance::export(root, id, args.options(id), token);
                tokio::pin!(export);
                tokio::select! {
                    result = &mut export => result,
                    _ = signal.recv() => { cancelled.cancel(); export.await }
                }
            });
            match result {
                Err(error) => render_lab_error(cli.json, error),
                Ok(result) => {
                    let text = format!(
                        "Exported Lab {} at {} to {} ({} bytes; complete: {}; replayable: false)",
                        result.lab_id,
                        result.snapshot_seq,
                        result.path.display(),
                        result.bytes,
                        result.selection_complete
                    );
                    let envelope = serde_json::json!({"contract_version":1,"request_id":format!("cli-{}",std::process::id()),"lab_id":id,"result":result});
                    match write_output(cli.json, &envelope, &text, false) {
                        Ok(()) => 0,
                        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 0,
                        Err(_) => 6,
                    }
                }
            }
        }
        Command::Mcp {
            transport: McpTransport::Stdio,
        } => {
            let result =
                project(cli.project.as_deref()).and_then(|root| run_async(mcp::serve(root)));
            if let Err(error) = result {
                let _ = writeln!(io::stderr(), "{}: {}", error.code, error.message);
                return error.exit;
            }
            0
        }
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
                "lab.delete" => schemars::schema_for!(maintenance::DeleteRequest),
                "lab.export" => schemars::schema_for!(maintenance::ExportRequest),
                "case.arm" => schemars::schema_for!(CaseRequest),
                "case.validate" => schemars::schema_for!(cases::custom::Artifact),
                _ => schemars::schema_for!(StatusRequest),
            };
            let response = if operation.starts_with("investigate.") || operation == "check.evaluate"
            {
                schemars::schema_for!(query::Envelope)
            } else if operation == "check.observe" {
                schemars::schema_for!(checks::Envelope)
            } else if operation == "lab.delete" {
                schemars::schema_for!(maintenance::Envelope<maintenance::DeleteResult>)
            } else if operation == "lab.export" {
                schemars::schema_for!(maintenance::Envelope<maintenance::ExportResult>)
            } else if operation == "case.validate" {
                schemars::schema_for!(cases::custom::ValidationEnvelope)
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
            if operation.starts_with("case.") {
                value["custom_catalog"] = cases::custom::catalog();
                value["custom_plan_schema"] =
                    serde_json::to_value(schemars::schema_for!(cases::custom::Plan))
                        .expect("typed schema serializes");
                value["custom_error_schema"] =
                    serde_json::to_value(schemars::schema_for!(cases::custom::ValidationError))
                        .expect("typed schema serializes");
            }
            if operation.starts_with("investigate.") {
                value["finding_schema"] =
                    serde_json::to_value(schemars::schema_for!(findings::Finding))
                        .expect("typed schema serializes");
                value["rule_schema"] =
                    serde_json::to_value(schemars::schema_for!(findings::Definition))
                        .expect("typed schema serializes");
                value["maintained_case_rules"] = serde_json::json!([
                    cases::confirmation::finding_definition(),
                    cases::status::finding_definition(),
                    cases::timeout::finding_definition(),
                    cases::blockhash::finding_definition()
                ]);
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
        Command::Dashboard { serve, selection } => {
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            let selection = match dashboard::bookmark_query(&selection) {
                Ok(query) => query,
                Err(error) => return render_lab_error(cli.json, error),
            };
            if serve {
                return match run_async(dashboard::viewer::serve(
                    root,
                    name,
                    cli.lab_id.as_deref(),
                    cli.json,
                    &selection,
                )) {
                    Ok(()) => 0,
                    Err(error) => render_lab_error(cli.json, error),
                };
            }
            match run_async(dashboard::launch(root, name, cli.lab_id.as_deref())) {
                Err(error) => render_lab_error(cli.json, error),
                Ok(mut result) => {
                    result.launch_url = dashboard::with_selection(&result.launch_url, &selection);
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
                surfpool_snapshot: None,
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
            let root = configuration.project.clone();
            let result = run_async(lab::up(configuration, settings, name));
            let launch = result.as_ref().ok().and_then(|r| {
                run_async(dashboard::launch(root, name, Some(&r.manifest.lab_id))).ok()
            });
            render_lab_dashboard(cli.json, result, launch)
        }
        Command::Case { operation } => {
            if matches!(operation, CaseCommand::List) {
                let catalog = CatalogReply {
                    contract_version: 1,
                    request_id: format!("cli-{}", std::process::id()),
                    result: cases::catalog(),
                    custom: cases::custom::catalog(),
                };
                let text = catalog
                    .result
                    .iter()
                    .map(|entry| format!("{} · {}", entry.case_id, entry.modes.join(", ")))
                    .collect::<Vec<_>>()
                    .join("\n");
                return match write_output(cli.json, &catalog, &text, false) {
                    Ok(()) => 0,
                    Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
                    Err(_) => 6,
                };
            }
            if let CaseCommand::Validate { file, mode } = operation {
                let yaml = match cases::read_artifact(&file) {
                    Ok(yaml) => yaml,
                    Err(error) => return render_lab_error(cli.json, error),
                };
                let result = cases::custom::validate(&yaml, mode);
                let (value, text, exit) = match result {
                    Ok(plan) => (
                        serde_json::json!({"contract_version":1,"result":plan}),
                        "Custom Case is valid".into(),
                        0,
                    ),
                    Err(error) => (
                        serde_json::json!({"contract_version":1,"error":error}),
                        error.message.clone(),
                        2,
                    ),
                };
                return match write_output(cli.json, &value, &text, exit != 0) {
                    Ok(()) => exit,
                    Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
                    Err(_) => 6,
                };
            }
            let root = match project(cli.project.as_deref()) {
                Ok(root) => root,
                Err(error) => return render_lab_error(cli.json, error),
            };
            let args = match operation {
                CaseCommand::Activate { file } => {
                    let yaml = match cases::read_artifact(&file) {
                        Ok(yaml) => yaml,
                        Err(error) => return render_lab_error(cli.json, error),
                    };
                    Some(cases::Arm {
                        case_id: "custom".into(),
                        custom_yaml: Some(yaml),
                        ..Default::default()
                    })
                }
                CaseCommand::Arm {
                    case,
                    hold_ms,
                    max_held,
                    signatures,
                    status_max_keys,
                    status_max_snapshot_bytes,
                    methods,
                    outage_ms,
                    recovery_ms,
                    blockhash_commitment,
                } => Some(cases::Arm {
                    case_id: case,
                    custom_yaml: None,
                    hold_ms,
                    max_held,
                    signatures,
                    confirmation: None,
                    status: (status_max_keys.is_some() || status_max_snapshot_bytes.is_some())
                        .then(|| cases::status::Settings {
                            max_keys: status_max_keys.unwrap_or(1024),
                            max_snapshot_bytes: status_max_snapshot_bytes
                                .unwrap_or(4 * 1024 * 1024),
                        }),
                    timeout: if methods.is_empty() && outage_ms.is_none() && recovery_ms.is_none() {
                        None
                    } else {
                        Some(cases::timeout::Settings {
                            methods,
                            outage_ms: outage_ms.unwrap_or(5000),
                            recovery_ms: recovery_ms.unwrap_or(10000),
                        })
                    },
                    blockhash: blockhash_commitment
                        .map(|commitment| cases::blockhash::Settings { commitment }),
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
        #[serde(skip_serializing_if = "Option::is_none")]
        dashboard: Option<dashboard::Launch>,
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
    render_lab_dashboard(json, result, None)
}
fn render_lab_dashboard(
    json: bool,
    result: Result<lab::LabResult, lab::LabError>,
    dashboard: Option<dashboard::Launch>,
) -> i32 {
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
            let text = match &dashboard {
                Some(launch) => format!("{text} · dashboard {}", launch.launch_url),
                None => text,
            };
            let envelope = LabEnvelope::Success {
                contract_version: 1,
                request_id: format!("cli-{}", std::process::id()),
                lab_id: result.manifest.lab_id.clone(),
                result: Box::new(result),
                dashboard,
            };
            match write_output(json, &envelope, &text, false) {
                Ok(()) => 0,
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
                Err(_) => 6,
            }
        }
    }
}
