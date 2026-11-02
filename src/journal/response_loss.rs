use super::{JournalError, JournalReader};
use rusqlite::OptionalExtension;
pub(super) const SCHEMA: &str = "
CREATE TABLE response_loss (
 signature TEXT PRIMARY KEY NOT NULL REFERENCES transactions(signature),
 selection_seq INTEGER NOT NULL UNIQUE REFERENCES events(event_seq)
) STRICT;
CREATE TRIGGER response_loss_selected AFTER INSERT ON events
 WHEN json_extract(NEW.event_json,'$.kind')='fault.response_loss_selected' BEGIN
 INSERT INTO response_loss VALUES(json_extract(NEW.event_json,'$.data.signature'),NEW.event_seq);
 END;
PRAGMA user_version=4;";
impl JournalReader {
    pub fn response_loss(&self, signature: &str) -> Result<Option<u64>, JournalError> {
        if self.version < 4 {
            return Ok(None);
        }
        self.connection
            .query_row(
                "SELECT selection_seq FROM response_loss WHERE signature=?1",
                [signature],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|value| value.map(|seq| seq as u64))
            .map_err(JournalError::from)
    }
}
