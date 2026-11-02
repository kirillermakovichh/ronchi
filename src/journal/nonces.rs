use super::{JournalError, JournalReader};
use crate::protocol::nonce::NonceAccount;
use rusqlite::OptionalExtension;
pub(super) const SCHEMA:&str="
CREATE TABLE nonce_versions (
 address TEXT NOT NULL,
 event_seq INTEGER NOT NULL REFERENCES events(event_seq),
 slot INTEGER NOT NULL CHECK(slot>=0),
 record_json TEXT NOT NULL CHECK(json_valid(record_json)),
 PRIMARY KEY(address,event_seq)
) STRICT;
CREATE INDEX nonce_context ON nonce_versions(address,slot DESC,event_seq DESC);
CREATE TRIGGER nonce_fixture AFTER INSERT ON events
 WHEN json_extract(NEW.event_json,'$.kind')='protocol.fixture' BEGIN
 INSERT INTO nonce_versions SELECT json_extract(value,'$.address'),NEW.event_seq,json_extract(value,'$.slot'),value
 FROM json_each(NEW.event_json,'$.data.nonce_accounts');
 END;
CREATE TRIGGER nonce_advance AFTER INSERT ON events
 WHEN json_extract(NEW.event_json,'$.kind')='transaction.transition' AND json_type(NEW.event_json,'$.data.nonce_advance')='object' BEGIN
 SELECT CASE WHEN (SELECT json_extract(record_json,'$.nonce') FROM nonce_versions WHERE address=json_extract(NEW.event_json,'$.data.nonce_advance.address') ORDER BY event_seq DESC LIMIT 1) IS NOT json_extract(NEW.event_json,'$.data.nonce_advance.previous_nonce') THEN RAISE(ABORT,'nonce advancement conflicts with committed state') END;
 INSERT INTO nonce_versions VALUES(json_extract(NEW.event_json,'$.data.nonce_advance.address'),NEW.event_seq,json_extract(NEW.event_json,'$.data.nonce_advance.slot'),json_remove(json_extract(NEW.event_json,'$.data.nonce_advance'),'$.previous_nonce'));
 END;
PRAGMA user_version=3;";
impl JournalReader {
    /// The selected slot pins the historical nonce bank view. None selects latest committed state.
    pub fn nonce(
        &self,
        address: &str,
        slot: Option<u64>,
    ) -> Result<Option<NonceAccount>, JournalError> {
        if self.version < 3 {
            return Ok(None);
        }
        let slot = slot.unwrap_or(i64::MAX as u64).min(i64::MAX as u64) as i64;
        let text:Option<String>=self.connection.query_row("SELECT record_json FROM nonce_versions WHERE address=?1 AND slot<=?2 ORDER BY slot DESC,event_seq DESC LIMIT 1",rusqlite::params![address,slot],|r|r.get(0)).optional()?;
        text.map(|text| serde_json::from_str(&text).map_err(JournalError::from))
            .transpose()
    }
}
