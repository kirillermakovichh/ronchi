//! Read-only lookup through the existing indexed navigation projection.
use super::{JournalError, JournalReader};
use crate::{checks::Report, query::EventRef};
use rusqlite::OptionalExtension;
impl JournalReader {
    pub(crate) fn check_collection(
        &self,
        id: &str,
    ) -> Result<Option<(String, Report)>, JournalError> {
        let row:Option<(i64,String,Option<String>)>=self.connection.query_row("SELECT event_seq,json_extract(metadata_json,'$.data.arguments_digest'),json_extract(metadata_json,'$.data.report') FROM query_events WHERE request_id=?1 AND kind IN ('check.collection_started','check.collection_completed') ORDER BY event_seq DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        row.map(|(seq, digest, text)| {
            let mut report: Report =
                serde_json::from_str(&text.ok_or(JournalError::InvalidInput)?)?;
            report.completion_ref = Some(EventRef {
                lab_id: report.lab_id.clone(),
                event_seq: seq as u64,
            });
            report.snapshot_seq = report.snapshot_seq.max(seq as u64);
            Ok((digest, report))
        })
        .transpose()
    }
}
