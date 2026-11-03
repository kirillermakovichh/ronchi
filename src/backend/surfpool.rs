//! Native Surfpool ownership. User applications and project sources are never cleanup targets.
use crate::{lab::LabError, process::ProcessIdentity, workspace};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::{Child, Command};
pub struct Owned {
    child: Child,
    pub identity: ProcessIdentity,
    pub client: super::Client,
    pub rpc_address: std::net::SocketAddr,
    pub websocket_address: std::net::SocketAddr,
    pub binary_sha256: String,
}
fn port() -> Result<std::net::TcpListener, LabError> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?)
}
impl Owned {
    pub async fn spawn(
        project: PathBuf,
        runtime: &Path,
        lab_id: &str,
        datasource: Option<&str>,
        snapshot: Option<&Path>,
    ) -> Result<Self, LabError> {
        let installed = crate::tools::verify(project.clone()).await?;
        let version = tokio::time::timeout(
            Duration::from_secs(3),
            Command::new(&installed.path)
                .arg("--version")
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .stdout(Stdio::piped())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| LabError::internal("Surfpool version check timed out"))??;
        if !version.status.success()
            || !String::from_utf8_lossy(&version.stdout)
                .split_whitespace()
                .any(|part| part == crate::tools::SURFPOOL_VERSION)
        {
            return Err(LabError::conflict(
                "Surfpool reports a different version from its pinned artifact",
            ));
        }
        workspace::private_directory(runtime)?;
        workspace::private_directory(&runtime.join("logs"))?;
        let snapshot = if let Some(path) = snapshot {
            let source = project.join(path).canonicalize()?;
            if !source.starts_with(&project) || !source.is_file() {
                return Err(LabError::invalid(
                    "Surfpool snapshot must be a regular file inside the selected project",
                ));
            }
            let destination = runtime.join("snapshot.json");
            let bytes = tokio::task::spawn_blocking(move || {
                use std::io::Read;
                let mut bytes = Vec::new();
                std::fs::File::open(source)?
                    .take(16 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > 16 * 1024 * 1024 {
                    return Err(LabError::invalid("Surfpool snapshot exceeds 16 MiB"));
                }
                let value: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|_| LabError::invalid("Surfpool snapshot must contain JSON"))?;
                if !value.is_object() {
                    return Err(LabError::invalid(
                        "Surfpool snapshot must contain a JSON object",
                    ));
                }
                Ok(bytes)
            })
            .await
            .map_err(|_| LabError::internal("Snapshot preparation failed"))??;
            workspace::atomic_bytes(&destination, &bytes)?;
            Some(destination)
        } else {
            None
        };
        let rpc = port()?;
        let ws = port()?;
        let rpc_address = rpc.local_addr()?;
        let websocket_address = ws.local_addr()?;
        let client = super::Client::new(
            rpc_address,
            format!("surfpool:{}:{lab_id}", crate::tools::SURFPOOL_VERSION),
            datasource,
        )
        .map_err(LabError::invalid)?;
        let mut command = Command::new(&installed.path);
        command
            .arg("start")
            .args([
                "--ci",
                "--no-deploy",
                "--airdrop-amount",
                "0",
                "--db",
                ":memory:",
                "--host",
                "127.0.0.1",
                "--port",
            ])
            .arg(rpc_address.port().to_string())
            .arg("--ws-port")
            .arg(websocket_address.port().to_string())
            .arg("--log-path")
            .arg(runtime.join("logs"))
            .args(["--log-level", "none", "--slot-time", "250"]);
        if let Some(datasource) = datasource {
            command.env("SURFPOOL_DATASOURCE_RPC_URL", datasource);
        } else {
            command
                .arg("--offline")
                .env_remove("SURFPOOL_DATASOURCE_RPC_URL");
        }
        if let Some(snapshot) = snapshot {
            command.arg("--snapshot").arg(snapshot);
        }
        let manifest = project.join("txtx.yml");
        if manifest.is_file() {
            command.arg("--manifest-file-path").arg(manifest);
        }
        command
            .env_remove("SURFPOOL_NETWORK")
            .current_dir(&project)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        drop(rpc);
        drop(ws);
        let mut child = command.spawn()?;
        let id = child
            .id()
            .ok_or_else(|| LabError::internal("Managed Surfpool has no process identity"))?;
        let identity = match crate::process::identity(id) {
            Ok(Some(identity)) => identity,
            _ => {
                let _ = child.kill().await;
                return Err(LabError::internal(
                    "Managed Surfpool identity cannot be verified",
                ));
            }
        };
        Ok(Self {
            child,
            identity,
            client,
            rpc_address,
            websocket_address,
            binary_sha256: installed.binary_sha256,
        })
    }
    pub async fn ready(&mut self) -> Result<serde_json::Value, LabError> {
        let readiness = async {
            loop {
                if self.child.try_wait()?.is_some() {
                    return Err(LabError::internal(
                        "Managed Surfpool exited before readiness",
                    ));
                }
                if self
                    .client
                    .call("getHealth", &serde_json::json!([]))
                    .await
                    .is_ok_and(|value| value == "ok")
                {
                    let version = self
                        .client
                        .call("getVersion", &serde_json::json!([]))
                        .await
                        .map_err(|_| {
                            LabError::internal("Managed Surfpool version readiness check failed")
                        })?;
                    let blockhash = self
                        .client
                        .call("getLatestBlockhash", &serde_json::json!([]))
                        .await
                        .map_err(|_| {
                            LabError::internal("Managed Surfpool blockhash readiness check failed")
                        })?;
                    if !blockhash["value"]["blockhash"].is_string()
                        || !blockhash["value"]["lastValidBlockHeight"].is_u64()
                    {
                        return Err(LabError::internal(
                            "Managed Surfpool readiness returned an invalid blockhash tuple",
                        ));
                    }
                    if self.child.try_wait()?.is_some() {
                        return Err(LabError::internal(
                            "Managed Surfpool exited during readiness",
                        ));
                    }
                    return Ok(serde_json::json!({"version":version,"blockhash":blockhash}));
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(120), readiness)
            .await
            .map_err(|_| LabError::internal("Managed Surfpool readiness exceeded 120 seconds"))?
    }
    pub async fn exited(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child.wait().await
    }
    pub async fn stop(&mut self, deadline: tokio::time::Instant) -> Result<(), LabError> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        match crate::process::identity(self.identity.pid)? {
            Some(identity) if identity == self.identity => {}
            None => {
                tokio::time::timeout_at(deadline, self.child.wait())
                    .await
                    .map_err(|_| {
                        LabError::internal("Managed Surfpool disappeared without a bounded reap")
                    })??;
                return Ok(());
            }
            _ => {
                return Err(LabError::conflict(
                    "Managed Surfpool identity changed; cleanup was refused",
                ));
            }
        }
        // SAFETY: this is our unreaped child and its full birth/executable identity was corroborated.
        let result = unsafe { libc::kill(self.identity.pid as i32, libc::SIGINT) };
        if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(LabError::internal("Managed Surfpool interrupt failed"));
        }
        let interrupt_deadline = deadline.min(tokio::time::Instant::now() + Duration::from_secs(2));
        if let Ok(result) = tokio::time::timeout_at(interrupt_deadline, self.child.wait()).await {
            result?;
            return Ok(());
        }
        self.child.start_kill()?;
        tokio::time::timeout_at(deadline, self.child.wait())
            .await
            .map_err(|_| {
                LabError::internal("Managed Surfpool did not exit within owned cleanup deadline")
            })??;
        Ok(())
    }
}

