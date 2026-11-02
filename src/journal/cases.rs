use super::{JournalError, JournalReader};
use rusqlite::OptionalExtension;
pub(super) const SCHEMA:&str="
CREATE TABLE case_policy (singleton INTEGER PRIMARY KEY CHECK(singleton=1), binding_json TEXT NOT NULL CHECK(json_valid(binding_json))) STRICT;
CREATE TRIGGER case_change AFTER INSERT ON events WHEN json_extract(NEW.event_json,'$.kind')='case.changed' BEGIN
 INSERT INTO case_policy VALUES(1,json_extract(NEW.event_json,'$.data.binding')) ON CONFLICT(singleton) DO UPDATE SET binding_json=excluded.binding_json;
 END;
PRAGMA user_version=5;";
impl JournalReader {
    pub fn case_binding(&self) -> Result<Option<crate::cases::Binding>, JournalError> {
        if self.version < 5 {
            return Ok(None);
        }
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT binding_json FROM case_policy WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|value| serde_json::from_str(&value).map_err(JournalError::from))
            .transpose()
    }
}
