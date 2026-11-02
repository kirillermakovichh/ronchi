pub mod config;
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
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    project: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Start a Lab (not implemented in this build)
    Up {
        #[arg(long, env = "RONCHI_MODE")]
        mode: Option<Mode>,
        #[arg(long, env = "RONCHI_READ_UPSTREAM")]
        read_upstream: Option<String>,
        #[arg(long, env = "RONCHI_SURFPOOL_DATASOURCE")]
        surfpool_datasource: Option<String>,
        #[arg(long, env = "RONCHI_RPC_LISTEN")]
        rpc_listen: Option<SocketAddr>,
        #[arg(long, env = "RONCHI_CONTROL_LISTEN")]
        control_listen: Option<SocketAddr>,
        #[arg(long = "case", env = "RONCHI_INITIAL_CASE")]
        initial_case: Option<String>,
    },
    /// Show version-1 request and response schemas without accessing a project
    Schema {
        #[arg(value_parser = ["lab.up"])]
        operation: String,
    },
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
    match cli.command {
        Command::Schema { operation } => {
            let value = serde_json::json!({"contract_version":1,"operation":operation,"implemented":false,"request":schemars::schema_for!(UpRequest),"response":schemars::schema_for!(ErrorEnvelope)});
            match write_output(true, &value, "", false) {
                Ok(()) => 0,
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
                Err(_) => 6,
            }
        }
        Command::Up {
            mode,
            read_upstream,
            surfpool_datasource,
            rpc_listen,
            control_listen,
            initial_case,
        } => {
            let result = std::env::current_dir()
                .map_err(|_| "Current directory is unavailable".to_string())
                .and_then(|cwd| config::discover(cli.project.as_deref(), &cwd))
                .and_then(|project| {
                    config::resolve(
                        project,
                        Settings {
                            schema_version: None,
                            mode,
                            read_upstream,
                            surfpool_datasource,
                            rpc_listen,
                            control_listen,
                            initial_case,
                        },
                    )
                });
            match result {
                Err(message) => failure(
                    cli.json,
                    2,
                    "invalid_argument",
                    message,
                    "configuration",
                    "Correct the project path and startup configuration",
                ),
                Ok(_) => failure(
                    cli.json,
                    4,
                    "unsupported_capability",
                    "Lab startup is not implemented in this build".into(),
                    "startup",
                    "Use a build with Journal and Lab lifecycle support",
                ),
            }
        }
    }
}