/// Reconcile only recorded, checksum-pinned Surfpool orphans after a daemon died.
/// Unknown executables and unverifiable identities remain conflicts.
pub async fn cleanup_orphans(
    project: PathBuf,
    children: &[ProcessIdentity],
) -> Result<(), LabError> {
    for expected in children {
        if !crate::process::matches(expected)? {
            continue;
        }
        let installed = crate::tools::verify(project.clone()).await?;
        if expected.executable != installed.path
            || crate::process::identity(expected.pid)?.as_ref() != Some(expected)
        {
            return Err(LabError::conflict(
                "Recorded orphan is not a verified managed Surfpool process; cleanup refused",
            ));
        }
        // SAFETY: manifest ownership, birth, executable identity, and pinned binary are corroborated.
        if unsafe { libc::kill(expected.pid as i32, libc::SIGINT) } != 0
            && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        {
            return Err(LabError::internal(
                "Verified Surfpool orphan could not be interrupted",
            ));
        }
        let interrupt = tokio::time::Instant::now() + Duration::from_secs(2);
        while crate::process::matches(expected)? && tokio::time::Instant::now() < interrupt {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if crate::process::matches(expected)? {
            if crate::process::identity(expected.pid)?.as_ref() != Some(expected) {
                return Err(LabError::conflict(
                    "Surfpool orphan identity changed before force cleanup",
                ));
            }
            // SAFETY: repeat identity verification immediately precedes signalling this recorded orphan.
            if unsafe { libc::kill(expected.pid as i32, libc::SIGKILL) } != 0
                && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
            {
                return Err(LabError::internal(
                    "Verified Surfpool orphan could not be killed",
                ));
            }
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while crate::process::matches(expected)? {
            if tokio::time::Instant::now() >= deadline {
                return Err(LabError::conflict(
                    "Verified Surfpool orphan remains after bounded cleanup",
                ));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    Ok(())
}
