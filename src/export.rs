//! Bounded evidence export. This format is an inspection artifact, not replay input.
use crate::{
    lab::{LabError, Lifecycle},
    query::{self, Arguments, Engine, Operation, Request},
    workspace::{self, Workspace},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub const MAX_EXPORT_BYTES: u64 = 64 * 1024 * 1024;
const FOOTER_RESERVE: u64 = 8192;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportOptions {
    /// Identifier for a new file in the selected workspace's exports directory.
    pub name: String,
    #[schemars(range(min = 16384, max = 67108864))]
    pub max_bytes: u64,
    pub arguments: Arguments,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ExportResult {
    pub lab_id: String,
    pub snapshot_seq: u64,
    pub path: PathBuf,
    pub bytes: u64,
    pub pages: u32,
    pub selection_complete: bool,
    pub payload_complete: bool,
    pub truncated: bool,
    pub reason: Option<String>,
    pub next_cursor: Option<String>,
    pub replayable: bool,
}
struct Output {
    file: Option<File>,
    path: PathBuf,
    committed: bool,
}
impl Output {
    fn finish(mut self) -> Result<(), LabError> {
        self.file
            .as_ref()
            .ok_or_else(|| LabError::internal("Export file is closed"))?
            .sync_all()?;
        self.committed = true;
        Ok(())
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        if !self.committed {
            self.file.take();
            let _ = fs::remove_file(&self.path);
        }
    }
}
async fn write(mut output: Output, bytes: Vec<u8>) -> Result<Output, LabError> {
    tokio::task::spawn_blocking(move || {
        output
            .file
            .as_mut()
            .ok_or_else(|| LabError::internal("Export file is closed"))?
            .write_all(&bytes)?;
        Ok(output)
    })
    .await
    .map_err(|_| LabError::internal("Export writer failed"))?
}
fn encode(value: &impl Serialize) -> Result<Vec<u8>, LabError> {
    let mut bytes =
        serde_json::to_vec(value).map_err(|_| LabError::internal("Export encoding failed"))?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn query_error(error: query::Error) -> LabError {
    let mut result = LabError::internal(error.message.to_string());
    result.exit = error.exit();
    result.code = match error.code.as_ref() {
        "invalid_argument" => "invalid_argument",
        "invalid_field" => "invalid_field",
        "invalid_cursor" => "invalid_cursor",
        "cancelled" => "cancelled",
        "deadline_exceeded" => "deadline_exceeded",
        "journal_unavailable" => "journal_unavailable",
        "state_conflict" => "state_conflict",
        _ => "internal_error",
    };
    result
}
/// Stream one explicitly selected Lab into bounded page records. Each page releases its reader.
/// An interrupted export removes only the new incomplete output, never original evidence.
pub async fn export(
    project: PathBuf,
    lab_id: &str,
    options: ExportOptions,
    cancel: CancellationToken,
) -> Result<ExportResult, LabError> {
    if !workspace::identifier(&options.name)
        || !(16384..=MAX_EXPORT_BYTES).contains(&options.max_bytes)
    {
        return Err(LabError::invalid(
            "Export requires an identifier name and 16384..67108864 output bytes",
        ));
    }
    if options.arguments.cursor.is_some() {
        return Err(LabError::invalid(
            "Export starts a new bounded evidence selection; cursors are not export inputs",
        ));
    }
    let mut request = Request {
        contract_version: 1,
        request_id: "export".into(),
        lab_id: lab_id.into(),
        operation: Operation::Evidence,
        arguments: options.arguments,
    };
    request.normalize().map_err(query_error)?;
    let ws = Workspace::new(&project);
    let _maintenance = crate::maintenance::read_lock(&ws, lab_id)?;
    let selected = crate::lab::status(project, "default", Some(lab_id)).await?;
    let manifest = selected.manifest;
    let alive = matches!(manifest.state, Lifecycle::Ready | Lifecycle::Stopping);
    let journal = ws.lab(lab_id)?.join("journal.sqlite");
    if fs::symlink_metadata(&journal)?.file_type().is_symlink() {
        return Err(LabError::invalid("Journal must not be a symlink"));
    }
    let metadata_path = journal.clone();
    let journal_schema_version = tokio::task::spawn_blocking(move || {
        crate::journal::JournalReader::open(&metadata_path)
            .map(|reader| reader.schema_version())
            .map_err(LabError::from)
    })
    .await
    .map_err(|_| LabError::internal("Journal version reader failed"))??;
    let engine = Engine::default();
    // Resolve and validate the snapshot before creating output.
    let mut first = engine
        .execute(
            journal.clone(),
            alive,
            "explicit",
            request.clone(),
            cancel.clone(),
        )
        .await
        .map_err(query_error)?;
    request.arguments.snapshot_seq = Some(first.view.snapshot_seq);
    let header = encode(
        &json!({"type":"header","format":"ronchi-evidence-ndjson-v1","contract_version":1,"lab_id":lab_id,"snapshot_seq":first.view.snapshot_seq,"scope":first.view.scope,"fields":first.view.fields,"versions":manifest.versions,"journal_schema_version":journal_schema_version,"journal_state":first.view.journal_state,"redaction_policy":"typed-facts-v1-preserved","include_payload":request.arguments.include_payload,"payload_offset":request.arguments.payload_offset,"omitted_payload_classes":if request.arguments.include_payload {json!(["raw_attachments","unselected_payload_bytes"])} else {json!(["retained_signed_bytes","raw_attachments","retained_payloads"])},"max_bytes":options.max_bytes,"replayable":false}),
    )?;
    if header.len() as u64 + FOOTER_RESERVE > options.max_bytes {
        return Err(LabError::invalid(
            "Export metadata exceeds its output-byte budget",
        ));
    }
    let first_cap = ((options.max_bytes - header.len() as u64 - FOOTER_RESERVE) as usize)
        .min(query::MAX_RESPONSE_BYTES)
        .saturating_sub(64);
    let initial_page = Engine::with_limits(query::Limits {
        response_bytes: first_cap.max(1),
        ..Default::default()
    })
    .map_err(query_error)?
    .execute(
        journal.clone(),
        alive,
        "explicit",
        request.clone(),
        cancel.clone(),
    )
    .await;
    let initial_too_large = initial_page
        .as_ref()
        .is_err_and(|error| error.code.as_ref() == "result_too_large");
    if !initial_too_large {
        first = initial_page.map_err(query_error)?;
    }
    let directory = ws.root.join("exports");
    workspace::private_directory(&directory)?;
    let path = directory.join(format!("{}.ndjson", options.name));
    use std::os::unix::fs::OpenOptionsExt;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|_| {
            LabError::conflict("Export name already exists or cannot be created; choose a new name")
        })?;
    let header_len = header.len() as u64;
    let mut output = write(
        Output {
            file: Some(file),
            path: path.clone(),
            committed: false,
        },
        header,
    )
    .await?;
    let mut bytes = header_len;
    let mut pages = 0;
    let mut payload_complete =
        !request.arguments.include_payload || request.arguments.payload_offset == 0;
    let mut response = first;
    let mut complete = false;
    let mut reason = None;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if cancel.is_cancelled() {
            return Err(query_error(query::Error::new(
                "cancelled",
                "Evidence export cancelled; original Journal is unchanged",
            )));
        }
        if initial_too_large && pages == 0 {
            reason = Some("output_bytes".into());
            break;
        }
        let record = encode(&json!({"type":"page","response":response}))?;
        if bytes + record.len() as u64 + FOOTER_RESERVE > options.max_bytes {
            reason = Some("output_bytes".into());
            break;
        }
        if request.arguments.include_payload
            && response.result["items"].as_array().is_some_and(|items| {
                items.iter().any(|i| {
                    i["payload"]["payload_truncated"] == true
                        || matches!(
                            i["payload_status"].as_str(),
                            Some("redacted" | "lost" | "omitted_by_policy")
                        )
                })
            })
        {
            payload_complete = false;
        }
        let record_len = record.len() as u64;
        output = write(output, record).await?;
        bytes += record_len;
        pages += 1;
        request.arguments.cursor = response.page.next_cursor.clone();
        if request.arguments.cursor.is_none() {
            complete = response.view.selection_complete;
            if !complete {
                reason = response
                    .page
                    .reason
                    .clone()
                    .or_else(|| Some("query_incomplete".into()));
            }
            break;
        }
        if pages == 512 || Instant::now() >= deadline {
            reason = Some(
                if pages == 512 {
                    "page_limit"
                } else {
                    "deadline"
                }
                .into(),
            );
            break;
        }
        let remaining = options.max_bytes - bytes - FOOTER_RESERVE;
        if remaining < 2048 {
            reason = Some("output_bytes".into());
            break;
        }
        let engine = Engine::with_limits(query::Limits {
            response_bytes: (remaining as usize).min(query::MAX_RESPONSE_BYTES) - 64,
            ..Default::default()
        })
        .map_err(query_error)?;
        match engine
            .execute(
                journal.clone(),
                alive,
                "explicit",
                request.clone(),
                cancel.clone(),
            )
            .await
        {
            Ok(page) => response = page,
            Err(error) if error.code.as_ref() == "result_too_large" => {
                reason = Some("output_bytes".into());
                break;
            }
            Err(error) => return Err(query_error(error)),
        }
    }
    if complete && !payload_complete {
        reason = Some("payload_chunks".into());
    }
    let result = ExportResult {
        lab_id: lab_id.into(),
        snapshot_seq: response.view.snapshot_seq,
        path,
        bytes: 0,
        pages,
        selection_complete: complete,
        payload_complete,
        truncated: !complete || !payload_complete,
        reason,
        next_cursor: request.arguments.cursor.clone(),
        replayable: false,
    };
    let footer = encode(
        &json!({"type":"footer","lab_id":lab_id,"snapshot_seq":result.snapshot_seq,"pages":pages,"selection_complete":complete,"payload_complete":payload_complete,"truncated":result.truncated,"reason":result.reason,"next_cursor":result.next_cursor,"original_journal_unchanged":true}),
    )?;
    if bytes + footer.len() as u64 > options.max_bytes {
        return Err(LabError::internal(
            "Export footer exceeded its reserved budget",
        ));
    }
    bytes += footer.len() as u64;
    output = write(output, footer).await?;
    tokio::task::spawn_blocking(move || output.finish())
        .await
        .map_err(|_| LabError::internal("Export finalization worker failed"))??;
    Ok(ExportResult { bytes, ..result })
}
