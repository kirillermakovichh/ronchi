use crate::{
    config::{EffectiveConfig, Mode, Settings},
    journal::{Event, JournalReader, JournalState},
    process::ProcessIdentity,
    workspace::{self, Workspace},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::PathBuf,
    time::{Duration, Instant},
};

pub mod daemon;
const STARTUP: Duration = Duration::from_secs(30);
const CLEANUP: Duration = Duration::from_secs(5);
#[derive(Debug)]
pub struct LabError {
    pub exit: i32,
    pub code: &'static str,
    pub message: String,
    pub phase: &'static str,
    pub effects: &'static str,
    pub lab_id: Option<String>,
}
impl LabError {
    fn new(exit: i32, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            exit,
            code,
            message: message.into(),
            phase: "lifecycle",
            effects: "none",
            lab_id: None,
        }
    }
    pub fn for_lab(mut self, id: &str) -> Self {
        self.lab_id = Some(id.to_string());
        self
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(6, "internal_error", message)
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(2, "invalid_argument", message)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(5, "state_conflict", message)
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(3, "lab_unavailable", message)
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(4, "unsupported_capability", message)
    }
}
impl From<std::io::Error> for LabError {
    fn from(_: std::io::Error) -> Self {
        Self::internal("Local filesystem or process operation failed")
    }
}
impl From<crate::journal::JournalError> for LabError {
    fn from(_: crate::journal::JournalError) -> Self {
        Self::internal("Journal admission or persistence failed; inspect Lab health")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Starting,
    Ready,
    Stopping,
    Stopped,
    Interrupted,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Manifest {
    pub contract_version: u32,
    pub lab_id: String,
    pub name: String,
    pub process: Option<ProcessIdentity>,
    pub owned_children: Vec<ProcessIdentity>,
    pub state: Lifecycle,
    pub health: String,
    pub configuration: EffectiveConfig,
    pub rpc_address: Option<SocketAddr>,
    pub control_address: Option<SocketAddr>,
    pub journal_state: JournalState,
    pub snapshot_seq: u64,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub active_case: Option<crate::cases::Policy>,
    #[serde(default)]
    pub held_effects: Vec<crate::cases::HeldEffect>,
    #[serde(default)]
    pub case_registration_seq: u64,
    pub failure: Option<String>,
    pub failure_exit: Option<i32>,
    pub versions: std::collections::BTreeMap<String, String>,
    pub disk_limits: crate::journal::DiskLimits,
}
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct LabResult {
    #[serde(flatten)]
    pub manifest: Manifest,
    pub selection_basis: String,
}
#[derive(Serialize, Deserialize)]
pub struct Launch {
    pub name: String,
    pub lab_id: String,
    pub configuration: EffectiveConfig,
}

pub fn event(kind: &str, started: Instant, data: serde_json::Value) -> Event {
    Event {
        schema_version: 1,
        kind: kind.into(),
        recorded_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unavailable".into()),
        elapsed_ns: started.elapsed().as_nanos().min(i64::MAX as u128) as u64,
        source: "ronchi".into(),
        execution_mode: Mode::Simulator,
        data,
    }
}
async fn operation_lock(
    ws: &Workspace,
    name: &str,
    cancellation: &mut tokio::signal::unix::Signal,
) -> Result<std::fs::File, LabError> {
    operation_lock_cancellable(
        ws,
        name,
        cancellation,
        &tokio_util::sync::CancellationToken::new(),
    )
    .await
}
async fn operation_lock_cancellable(
    ws: &Workspace,
    name: &str,
    cancellation: &mut tokio::signal::unix::Signal,
    external: &tokio_util::sync::CancellationToken,
) -> Result<std::fs::File, LabError> {
    let lock = ws.lock(name, "operation")?;
    let deadline = Instant::now() + STARTUP;
    loop {
        match fs2::FileExt::try_lock_exclusive(&lock) {
            Ok(()) => return Ok(lock),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(e) => return Err(e.into()),
        }
        if Instant::now() >= deadline {
            return Err(LabError::new(
                124,
                "deadline_exceeded",
                "Timed out waiting for the Lab operation lock",
            ));
        }
        tokio::select! { _=external.cancelled()=>return Err(LabError::new(130,"cancelled","Cancelled before acquiring the Lab lock")), _ = cancellation.recv()=> return Err(LabError::new(130,"cancelled","Cancelled before acquiring the Lab lock")), _ = tokio::time::sleep(Duration::from_millis(25))=>() }
    }
}
fn available(configuration: &EffectiveConfig) -> Result<(), LabError> {
    configuration.checks.validate().map_err(LabError::invalid)?;
    if configuration.mode != Mode::Simulator {
        return Err(LabError::unsupported("This build supports simulator mode"));
    }
    if let Some(case_id) = &configuration.initial_case {
        crate::cases::Arm {
            case_id: case_id.clone(),
            ..Default::default()
        }
        .validate()
        .map_err(crate::cases::lab_error)?;
    }
    Ok(())
}
async fn authenticated(ws: &Workspace, manifest: &Manifest) -> Result<Manifest, LabError> {
    let result = async {
        let expected = manifest
            .process
            .as_ref()
            .ok_or_else(|| LabError::unavailable("Lab process identity is absent"))?;
        if !crate::process::matches(expected)? {
            return Err(LabError::unavailable(
                "Recorded Lab process is no longer present",
            ));
        }
        let response: Manifest = crate::control::request(ws, manifest, false, false).await?;
        if response.lab_id != manifest.lab_id
            || response.process != manifest.process
            || response.contract_version != 1
        {
            return Err(LabError::conflict(
                "Authenticated daemon identity does not match the manifest",
            ));
        }
        Ok(response)
    }
    .await;
    result.map_err(|mut error: LabError| {
        error.lab_id = Some(manifest.lab_id.clone());
        error
    })
}
pub async fn status(project: PathBuf, name: &str, id: Option<&str>) -> Result<LabResult, LabError> {
    let ws = Workspace::new(&project);
    let mut manifest = ws.target(name, id)?;
    match authenticated(&ws, &manifest).await {
        Ok(live) => Ok(LabResult {
            manifest: live,
            selection_basis: if id.is_some() { "explicit" } else { "current" }.into(),
        }),
        Err(_) => {
            let is_current = ws
                .target(&manifest.name, None)
                .is_ok_and(|m| m.lab_id == manifest.lab_id);
            if is_current && !ws.owner_free(&manifest.name)? {
                return Err(LabError::conflict(
                    "A Lab owner holds the lock but cannot be authenticated; inspect its manifest and process identity",
                ).for_lab(&manifest.lab_id));
            }
            if let Ok(reader) =
                JournalReader::open(&ws.lab(&manifest.lab_id)?.join("journal.sqlite"))
                && let Some(binding) = reader.case_binding()?
            {
                manifest.active_case = binding.policy;
                manifest.case_registration_seq = binding.registration_seq;
            }
            let snapshot = JournalReader::open(&ws.lab(&manifest.lab_id)?.join("journal.sqlite"))
                .and_then(|r| r.snapshot(false));
            if matches!(
                manifest.state,
                Lifecycle::Starting | Lifecycle::Ready | Lifecycle::Stopping
            ) {
                manifest.health = "failed".into();
            }
            match snapshot {
                Ok(snapshot) => {
                    manifest.journal_state = snapshot.state;
                    manifest.snapshot_seq = snapshot.snapshot_seq;
                    manifest.state = if snapshot.state == JournalState::Finalized {
                        Lifecycle::Stopped
                    } else {
                        Lifecycle::Interrupted
                    };
                }
                Err(_) if manifest.state == Lifecycle::Failed => (),
                Err(_) => return Err(LabError::internal("Retained Journal cannot be read")),
            }
            manifest.held_effects.clear();
            manifest.rpc_address = None;
            manifest.control_address = None;
            Ok(LabResult {
                manifest,
                selection_basis: if id.is_some() { "explicit" } else { "latest" }.into(),
            })
        }
    }
}

pub async fn up(
    configuration: EffectiveConfig,
    explicit: Settings,
    name: &str,
) -> Result<LabResult, LabError> {
    up_selected(
        configuration,
        explicit,
        name,
        false,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
}
/// Start only a never-used name, under the ordinary startup operation lock.
pub(crate) async fn up_fresh(
    configuration: EffectiveConfig,
    external: tokio_util::sync::CancellationToken,
) -> Result<LabResult, LabError> {
    for index in 1..=1024 {
        let name = if index == 1 {
            "demo".into()
        } else {
            format!("demo-{index}")
        };
        match up_selected(
            configuration.clone(),
            Settings::default(),
            &name,
            true,
            external.clone(),
        )
        .await
        {
            Err(error) if error.code == "name_occupied" => continue,
            result => return result,
        }
    }
    Err(LabError::conflict(
        "Demo name allocation reached its bounded limit",
    ))
}
async fn up_selected(
    configuration: EffectiveConfig,
    explicit: Settings,
    name: &str,
    fresh: bool,
    external: tokio_util::sync::CancellationToken,
) -> Result<LabResult, LabError> {
    use std::process::{Command, Stdio};
    if !workspace::identifier(name) {
        return Err(LabError::invalid("Invalid Lab name"));
    }
    available(&configuration)?;
    let ws = Workspace::new(&configuration.project);
    ws.initialize()?;
    let mut cancellation =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let _operation = operation_lock_cancellable(&ws, name, &mut cancellation, &external).await?;
    if external.is_cancelled() {
        return Err(LabError::new(
            130,
            "cancelled",
            "Cancelled before Lab allocation",
        ));
    }
    if ws.root.join("names").join(format!("{name}.json")).exists() {
        if fresh {
            return Err(LabError::new(
                5,
                "name_occupied",
                "Fresh Lab name is already allocated",
            ));
        }
        let previous = ws.target(name, None)?;
        if let Ok(live) = authenticated(&ws, &previous).await {
            if live.state != Lifecycle::Ready {
                return Err(
                    LabError::conflict("Lab is not ready; inspect status before retrying")
                        .for_lab(&previous.lab_id),
                );
            }
            let config = &live.configuration;
            let compatible = explicit.checks.as_ref().is_none_or(|v| v == &config.checks)
                && explicit
                    .slot_duration_ms
                    .is_none_or(|v| v == config.slot_duration_ms)
                && explicit.mode.is_none_or(|v| v == config.mode)
                && explicit.read_upstream.as_ref().is_none_or(|v| {
                    Some(&crate::upstream::source_identity(v)) == config.read_upstream.as_ref()
                })
                && explicit.surfpool_datasource.as_ref().is_none_or(|v| {
                    Some(&crate::upstream::source_identity(v))
                        == config.surfpool_datasource.as_ref()
                })
                && explicit.rpc_listen.is_none_or(|v| v == config.rpc_listen)
                && explicit
                    .control_listen
                    .is_none_or(|v| v == config.control_listen)
                && explicit
                    .initial_case
                    .as_ref()
                    .is_none_or(|v| Some(v) == config.initial_case.as_ref());
            if !compatible {
                return Err(LabError::conflict(
                    "Explicit startup settings differ; stop the Lab before changing them",
                )
                .for_lab(&previous.lab_id));
            }
            return Ok(LabResult {
                manifest: live,
                selection_basis: "current".into(),
            });
        }
        for child in &previous.owned_children {
            match crate::process::matches(child) {
                Ok(false) => (),
                Ok(true) => {
                    return Err(LabError::conflict(
                        "A recorded child is still alive; this build cannot verify its cleanup capability, so name reuse is refused",
                    ));
                }
                Err(_) => {
                    return Err(LabError::conflict(
                        "Recorded child ownership cannot be established; name reuse is refused",
                    )
                    .for_lab(&previous.lab_id));
                }
            }
        }
        if !ws.owner_free(name)? {
            return Err(
                LabError::conflict("An unauthenticated owner remains; refuse name reuse")
                    .for_lab(&previous.lab_id),
            );
        }
        if let Some(process) = &previous.process
            && crate::process::matches(process)?
        {
            return Err(LabError::conflict(
                "Recorded daemon is still alive without control; inspect it before reusing this name",
            ));
        }
    }
    let id = format!("L{}", workspace::nonce()?);
    let result=async {
    let dir = ws.lab(&id)?;
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    }
    workspace::atomic_bytes(&dir.join("token"), workspace::nonce()?.as_bytes())?;
    workspace::atomic_json(
        &dir.join("launch.json"),
        &Launch {
            name: name.into(),
            lab_id: id.clone(),
            configuration: configuration.public(),
        },
    )?;
    let lease = workspace::open_private(&dir.join("startup.lease"), true)?;
    fs2::FileExt::try_lock_exclusive(&lease)?;
    let initial = Manifest {
        contract_version: 1,
        lab_id: id.clone(),
        name: name.into(),
        process: None,
        owned_children: Vec::new(),
        state: Lifecycle::Starting,
        health: "starting".into(),
        configuration: configuration.public(),
        rpc_address: None,
        control_address: None,
        journal_state: JournalState::Open,
        snapshot_seq: 0,
        capabilities: [
            "lab.up",
            "lab.status",
            "lab.down",
            "case.list",
            "case.arm",
            "case.disarm",
        ]
        .into_iter()
        .map(String::from)
        .chain(
            crate::rpc::BOOTSTRAP_METHODS
                .into_iter()
                .chain(crate::simulator::transactions::TRANSACTION_METHODS)
                .chain(crate::upstream::READ_METHODS.into_iter().filter(|_|configuration.read_upstream.is_some()))
                .map(|method| format!("rpc.{method}")),
        )
        .collect(),
        active_case: None,
        held_effects: vec![],
        case_registration_seq: 0,
        failure: None,
        failure_exit: None,
        versions: std::collections::BTreeMap::from([
            ("ronchi".into(), env!("CARGO_PKG_VERSION").into()),
            ("sqlite".into(), rusqlite::version().into()),
            ("solana-transaction".into(), "6.0.0".into()),
        ]),
        disk_limits: crate::journal::DiskLimits::default(),
    };
    workspace::atomic_json(&dir.join("manifest.json"), &initial)?;
    workspace::atomic_json(&ws.root.join("names").join(format!("{name}.json")), &id)?;
    let mut command = Command::new(std::env::current_exe()?);
    command.env_remove("RONCHI_RUNTIME_READ_UPSTREAM");
    if let Some(url) = &configuration.read_upstream { command.env("RONCHI_RUNTIME_READ_UPSTREAM", url); }
    let mut child = command
        .args(["__daemon", "--lab-id", &id])
        .arg("--project")
        .arg(&configuration.project)
        .current_dir(&configuration.project)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + STARTUP;
    loop {
        if let Ok(manifest) = ws.manifest(&id) {
            if manifest.state == Lifecycle::Ready && !external.is_cancelled() {
                let gate = workspace::open_private(&dir.join("readiness.gate"), true)?;
                if fs2::FileExt::try_lock_exclusive(&gate).is_ok() {
                    let manifest = ws.manifest(&id)?;
                    if manifest.state == Lifecycle::Ready {
                        let manifest = authenticated(&ws, &manifest).await?;
                        if manifest.state != Lifecycle::Ready || manifest.health != "healthy" {
                            let mut error=LabError::internal("Lab health failed before the startup response; inspect retained evidence").for_lab(&id);
                            error.effects = "applied";
                            return Err(error);
                        }
                        drop(lease);
                        return Ok(LabResult {
                            manifest,
                            selection_basis: "current".into(),
                        });
                    }
                }
            }
            if manifest.state == Lifecycle::Failed {
                let _ = child.wait();
                let exit = manifest.failure_exit.unwrap_or(6);
                let code = match exit {
                    2 => "invalid_argument",
                    3 => "lab_unavailable",
                    4 => "unsupported_capability",
                    5 => "state_conflict",
                    124 => "deadline_exceeded",
                    130 => "cancelled",
                    _ => "internal_error",
                };
                let mut error = LabError::new(
                    exit,
                    code,
                    manifest
                        .failure
                        .unwrap_or_else(|| "Lab startup failed".into()),
                );
                error.lab_id = Some(id.clone());
                return Err(error);
            }
        }
        if child.try_wait()?.is_some() {
            return Err(LabError::internal(
                "Lab process exited before readiness; inspect retained startup metadata",
            ));
        }
        let cancelled = tokio::select! { _=external.cancelled()=>true, _=cancellation.recv()=>true, _=tokio::time::sleep(Duration::from_millis(25))=>false };
        if cancelled || Instant::now() >= deadline {
            // Release the startup lease before waiting for the readiness gate.
            drop(lease);
            let gate = workspace::open_private(&dir.join("readiness.gate"), true)?;
            let gate_deadline = Instant::now() + CLEANUP;
            let mut acquired = false;
            loop {
                if fs2::FileExt::try_lock_exclusive(&gate).is_ok() {
                    acquired = true;
                    break;
                }
                if child.try_wait()?.is_some() || Instant::now() >= gate_deadline {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let ready = ws.manifest(&id).is_ok_and(|m| m.state == Lifecycle::Ready);
            let exited = child.try_wait()?.is_some();
            if !ready && acquired && !exited {
                child.kill()?;
                child.wait()?;
            }
            let uncertain = !ready && !acquired && !exited;
            let mut error = LabError::new(
                if cancelled { 130 } else { 124 },
                if cancelled {
                    "cancelled"
                } else {
                    "deadline_exceeded"
                },
                if ready {
                    "Readiness is committed; the Lab remains running"
                } else {
                    "Startup stopped before readiness; retained evidence may be interrupted"
                },
            );
            error.effects = if ready {
                "applied"
            } else if uncertain {
                "unknown"
            } else {
                "none"
            };
            error.lab_id = Some(id.clone());
            return Err(error);
        }
    }
    }.await;
    result.map_err(|mut error: LabError| {
        if error.lab_id.is_none() {
            error.lab_id = Some(id.clone());
        }
        error
    })
}

pub async fn down(
    project: PathBuf,
    name: &str,
    exact_id: Option<&str>,
    force: bool,
) -> Result<LabResult, LabError> {
    let ws = Workspace::new(&project);
    let name = if let Some(id) = exact_id {
        ws.manifest(id)?.name
    } else {
        name.to_string()
    };
    let name = name.as_str();
    if !workspace::identifier(name) {
        return Err(LabError::invalid("Invalid Lab name"));
    }
    if !ws.root.join("names").join(format!("{name}.json")).exists() {
        return Err(LabError::conflict(
            "No current live Lab exists for this name",
        ));
    }
    let mut cancellation =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let _operation = operation_lock(&ws, name, &mut cancellation).await?;
    let manifest = ws.target(name, None)?;
    if exact_id.is_some_and(|id| id != manifest.lab_id)
        || matches!(
            manifest.state,
            Lifecycle::Stopped | Lifecycle::Interrupted | Lifecycle::Failed
        )
    {
        let mut error = LabError::conflict("The selected Lab is not the current live target");
        error.lab_id = Some(exact_id.unwrap_or(&manifest.lab_id).to_string());
        return Err(error);
    }
    let live = authenticated(&ws, &manifest).await?;
    if let Err(error) = crate::control::request(&ws, &live, true, force).await
        && error.effects != "unknown"
    {
        return Err(error);
    }
    // A lost response does not undo an accepted shutdown. Inspect without retrying the mutation.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if ws.owner_free(name)?
            && !crate::process::matches(
                live.process
                    .as_ref()
                    .ok_or_else(|| LabError::internal("Missing process identity"))?,
            )?
        {
            let result = status(project, name, exact_id).await?;
            if result.manifest.health == "failed"
                || (!force && result.manifest.journal_state != JournalState::Finalized)
            {
                let mut error = LabError::internal(
                    "Lab stopped with interrupted evidence; inspect status and Journal health",
                );
                error.effects = "applied";
                error.lab_id = Some(live.lab_id);
                return Err(error);
            }
            return Ok(result);
        }
        if Instant::now() >= deadline {
            let mut error = LabError::new(
                124,
                "deadline_exceeded",
                "Shutdown is accepted but completion is uncertain; inspect status before retrying",
            );
            error.effects = "unknown";
            error.lab_id = Some(live.lab_id);
            return Err(error);
        }
        tokio::select! { _=cancellation.recv()=>{ let mut error=LabError::new(130,"cancelled","Shutdown remains accepted; inspect status for completion"); error.effects="applied"; error.lab_id=Some(live.lab_id); return Err(error); }, _=tokio::time::sleep(Duration::from_millis(25))=>() }
    }
}

pub async fn case_change(
    project: PathBuf,
    name: &str,
    id: Option<&str>,
    args: Option<crate::cases::Arm>,
) -> Result<LabResult, LabError> {
    if let Some(args) = &args {
        args.validate().map_err(crate::cases::lab_error)?;
    }
    let ws = Workspace::new(&project);
    let mut result = status(project, name, id).await?;
    if result.manifest.state != Lifecycle::Ready {
        return Err(LabError::conflict("Case changes require a ready live Lab")
            .for_lab(&result.manifest.lab_id));
    }
    let binding = crate::control::case_request(&ws, &result.manifest, args).await?;
    result.manifest.active_case = binding.policy;
    result.manifest.case_registration_seq = binding.registration_seq;
    Ok(result)
}
