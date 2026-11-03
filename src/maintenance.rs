pub(crate) mod cli;
use crate::{
    lab::LabError,
    workspace::{self, Workspace},
};
use fs2::FileExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeleteResult {
    pub deleted_lab_id: String,
    pub evidence_references_invalidated: bool,
    pub independent_exports_preserved: bool,
}

pub(crate) fn lock(ws: &Workspace, name: &str, kind: &str) -> Result<fs::File, LabError> {
    let file = ws.lock(name, kind)?;
    file.try_lock_exclusive()
        .map_err(|_| LabError::conflict("Lab maintenance or lifecycle operation is in progress"))?;
    Ok(file)
}

/// Reject links before destructive work. Deletion never follows user-supplied links.
fn check_tree(path: &Path, depth: usize, remaining: &mut usize) -> Result<(), LabError> {
    if depth > 16 || *remaining == 0 {
        return Err(LabError::conflict(
            "Lab artifacts exceed the bounded deletion inventory",
        ));
    }
    *remaining -= 1;
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(LabError::conflict(
            "Lab artifacts contain a symlink; resolve it before deletion",
        ));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            check_tree(&entry?.path(), depth + 1, remaining)?;
        }
    } else if !metadata.is_file() {
        return Err(LabError::conflict(
            "Lab artifacts contain an unsupported filesystem entry",
        ));
    }
    Ok(())
}

/// Delete one exact stopped Lab. This operation never stops or signals processes.
pub async fn delete(project: PathBuf, lab_id: &str) -> Result<DeleteResult, LabError> {
    let ws = Workspace::new(&project);
    let selected = ws.manifest(lab_id)?;
    let _operation = lock(&ws, &selected.name, "operation")?;
    let _maintenance = lock(&ws, lab_id, "maintenance")?;
    let selected = ws.manifest(lab_id)?;
    let current = ws
        .target(&selected.name, None)
        .is_ok_and(|m| m.lab_id == lab_id);
    let _owner = if current {
        Some(lock(&ws, &selected.name, "owner")?)
    } else {
        None
    };
    for process in selected
        .process
        .iter()
        .chain(selected.owned_children.iter())
    {
        if crate::process::matches(process)? {
            return Err(LabError::conflict("The selected Lab or a recorded owned process is still alive; stop it before deletion").for_lab(lab_id));
        }
    }
    let path = ws.lab(lab_id)?;
    let name_path = ws
        .root
        .join("names")
        .join(format!("{}.json", selected.name));
    if !workspace::identifier(&selected.name) {
        return Err(LabError::invalid("Invalid retained Lab name"));
    }
    for path in [&ws.root, &ws.root.join("names")] {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(LabError::conflict(
                "Workspace paths must not contain symlinks",
            ));
        }
    }
    if current && fs::symlink_metadata(&name_path)?.file_type().is_symlink() {
        return Err(LabError::conflict(
            "Lab name reference must not be a symlink",
        ));
    }
    let id = lab_id.to_string();
    tokio::task::spawn_blocking(move || {
        let _locks = (_operation, _maintenance, _owner);
        check_tree(&path, 0, &mut 4096)?;
        fs::remove_dir_all(&path).map_err(|_| {
            let mut error =
                LabError::internal("Lab deletion could not complete; inspect remaining artifacts");
            error.effects = "partial";
            error.for_lab(&id)
        })?;
        if current {
            fs::remove_file(name_path).map_err(|_| {
                let mut error = LabError::internal(
                    "Lab artifacts were deleted but its name reference could not be removed",
                );
                error.effects = "partial";
                error.for_lab(&id)
            })?;
        }
        Ok(DeleteResult {
            deleted_lab_id: id,
            evidence_references_invalidated: true,
            independent_exports_preserved: true,
        })
    })
    .await
    .map_err(|_| LabError::internal("Lab deletion worker failed"))?
}

pub use crate::export::{ExportOptions, ExportResult, export};

/// Hold a shared lease while reading retained evidence so deletion cannot unlink it.
pub fn read_lock(ws: &Workspace, lab_id: &str) -> Result<fs::File, LabError> {
    ws.manifest(lab_id)?;
    let file = ws.lock(lab_id, "maintenance")?;
    FileExt::try_lock_shared(&file)
        .map_err(|_| LabError::conflict("Lab evidence maintenance is in progress"))?;
    ws.manifest(lab_id)?;
    Ok(file)
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteRequest {
    pub contract_version: u32,
    pub request_id: String,
    pub lab_id: String,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    pub contract_version: u32,
    pub request_id: String,
    pub lab_id: String,
    pub arguments: ExportOptions,
}
#[derive(Serialize, JsonSchema)]
pub struct Response<T> {
    pub contract_version: u32,
    pub request_id: String,
    pub lab_id: String,
    pub result: T,
}
#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
pub enum Envelope<T> {
    Success(Response<T>),
    Failure(crate::ErrorEnvelope),
}
