//! Atomic protocol-state projection of committed transaction facts.
use super::{JournalError, JournalReader};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransactionPhase {
    Pending,
    Processed,
    Confirmed,
    Finalized,
    Expired,
    Unprocessed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub signature: String,
    pub wire_base64: String,
    pub digest: String,
    pub version: i16,
    pub blockhash: String,
    pub accepted_ns: u64,
    pub admission_slot: u64,
    pub admission_height: u64,
    pub process_at_ns: u64,
    pub confirmed_at_ns: u64,
    pub finalized_at_ns: u64,
    pub hash_height: Option<u64>,
    pub last_valid_height: Option<u64>,
    pub expiry_at_ns: Option<u64>,
    pub next_due_ns: Option<u64>,
    pub stage: TransactionPhase,
    pub processed_slot: Option<u64>,
    pub processed_height: Option<u64>,
    pub admission_request: serde_json::Value,
}
pub(super) const SCHEMA:&str="
CREATE TABLE transactions (
 signature TEXT PRIMARY KEY NOT NULL,
 record_json TEXT NOT NULL CHECK(json_valid(record_json)),
 next_due_ns INTEGER CHECK(next_due_ns>=0),
 acceptance_seq INTEGER NOT NULL UNIQUE REFERENCES events(event_seq)
) STRICT;
CREATE INDEX transactions_due ON transactions(next_due_ns,signature) WHERE next_due_ns IS NOT NULL;
CREATE TRIGGER transaction_admission AFTER INSERT ON events
 WHEN json_extract(NEW.event_json,'$.kind')='transaction.accepted' BEGIN
 INSERT INTO transactions VALUES(json_extract(NEW.event_json,'$.data.signature'),json_extract(NEW.event_json,'$.data'),json_extract(NEW.event_json,'$.data.next_due_ns'),NEW.event_seq);
 END;
CREATE TRIGGER transaction_transition AFTER INSERT ON events
 WHEN json_extract(NEW.event_json,'$.kind')='transaction.transition' BEGIN
 UPDATE transactions SET record_json=json_set(record_json,
 '$.stage',json_extract(NEW.event_json,'$.data.stage'),
 '$.processed_slot',json_extract(NEW.event_json,'$.data.processed_slot'),
 '$.processed_height',json_extract(NEW.event_json,'$.data.processed_height'),
 '$.next_due_ns',json_extract(NEW.event_json,'$.data.next_due_ns')),
 next_due_ns=json_extract(NEW.event_json,'$.data.next_due_ns')
 WHERE signature=json_extract(NEW.event_json,'$.data.signature');
 SELECT CASE WHEN changes()!=1 THEN RAISE(ABORT,'transition identity is absent') END;
 END;
PRAGMA user_version=2;";
impl JournalReader {
    pub fn transaction(&self, signature: &str) -> Result<Option<TransactionRecord>, JournalError> {
        if self.version < 2 {
            return Ok(None);
        }
        let text:Option<String>=self.connection.query_row("SELECT CASE WHEN length(record_json)<=65536 THEN record_json ELSE NULL END FROM transactions WHERE signature=?1",[signature],|r|r.get(0)).optional()?;
        text.map(|s| serde_json::from_str(&s).map_err(JournalError::from))
            .transpose()
    }
    pub fn next_transaction(&self) -> Result<Option<TransactionRecord>, JournalError> {
        if self.version < 2 {
            return Ok(None);
        }
        let text:Option<String>=self.connection.query_row("SELECT CASE WHEN length(record_json)<=65536 THEN record_json ELSE NULL END FROM transactions WHERE next_due_ns IS NOT NULL ORDER BY next_due_ns,signature LIMIT 1",[],|r|r.get(0)).optional()?;
        text.map(|s| serde_json::from_str(&s).map_err(JournalError::from))
            .transpose()
    }
    pub fn has_transactions(&self) -> Result<bool, JournalError> {
        if self.version < 2 {
            return Ok(false);
        }
        Ok(self
            .connection
            .query_row("SELECT EXISTS(SELECT 1 FROM transactions)", [], |r| {
                r.get(0)
            })?)
    }
}
