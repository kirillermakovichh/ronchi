use super::budget::{Budget, Permit};
use super::{
    BUSY_TIMEOUT, DiskLimits, DiskUsage, Event, JournalError, JournalState, Snapshot, encode,
};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use std::path::Path;

pub struct JournalStore {
    connection: Connection,
    pub(super) budget: Budget,
    pub(super) path: std::path::PathBuf,
}
pub struct JournalReader {
    pub(super) version: u32,
    pub(super) connection: Connection,
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
        Self::with_disk_limits(path, lab_id, DiskLimits::default())
    }
    pub fn with_disk_limits(
        path: &Path,
        lab_id: &str,
        limits: DiskLimits,
    ) -> Result<Self, JournalError> {
        if lab_id.is_empty()
            || lab_id.len() > 128
            || !lab_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(JournalError::InvalidInput);
        }
        let budget = Budget::new(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
            path.file_name().ok_or(JournalError::InvalidInput)?,
            limits,
        )?;
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
        connection.pragma_update(None, "cache_spill", "OFF")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "max_page_count", 32)?;
        connection.pragma_update(None, "wal_autocheckpoint", 1000)?;
        let foreign_keys: u32 = connection.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
        let spill: u32 = connection.query_row("PRAGMA cache_spill", [], |r| r.get(0))?;
        let sync: u32 = connection.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
        if mode != "wal" || sync != 2 || spill != 0 || foreign_keys != 1 {
            return Err(JournalError::UnsupportedSchema);
        }
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(SCHEMA)?;
        tx.execute_batch(super::transactions::SCHEMA)?;
        tx.execute_batch(super::nonces::SCHEMA)?;
        tx.execute_batch(super::response_loss::SCHEMA)?;
        tx.execute_batch(super::cases::SCHEMA)?;
        tx.execute("INSERT INTO journal VALUES(1,?1,0,'open')", [lab_id])?;
        tx.commit()?;
        let pages: u32 = connection.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: u32 = connection.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        budget.initialize(u64::from(page_size), u64::from(pages))?;
        Ok(Self {
            connection,
            budget,
            path: path.to_path_buf(),
        })
    }

    pub fn disk_usage(&self) -> Result<DiskUsage, JournalError> {
        self.budget.usage()
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
        if state == JournalState::Finalized && self.budget.failed() {
            return Err(JournalError::WriterFailed);
        }
        if state == JournalState::Open {
            return Err(JournalError::InvalidInput);
        }
        self.commit(encode(std::slice::from_ref(terminal))?, Some(state))
    }

    pub(super) fn commit(
        &mut self,
        encoded: Vec<String>,
        terminal: Option<JournalState>,
    ) -> Result<u64, JournalError> {
        let permits = if terminal.is_some() {
            Vec::new()
        } else {
            vec![self.budget.reserve(&encoded)?]
        };
        self.commit_reserved(encoded, terminal, permits)
    }
    pub(super) fn commit_reserved(
        &mut self,
        encoded: Vec<String>,
        terminal: Option<JournalState>,
        permits: Vec<Permit>,
    ) -> Result<u64, JournalError> {
        let budget = self.budget.clone();
        budget.commit(&permits, terminal.is_some(), |max_pages| {
            self.connection.pragma_update(
                None,
                "max_page_count",
                u32::try_from(max_pages).map_err(|_| JournalError::DiskBudget)?,
            )?;
            let seq = self.write_transaction(encoded, terminal)?;
            let pages: u32 = self
                .connection
                .query_row("PRAGMA page_count", [], |r| r.get(0))?;
            Ok((seq, u64::from(pages)))
        })
    }
    fn write_transaction(
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
        if !matches!(version, 1..=5) {
            return Err(JournalError::UnsupportedSchema);
        }
        Ok(Self {
            connection,
            version,
        })
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
