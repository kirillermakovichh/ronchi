use crate::lab::{LabError, Manifest};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_FILE_BYTES: u64 = 64 * 1024;
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
pub fn nonce() -> Result<String, LabError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| LabError::internal("System randomness is unavailable"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub fn private_directory(path: &Path) -> Result<(), LabError> {
    use std::os::unix::fs::DirBuilderExt;
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    if fs::symlink_metadata(path)?.file_type().is_symlink() || !path.is_dir() {
        return Err(LabError::invalid(
            "Workspace paths must be directories without symlinks",
        ));
    }
    Ok(())
}
pub fn open_private(path: &Path, create: bool) -> Result<File, LabError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(create)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW);
    Ok(options.open(path)?)
}
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, LabError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut data = Vec::new();
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > MAX_FILE_BYTES {
        return Err(LabError::internal("Workspace metadata exceeds its limit"));
    }
    serde_json::from_slice(&data)
        .map_err(|_| LabError::internal("Workspace metadata is invalid or incompatible"))
}
pub fn atomic_json(path: &Path, value: &impl serde::Serialize) -> Result<(), LabError> {
    let data =
        serde_json::to_vec(value).map_err(|_| LabError::internal("Metadata cannot be encoded"))?;
    if data.len() as u64 > MAX_FILE_BYTES {
        return Err(LabError::internal("Metadata exceeds its byte limit"));
    }
    atomic_bytes(path, &data).map_err(LabError::from)
}
pub use crate::files::atomic_bytes;

#[derive(Clone)]
pub struct Workspace {
    pub root: PathBuf,
}
impl Workspace {
    pub fn new(project: &Path) -> Self {
        Self {
            root: project.join("ronchi"),
        }
    }
    pub fn initialize(&self) -> Result<(), LabError> {
        private_directory(&self.root)?;
        for name in ["labs", "names", "locks"] {
            private_directory(&self.root.join(name))?;
        }
        Ok(())
    }
    pub fn lab(&self, id: &str) -> Result<PathBuf, LabError> {
        if !identifier(id) {
            return Err(LabError::invalid("Invalid Lab ID"));
        }
        let labs = self.root.join("labs");
        let dir = labs.join(id);
        for path in [&self.root, &labs, &dir] {
            if path.exists() && fs::symlink_metadata(path)?.file_type().is_symlink() {
                return Err(LabError::invalid("Lab paths must not contain symlinks"));
            }
        }
        Ok(dir)
    }
    pub fn manifest(&self, id: &str) -> Result<Manifest, LabError> {
        let path = self.lab(id)?.join("manifest.json");
        if !path.exists() {
            return Err(LabError::unavailable("The selected Lab manifest is absent"));
        }
        let manifest: Manifest = read_json(&path)?;
        if manifest.lab_id != id || manifest.contract_version != 1 {
            return Err(LabError::conflict(
                "Manifest identity or version does not match",
            ));
        }
        Ok(manifest)
    }
    pub fn target(&self, name: &str, id: Option<&str>) -> Result<Manifest, LabError> {
        let id = match id {
            Some(id) => id.to_string(),
            None => {
                if !identifier(name) {
                    return Err(LabError::invalid("Invalid Lab name"));
                }
                let id: String = read_json(&self.root.join("names").join(format!("{name}.json")))
                    .map_err(|_| {
                    LabError::unavailable("No retained Lab exists for this name")
                })?;
                id
            }
        };
        self.manifest(&id)
    }
    pub fn lock(&self, name: &str, suffix: &str) -> Result<File, LabError> {
        if !identifier(name) || !identifier(suffix) {
            return Err(LabError::invalid("Invalid Lab lock identifier"));
        }
        for path in [&self.root, &self.root.join("locks")] {
            if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
                return Err(LabError::invalid("Lock paths must not contain symlinks"));
            }
        }
        open_private(
            &self.root.join("locks").join(format!("{name}.{suffix}")),
            true,
        )
    }
    pub fn owner_free(&self, name: &str) -> Result<bool, LabError> {
        let lock = self.lock(name, "owner")?;
        match FileExt::try_lock_exclusive(&lock) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

pub fn read_token(path: &Path) -> Result<String, LabError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut token = String::new();
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?
        .take(65)
        .read_to_string(&mut token)?;
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(LabError::conflict("Control credential is invalid"));
    }
    Ok(token)
}
