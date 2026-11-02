use clap::ValueEnum;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ValueEnum, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Simulator,
    Surfpool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub schema_version: Option<u32>,
    pub mode: Option<Mode>,
    pub read_upstream: Option<String>,
    pub surfpool_datasource: Option<String>,
    pub rpc_listen: Option<SocketAddr>,
    pub control_listen: Option<SocketAddr>,
    pub initial_case: Option<String>,
    pub slot_duration_ms: Option<u64>,
    pub simulator_state: Option<crate::protocol::state::ProtocolState>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct EffectiveConfig {
    pub project: PathBuf,
    pub mode: Mode,
    pub read_upstream: Option<String>,
    pub surfpool_datasource: Option<String>,
    pub rpc_listen: SocketAddr,
    pub control_listen: SocketAddr,
    pub initial_case: Option<String>,
    #[serde(default = "default_slot_duration_ms")]
    pub slot_duration_ms: u64,
    #[serde(default)]
    pub simulator_state: crate::protocol::state::ProtocolState,
}

pub fn discover(explicit: Option<&Path>, cwd: &Path) -> Result<PathBuf, String> {
    let root = if let Some(path) = explicit {
        cwd.join(path)
    } else {
        cwd.ancestors()
            .find(|p| p.join("ronchi/config.toml").is_file())
            .or_else(|| cwd.ancestors().find(|p| p.join(".git").exists()))
            .unwrap_or(cwd)
            .to_path_buf()
    };
    let root = root
        .canonicalize()
        .map_err(|_| "Project directory cannot be resolved".to_string())?;
    if !root.is_dir() {
        return Err("Project must be a directory".into());
    }
    Ok(root)
}

pub fn resolve(project: PathBuf, overrides: Settings) -> Result<EffectiveConfig, String> {
    resolve_requested(project, overrides).map(|(configuration, _)| configuration)
}
pub fn resolve_requested(
    project: PathBuf,
    overrides: Settings,
) -> Result<(EffectiveConfig, Settings), String> {
    let path = project.join("ronchi/config.toml");
    let file: Settings = match std::fs::read_to_string(path) {
        Ok(text) => {
            let settings: Settings = toml::from_str(&text)
                .map_err(|_| "Invalid configuration: check keys and value types".to_string())?;
            if settings.schema_version != Some(1) {
                return Err("Configuration requires schema_version = 1".into());
            }
            settings
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(_) => return Err("Configuration cannot be read".into()),
    };
    let requested = Settings {
        simulator_state: overrides
            .simulator_state
            .clone()
            .or(file.simulator_state.clone()),
        slot_duration_ms: overrides.slot_duration_ms.or(file.slot_duration_ms),
        schema_version: None,
        mode: overrides.mode.or(file.mode),
        read_upstream: overrides
            .read_upstream
            .clone()
            .or(file.read_upstream.clone()),
        surfpool_datasource: overrides
            .surfpool_datasource
            .clone()
            .or(file.surfpool_datasource.clone()),
        rpc_listen: overrides.rpc_listen.or(file.rpc_listen),
        control_listen: overrides.control_listen.or(file.control_listen),
        initial_case: overrides.initial_case.clone().or(file.initial_case.clone()),
    };
    let simulator_state = requested.simulator_state.clone().unwrap_or_default();
    simulator_state.validate()?;
    let slot_duration_ms = requested.slot_duration_ms.unwrap_or(400);
    if !(1..=60_000).contains(&slot_duration_ms) {
        return Err("slot_duration_ms must be between 1 and 60000".into());
    }
    let mode = overrides.mode.or(file.mode).unwrap_or_default();
    let read_upstream = overrides.read_upstream.or(file.read_upstream);
    let surfpool_datasource = overrides.surfpool_datasource.or(file.surfpool_datasource);
    if (mode == Mode::Surfpool && read_upstream.is_some())
        || (mode == Mode::Simulator && surfpool_datasource.is_some())
    {
        return Err("Execution mode conflicts with the configured datasource authority".into());
    }
    for url in [&read_upstream, &surfpool_datasource].into_iter().flatten() {
        let valid = url::Url::parse(url).is_ok_and(|parsed| {
            matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some()
        });
        if !valid || url.chars().any(char::is_whitespace) {
            return Err("Datasource must be an absolute HTTP or HTTPS URL".into());
        }
    }
    let rpc_listen = overrides
        .rpc_listen
        .or(file.rpc_listen)
        .unwrap_or(([127, 0, 0, 1], 8899).into());
    let control_listen = overrides
        .control_listen
        .or(file.control_listen)
        .unwrap_or(([127, 0, 0, 1], 8900).into());
    if !rpc_listen.ip().is_loopback() || !control_listen.ip().is_loopback() {
        return Err("Listeners must use loopback addresses".into());
    }
    if rpc_listen == control_listen && rpc_listen.port() != 0 {
        return Err("RPC and control listeners must use distinct ports".into());
    }
    let initial_case = overrides.initial_case.or(file.initial_case);
    if initial_case.as_ref().is_some_and(|s| {
        s.is_empty()
            || s.len() > 128
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    }) {
        return Err("Initial Case must be a bounded identifier".into());
    }
    Ok((
        EffectiveConfig {
            project,
            mode,
            read_upstream,
            surfpool_datasource,
            rpc_listen,
            control_listen,
            initial_case,
            slot_duration_ms,
            simulator_state,
        },
        requested,
    ))
}

fn default_slot_duration_ms() -> u64 {
    400
}
