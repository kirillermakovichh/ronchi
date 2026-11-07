//! Verified distribution companion or explicit project-local installation. No runtime downloads.
use crate::{
    lab::LabError,
    workspace::{self, Workspace},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Deserialize)]
struct Catalog<'a> {
    version: &'a str,
    #[serde(borrow)]
    artifacts: Vec<Artifact<'a>>,
}
#[derive(Deserialize)]
struct Artifact<'a> {
    platform: &'a str,
    sha256: &'a str,
}
fn catalog() -> &'static Catalog<'static> {
    static CATALOG: std::sync::OnceLock<Catalog<'static>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../release/surfpool.json"))
            .expect("checked-in Surfpool catalog must be valid")
    })
}
pub fn version() -> &'static str {
    catalog().version
}
const ARCHIVE_LIMIT: u64 = 64 * 1024 * 1024;
const BINARY_LIMIT: u64 = 128 * 1024 * 1024;
#[derive(Debug, Serialize)]
pub struct Installed {
    pub tool: &'static str,
    pub version: &'static str,
    pub platform: &'static str,
    pub archive_sha256: &'static str,
    pub binary_sha256: String,
    pub path: PathBuf,
}
fn platform() -> Result<(&'static str, &'static str), LabError> {
    let name = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "darwin-arm64",
        ("macos", "x86_64") => "darwin-x64",
        ("linux", "x86_64") => "linux-x64",
        _ => {
            return Err(LabError::unsupported(
                "This platform has no pinned native Surfpool artifact",
            ));
        }
    };
    catalog()
        .artifacts
        .iter()
        .find(|artifact| artifact.platform == name)
        .map(|artifact| (artifact.platform, artifact.sha256))
        .ok_or_else(|| LabError::unsupported("Pinned Surfpool artifact is absent from the catalog"))
}
fn digest(mut reader: impl Read, limit: u64, deadline: Instant) -> Result<String, LabError> {
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    let mut total = 0;
    loop {
        if Instant::now() >= deadline {
            return Err(LabError::internal(
                "Native tool operation exceeded five minutes",
            ));
        }
        let n = match reader.read(&mut bytes) {
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > limit {
            return Err(LabError::conflict(
                "Native tool exceeds its pinned size ceiling",
            ));
        }
        hash.update(&bytes[..n]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
fn file_digest(path: &Path, limit: u64, deadline: Instant) -> Result<String, LabError> {
    use std::os::unix::fs::OpenOptionsExt;
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(LabError::conflict(
            "Native tool inputs must be regular files",
        ));
    }
    digest(
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?,
        limit,
        deadline,
    )
}
fn root(project: &Path) -> PathBuf {
    project.join("ronchi/tools/surfpool").join(version())
}
fn verify_archive(path: &Path, deadline: Instant) -> Result<(), LabError> {
    if file_digest(path, ARCHIVE_LIMIT, deadline)? != platform()?.1 {
        return Err(LabError::conflict(
            "Surfpool archive checksum does not match the pinned official artifact",
        ));
    }
    Ok(())
}
fn expected_binary(archive: &Path, deadline: Instant) -> Result<String, LabError> {
    // Only the exact checksum-pinned archive reaches tar. Read one named regular entry.
    let mut child = Command::new("/usr/bin/tar")
        .args(["-xOzf"])
        .arg(archive)
        .arg("surfpool")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    use std::os::fd::AsRawFd;
    if let Some(stdout) = child.stdout.as_ref() {
        // SAFETY: stdout is this owned pipe; nonblocking reads permit cooperative deadline checks.
        unsafe {
            let flags = libc::fcntl(stdout.as_raw_fd(), libc::F_GETFL);
            if flags >= 0 {
                libc::fcntl(stdout.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }
    }
    let result = digest(
        child
            .stdout
            .take()
            .ok_or_else(|| LabError::internal("Archive reader is absent"))?,
        BINARY_LIMIT,
        deadline,
    );
    if result.is_err() {
        let _ = child.kill();
    }
    let status = wait(&mut child, deadline)?;
    if !status.success() {
        return Err(LabError::conflict(
            "Pinned Surfpool archive cannot be extracted",
        ));
    }
    result
}
fn wait(child: &mut Child, deadline: Instant) -> Result<std::process::ExitStatus, LabError> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(LabError::internal(
                "Native tool operation exceeded five minutes",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn verify_sync(project: &Path, deadline: Instant) -> Result<Installed, LabError> {
    let workspace = project.join("ronchi");
    if let Ok(metadata) = fs::symlink_metadata(&workspace)
        && (!metadata.is_dir() || metadata.file_type().is_symlink())
    {
        return Err(LabError::conflict(
            "Managed workspace must be a directory without symlinks",
        ));
    }
    verify_directory(&root(project), deadline)
}
fn verify_directory(directory: &Path, deadline: Instant) -> Result<Installed, LabError> {
    // The executable's distribution root is canonical; the private tool subtree
    // must contain regular directories, never a PATH-selected or symlinked tool.
    for path in directory.ancestors().take(3) {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                LabError::unsupported(
                    "Pinned Surfpool is absent; reinstall the Ronchi distribution, or use ronchi tool surfpool install --archive <official-v1.7.0-platform.tar.gz> for a source build",
                )
            } else {
                LabError::from(error)
            }
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(LabError::conflict(
                "Managed tool paths must be directories without symlinks",
            ));
        }
    }
    let archive = directory.join("artifact.tar.gz");
    verify_archive(&archive, deadline)?;
    let expected = expected_binary(&archive, deadline)?;
    let path = directory.join("surfpool");
    let actual = file_digest(&path, BINARY_LIMIT, deadline)?;
    if expected != actual {
        return Err(LabError::conflict(
            "Installed Surfpool binary differs from its verified artifact",
        ));
    }
    let (platform, archive_sha256) = platform()?;
    Ok(Installed {
        tool: "surfpool",
        version: version(),
        platform,
        archive_sha256,
        binary_sha256: actual,
        path,
    })
}
fn verify_for_executable(
    project: &Path,
    executable: &Path,
    deadline: Instant,
) -> Result<Installed, LabError> {
    let executable = executable.canonicalize()?;
    let distribution = executable
        .parent()
        .filter(|bin| bin.file_name().is_some_and(|name| name == "bin"))
        .and_then(Path::parent)
        .map(|root| root.join("libexec/surfpool"));
    if let Some(tools) = distribution {
        match fs::symlink_metadata(&tools) {
            Ok(_) => return verify_directory(&tools.join(version()), deadline),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    verify_sync(project, deadline)
}
pub async fn verify(project: PathBuf) -> Result<Installed, LabError> {
    tokio::task::spawn_blocking(move || {
        verify_for_executable(
            &project,
            &std::env::current_exe()?,
            Instant::now() + Duration::from_secs(300),
        )
    })
    .await
    .map_err(|_| LabError::internal("Native tool verification worker failed"))?
}
pub async fn install(project: PathBuf, archive: PathBuf) -> Result<Installed, LabError> {
    tokio::task::spawn_blocking(move || {
        let deadline = Instant::now() + Duration::from_secs(300);
        verify_archive(&archive, deadline)?;
        let ws = Workspace::new(&project);
        ws.initialize()?;
        let lock = ws.lock("surfpool", "install")?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|_| LabError::conflict("Surfpool installation is already in progress"))?;
        workspace::private_directory(&ws.root.join("tools"))?;
        workspace::private_directory(&ws.root.join("tools/surfpool"))?;
        let directory = root(&project);
        if directory.exists() {
            return verify_sync(&project, deadline);
        }
        let temporary = ws
            .root
            .join("tools/surfpool")
            .join(format!(".install-{}", workspace::nonce()?));
        workspace::private_directory(&temporary)?;
        let result = (|| {
            use std::os::unix::fs::OpenOptionsExt;
            let input = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&archive)?;
            if !input.metadata()?.is_file() {
                return Err(LabError::conflict(
                    "Tool archive must remain a regular file",
                ));
            }
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(temporary.join("artifact.tar.gz"))?;
            let copied = std::io::copy(&mut input.take(ARCHIVE_LIMIT + 1), &mut output)?;
            if copied > ARCHIVE_LIMIT {
                return Err(LabError::conflict(
                    "Tool archive grew beyond its byte limit",
                ));
            }
            output.sync_all()?;
            verify_archive(&temporary.join("artifact.tar.gz"), deadline)?;
            let mut extraction = Command::new("/usr/bin/tar")
                .arg("-xzf")
                .arg(temporary.join("artifact.tar.gz"))
                .arg("-C")
                .arg(&temporary)
                .arg("surfpool")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let status = wait(&mut extraction, deadline)?;
            if !status.success() {
                return Err(LabError::conflict(
                    "Pinned Surfpool archive cannot be extracted",
                ));
            }
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                temporary.join("surfpool"),
                fs::Permissions::from_mode(0o700),
            )?;
            File::open(temporary.join("surfpool"))?.sync_all()?;
            File::open(temporary.join("artifact.tar.gz"))?.sync_all()?;
            fs::rename(&temporary, &directory)?;
            File::open(
                directory
                    .parent()
                    .ok_or_else(|| LabError::internal("Tool parent is absent"))?,
            )?
            .sync_all()?;
            verify_sync(&project, deadline)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&temporary);
        }
        result
    })
    .await
    .map_err(|_| LabError::internal("Native tool installation worker failed"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_distribution_never_uses_a_project_tool() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin/ronchi"), b"fixture").unwrap();
        fs::create_dir_all(root.join("libexec/surfpool")).unwrap();
        let local = root.join("ronchi/tools/surfpool").join(version());
        fs::create_dir_all(&local).unwrap();
        fs::write(local.join("artifact.tar.gz"), b"invalid fallback").unwrap();
        let error = verify_for_executable(
            root,
            &root.join("bin/ronchi"),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap_err();
        assert!(error.message.contains("Pinned Surfpool is absent"));
        assert_eq!(error.code, "unsupported_capability");
    }

    #[test]
    fn symlinked_companion_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin/ronchi"), b"fixture").unwrap();
        fs::create_dir_all(root.join("libexec")).unwrap();
        fs::create_dir(root.join("other")).unwrap();
        fs::create_dir(root.join("other").join(version())).unwrap();
        std::os::unix::fs::symlink(root.join("other"), root.join("libexec/surfpool")).unwrap();
        let error = verify_for_executable(
            root,
            &root.join("bin/ronchi"),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap_err();
        assert!(error.message.contains("without symlinks"));
    }
}
