//! Constant-space Lab-wide RPC counters. Only the writer publishes committed updates.
use super::{Event, JournalError, JournalReader};
use serde::{Deserialize, Serialize};

const SECOND: u64 = 1_000_000_000;
const GAP: u64 = 10 * SECOND;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct RpcTraffic {
    pub requests: u64,
    pub first_elapsed_ns: Option<u64>,
    pub last_rpc_elapsed_ns: Option<u64>,
    pub active_start_elapsed_ns: u64,
    /// Sum of closed active intervals, each with a one-second minimum.
    pub active_completed_ns: u64,
    pub observed_elapsed_ns: u64,
    pub end_known: bool,
    /// Set only by a live control snapshot, never by a retained reader.
    pub running: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Update {
    elapsed_ns: u64,
    registration: bool,
    terminal: bool,
}
pub(super) fn updates(events: &[Event]) -> Vec<Update> {
    events
        .iter()
        .filter(|e| e.schema_version == 1)
        .map(|e| Update {
            elapsed_ns: e.elapsed_ns,
            registration: e.source == "ronchi" && e.kind == "rpc.registered",
            terminal: e.source == "ronchi"
                && matches!(e.kind.as_str(), "lab.finalized" | "lab.interrupted"),
        })
        .collect()
}
impl RpcTraffic {
    pub(super) fn apply(&mut self, updates: impl IntoIterator<Item = Update>) {
        for update in updates {
            self.observed_elapsed_ns = self.observed_elapsed_ns.max(update.elapsed_ns);
            self.end_known |= update.terminal;
            if !update.registration {
                continue;
            }
            // Concurrent producers can enqueue slightly out of timestamp order.
            let now = update.elapsed_ns.max(self.last_rpc_elapsed_ns.unwrap_or(0));
            if let Some(last) = self.last_rpc_elapsed_ns {
                if now.saturating_sub(last) >= GAP {
                    self.active_completed_ns = self.active_completed_ns.saturating_add(
                        last.saturating_sub(self.active_start_elapsed_ns)
                            .max(SECOND),
                    );
                    self.active_start_elapsed_ns = now;
                }
            } else {
                self.first_elapsed_ns = Some(now);
                self.active_start_elapsed_ns = now;
            }
            self.requests = self.requests.saturating_add(1);
            self.last_rpc_elapsed_ns = Some(now);
        }
    }
}

impl JournalReader {
    /// Replay once when opening retained history; no payloads or history vectors are loaded.
    pub fn rpc_traffic(&self) -> Result<RpcTraffic, JournalError> {
        replay(&self.connection)
    }
}
pub(super) fn replay(connection: &rusqlite::Connection) -> Result<RpcTraffic, JournalError> {
    let mut statement = connection.prepare("SELECT json_extract(event_json,'$.elapsed_ns'), json_extract(event_json,'$.kind'), json_extract(event_json,'$.source') FROM events WHERE json_extract(event_json,'$.schema_version')=1 ORDER BY event_seq")?;
    let mut rows = statement.query([])?;
    let mut result = RpcTraffic::default();
    while let Some(row) = rows.next()? {
        let elapsed =
            u64::try_from(row.get::<_, i64>(0)?).map_err(|_| JournalError::UnsupportedSchema)?;
        let kind: String = row.get(1)?;
        let source: String = row.get(2)?;
        result.apply([Update {
            elapsed_ns: elapsed,
            registration: source == "ronchi" && kind == "rpc.registered",
            terminal: source == "ronchi"
                && matches!(kind.as_str(), "lab.finalized" | "lab.interrupted"),
        }]);
    }
    Ok(result)
}
