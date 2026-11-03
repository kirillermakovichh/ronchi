//! Bounded durable event storage. Callers supply validated, redacted event facts.
//! This module does not infer application outcomes or restore a running Lab.
mod budget;
mod cases;
mod checks;
pub use budget::{DEFAULT_DISK_BYTES, DiskLimits, DiskUsage, MIN_SHUTDOWN_RESERVE};
mod evaluate;
mod findings;
mod nonces;
mod query;
mod query_index;
mod response_loss;
mod store;
mod transactions;
pub use transactions::{TransactionPhase, TransactionRecord};
mod writer;
use serde::{Deserialize, Serialize};
use std::time::Duration;
pub use store::{JournalReader, JournalStore};
pub use writer::{
    CommitReceipt, JournalWriter, QueueLimits, TransitionReservation, WriterHandle, WriterHealth,
    WriterStatus,
};

pub const MAX_BATCH_EVENTS: usize = 256;
pub const MAX_BATCH_BYTES: usize = 1024 * 1024;
pub const MAX_EVENT_BYTES: usize = 64 * 1024;
pub const BUSY_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("Journal storage failed")]
    Storage(#[from] rusqlite::Error),
    #[error("Journal event cannot be encoded")]
    Encoding(#[from] serde_json::Error),
    #[error("Journal input exceeds its bounds or is invalid")]
    InvalidInput,
    #[error("Journal schema is not supported")]
    UnsupportedSchema,
    #[error("Journal is already closed")]
    Closed,
    #[error("Journal admission capacity exhausted")]
    Saturated,
    #[error("Lab evidence disk budget exhausted")]
    DiskBudget,
    #[error("Filesystem cannot sustain the Lab shutdown reserve")]
    DiskSpace,
    #[error("Lab evidence filesystem accounting failed")]
    Filesystem(#[source] std::io::Error),
    #[error("Journal writer failed; inspect Lab health before retrying")]
    WriterFailed,
    #[error("Journal writer thread could not start")]
    Thread(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalState {
    Open,
    Finalized,
    Interrupted,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub lab_id: String,
    pub snapshot_seq: u64,
    pub state: JournalState,
}

/// A version-1 storage fact. Each event kind defines its own redacted data schema.
/// Full protocol event families are added with their owning implementation slices.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub schema_version: u32,
    pub kind: String,
    pub recorded_at: String,
    pub elapsed_ns: u64,
    pub source: String,
    pub execution_mode: crate::config::Mode,
    pub data: serde_json::Value,
}

fn encode(events: &[Event]) -> Result<Vec<String>, JournalError> {
    if events.is_empty() || events.len() > MAX_BATCH_EVENTS {
        return Err(JournalError::InvalidInput);
    }
    let mut bytes = 0;
    events
        .iter()
        .map(|event| {
            if event.schema_version != 1
                || event.kind.is_empty()
                || event.kind.len() > 128
                || event.source.is_empty()
                || event.source.len() > 128
                || event.recorded_at.len() > 64
                || event.elapsed_ns > i64::MAX as u64
            {
                return Err(JournalError::InvalidInput);
            }
            let mut buffer = BoundedBuffer(Vec::new());
            serde_json::to_writer(&mut buffer, event).map_err(|_| JournalError::InvalidInput)?;
            let json = String::from_utf8(buffer.0).map_err(|_| JournalError::InvalidInput)?;
            bytes += json.len();
            if json.len() > MAX_EVENT_BYTES || bytes > MAX_BATCH_BYTES {
                return Err(JournalError::InvalidInput);
            }
            Ok(json)
        })
        .collect()
}

struct BoundedBuffer(Vec<u8>);
impl std::io::Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_EVENT_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("Event exceeds byte limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
