use super::{BUSY_TIMEOUT, Event, JournalError, JournalState, Snapshot, encode};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use std::path::Path;

pub struct JournalStore {
    connection: Connection,
}
pub struct JournalReader {
    connection: Connection,
}
const SCHEMA: &str = "
CREATE TABLE journal (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 lab_id TEXT NOT NULL CHECK(length(lab_id) BETWEEN 1 AND 128),
 snapshot_seq INTEGER NOT NULL CHECK(snapshot_seq>=0),
 state TEXT NOT NULL CHECK(state IN ('open','finalized','interrupted'))
) STRICT;
CREATE TABLE events (
 event_seq INTEGER PRIMARY KEY CHECK(event_seq>0),
 event_json TEXT NOT NULL CHECK(json_valid(event_json))
) STRICT;
PRAGMA user_version=1;";

fn snapshot(connection: &Connection, recovered: bool) -> Result<Snapshot, JournalError> {
    let (lab_id, seq, state): (String, i64, String) = connection.query_row(
        "SELECT lab_id,snapshot_seq,state FROM journal WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let state = match state.as_str() {
        "open" if recovered => JournalState::Interrupted,
        "open" => JournalState::Open,
        "finalized" => JournalState::Finalized,
        "interrupted" => JournalState::Interrupted,
        _ => return Err(JournalError::UnsupportedSchema),
    };
    Ok(Snapshot {
        lab_id,
        snapshot_seq: u64::try_from(seq).map_err(|_| JournalError::UnsupportedSchema)?,
        state,
    })
}

impl JournalStore {
    /// Creates a new file-backed Journal. Existing schema is never overwritten.
    /// The lifecycle owner must serialize ownership of the supplied Lab path.
    pub fn create(path: &Path, lab_id: &str) -> Result<Self, JournalError> {
        if lab_id.is_empty()
            || lab_id.len() > 128
            || !lab_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(JournalError::InvalidInput);
        }
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        let version: u32 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let objects: u32 =
            connection.query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0))?;
        if version != 0 || objects != 0 {
            return Err(JournalError::UnsupportedSchema);
        }
        let mode: String = connection.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "wal_autocheckpoint", 1000)?;
        let sync: u32 = connection.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
        if mode != "wal" || sync != 2 {
            return Err(JournalError::UnsupportedSchema);
        }
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(SCHEMA)?;
        tx.execute("INSERT INTO journal VALUES(1,?1,0,'open')", [lab_id])?;
        tx.commit()?;
        Ok(Self { connection })
    }

    pub fn snapshot(&self) -> Result<Snapshot, JournalError> {
        snapshot(&self.connection, false)
    }

    /// Acknowledges only committed contiguous sequences. A failed unit is atomic.
    pub fn append(&mut self, events: &[Event]) -> Result<u64, JournalError> {
        self.commit(encode(events)?, None)
    }

    /// Writes a terminal fact and its state in the same transaction.
    /// The lifecycle owner must settle producers before requesting finalization.
    pub fn finish(&mut self, terminal: &Event, state: JournalState) -> Result<u64, JournalError> {
        if state == JournalState::Open {
            return Err(JournalError::InvalidInput);
        }
        self.commit(encode(std::slice::from_ref(terminal))?, Some(state))
    }

    fn commit(
        &mut self,
        encoded: Vec<String>,
        terminal: Option<JournalState>,
    ) -> Result<u64, JournalError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = snapshot(&tx, false)?;
        if current.state != JournalState::Open {
            return Err(JournalError::Closed);
        }
        let last = current
            .snapshot_seq
            .checked_add(encoded.len() as u64)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or(JournalError::InvalidInput)?;
        {
            let mut insert = tx.prepare_cached("INSERT INTO events VALUES(?1,?2)")?;
            for (offset, event) in encoded.into_iter().enumerate() {
                insert.execute(params![
                    (current.snapshot_seq + offset as u64 + 1) as i64,
                    event
                ])?;
            }
        }
        let state = match terminal {
            Some(JournalState::Finalized) => "finalized",
            Some(JournalState::Interrupted) => "interrupted",
            _ => "open",
        };
        tx.execute(
            "UPDATE journal SET snapshot_seq=?1,state=?2 WHERE singleton=1",
            params![last as i64, state],
        )?;
        tx.commit()?;
        Ok(last)
    }
}

impl JournalReader {
    pub fn open(path: &Path) -> Result<Self, JournalError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        let version: u32 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != 1 {
            return Err(JournalError::UnsupportedSchema);
        }
        Ok(Self { connection })
    }

    /// `owner_alive` must come from lifecycle authentication, never a PID alone.
    pub fn snapshot(&self, owner_alive: bool) -> Result<Snapshot, JournalError> {
        snapshot(&self.connection, !owner_alive)
    }

    /// Reads one committed event. Bounded query/pagination belongs to task 8.
    pub fn event(&self, seq: u64) -> Result<Option<Event>, JournalError> {
        use rusqlite::OptionalExtension;
        let seq = i64::try_from(seq).map_err(|_| JournalError::InvalidInput)?;
        let json: Option<String> = self.connection.query_row(
            "SELECT event_json FROM events WHERE event_seq=?1 AND event_seq <= (SELECT snapshot_seq FROM journal WHERE singleton=1)", [seq], |r| r.get(0),
        ).optional()?;
        json.map(|text| serde_json::from_str(&text).map_err(JournalError::from))
            .transpose()
    }
}
