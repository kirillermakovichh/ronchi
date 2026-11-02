//! Process identity is corroborated with OS birth information and executable inode.
//! Persistent controls never signal a process based on a manifest PID.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub birth: String,
    pub executable: PathBuf,
    pub device: u64,
    pub inode: u64,
}

#[cfg(unix)]
pub fn identity(pid: u32) -> io::Result<Option<ProcessIdentity>> {
    use std::os::unix::fs::MetadataExt;
    let Some((birth, executable)) = platform_identity(pid)? else {
        return Ok(None);
    };
    let metadata = std::fs::metadata(&executable)?;
    Ok(Some(ProcessIdentity {
        pid,
        birth,
        executable,
        device: metadata.dev(),
        inode: metadata.ino(),
    }))
}
#[cfg(target_os = "macos")]
fn platform_identity(pid: u32) -> io::Result<Option<(String, PathBuf)>> {
    use std::os::unix::ffi::OsStringExt;
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    // SAFETY: proc_pidinfo writes at most `size` bytes into a correctly aligned buffer.
    let count = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size as i32,
        )
    };
    if count == 0 {
        return Ok(None);
    }
    if count != size as i32 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the complete structure was initialized by proc_pidinfo.
    let info = unsafe { info.assume_init() };
    if info.pbi_status == libc::SZOMB {
        return Ok(None);
    }
    let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: path is a writable byte buffer of the supplied length.
    let count =
        unsafe { libc::proc_pidpath(pid as i32, path.as_mut_ptr().cast(), path.len() as u32) };
    if count <= 0 {
        return Err(io::Error::last_os_error());
    }
    let len = path.iter().position(|b| *b == 0).unwrap_or(path.len());
    path.truncate(len);
    Ok(Some((
        format!("{}:{}", info.pbi_start_tvsec, info.pbi_start_tvusec),
        PathBuf::from(std::ffi::OsString::from_vec(path)),
    )))
}
#[cfg(target_os = "linux")]
fn platform_identity(pid: u32) -> io::Result<Option<(String, PathBuf)>> {
    let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .ok_or_else(|| io::Error::other("Invalid process stat"))?
        .1
        .split_whitespace()
        .collect();
    if fields.first() == Some(&"Z") {
        return Ok(None);
    }
    let birth = fields
        .get(19)
        .ok_or_else(|| io::Error::other("Missing process birth"))?
        .to_string();
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let executable = std::fs::read_link(format!("/proc/{pid}/exe"))?;
    let bytes = executable.as_os_str().as_bytes();
    let executable = PathBuf::from(std::ffi::OsString::from_vec(
        bytes.strip_suffix(b" (deleted)").unwrap_or(bytes).to_vec(),
    ));
    Ok(Some((birth, executable)))
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("Native process identity currently supports macOS and Linux only");

/// Corroborate an authenticated daemon without mistaking a rebuilt file for a new process.
/// The control response must still match the entire originally recorded identity.
pub fn matches(expected: &ProcessIdentity) -> io::Result<bool> {
    Ok(platform_identity(expected.pid)?
        .is_some_and(|(birth, path)| birth == expected.birth && path == expected.executable))
}

pub fn detach() -> io::Result<()> {
    // SAFETY: setsid has no pointer arguments and is called before starting runtime threads.
    if unsafe { libc::setsid() } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
