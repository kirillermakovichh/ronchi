use super::{
    Event, JournalError, JournalState, JournalStore, MAX_BATCH_BYTES, MAX_BATCH_EVENTS, encode,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, watch};

const QUEUE_EVENTS: usize = 4096;
const QUEUE_BYTES: usize = 8 * 1024 * 1024;
const BATCH_WAIT: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriterHealth {
    Healthy,
    Failed,
    Closed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriterStatus {
    pub health: WriterHealth,
    pub snapshot_seq: u64,
}

struct Unit {
    events: Vec<String>,
    bytes: usize,
    reply: oneshot::Sender<Result<u64, JournalError>>,
}
struct State {
    queue: VecDeque<Unit>,
    pending_events: usize,
    pending_bytes: usize,
    closing: bool,
    terminal: Option<(Vec<String>, JournalState)>,
}
#[derive(Clone, Copy)]
pub struct QueueLimits {
    pub events: usize,
    pub bytes: usize,
}
impl Default for QueueLimits {
    fn default() -> Self {
        Self {
            events: QUEUE_EVENTS,
            bytes: QUEUE_BYTES,
        }
    }
}
struct Shared {
    limits: QueueLimits,
    state: Mutex<State>,
    changed: Condvar,
    status: watch::Sender<WriterStatus>,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn fail(&self) {
        self.status.send_modify(|s| s.health = WriterHealth::Failed);
        self.changed.notify_all();
    }
}

#[derive(Clone)]
pub struct WriterHandle {
    shared: Arc<Shared>,
}
pub struct JournalWriter {
    handle: WriterHandle,
    thread: Option<JoinHandle<Result<u64, JournalError>>>,
}
/// Dropping a receipt cancels waiting only. The writer still commits admitted work.
pub struct CommitReceipt(oneshot::Receiver<Result<u64, JournalError>>);
impl CommitReceipt {
    pub async fn committed(self) -> Result<u64, JournalError> {
        self.0.await.map_err(|_| JournalError::WriterFailed)?
    }
    /// Use outside asynchronous runtime workers.
    pub fn blocking_committed(self) -> Result<u64, JournalError> {
        self.0
            .blocking_recv()
            .map_err(|_| JournalError::WriterFailed)?
    }
}

impl WriterHandle {
    pub fn status(&self) -> WriterStatus {
        *self.shared.status.borrow()
    }
    pub fn subscribe(&self) -> watch::Receiver<WriterStatus> {
        self.shared.status.subscribe()
    }

    /// Never waits for queue space. Saturation permanently fails admission.
    /// The returned receipt acknowledges SQLite commit, not queue insertion.
    pub fn try_append(&self, events: &[Event]) -> Result<CommitReceipt, JournalError> {
        let encoded = encode(events)?;
        let bytes = encoded.iter().map(String::len).sum::<usize>();
        let (reply, receipt) = oneshot::channel();
        let mut state = self.shared.lock();
        if self.status().health == WriterHealth::Failed {
            return Err(JournalError::WriterFailed);
        }
        if state.closing {
            return Err(JournalError::Closed);
        }
        if state.pending_events + encoded.len() > self.shared.limits.events
            || state.pending_bytes + bytes > self.shared.limits.bytes
        {
            self.shared.fail();
            return Err(JournalError::Saturated);
        }
        state.pending_events += encoded.len();
        state.pending_bytes += bytes;
        state.queue.push_back(Unit {
            events: encoded,
            bytes,
            reply,
        });
        self.shared.changed.notify_one();
        Ok(CommitReceipt(receipt))
    }
}

impl JournalWriter {
    /// Moves SQLite ownership to one dedicated thread. Creation is blocking.
    pub fn start(store: JournalStore) -> Result<Self, JournalError> {
        Self::with_limits(store, QueueLimits::default())
    }
    pub fn with_limits(store: JournalStore, limits: QueueLimits) -> Result<Self, JournalError> {
        if limits.events == 0
            || limits.events > QUEUE_EVENTS
            || limits.bytes == 0
            || limits.bytes > QUEUE_BYTES
        {
            return Err(JournalError::InvalidInput);
        }

        let snapshot = store.snapshot()?;
        if snapshot.state != JournalState::Open {
            return Err(JournalError::Closed);
        }
        let snapshot_seq = snapshot.snapshot_seq;
        let (status, _) = watch::channel(WriterStatus {
            health: WriterHealth::Healthy,
            snapshot_seq,
        });
        let shared = Arc::new(Shared {
            limits,
            state: Mutex::new(State {
                queue: VecDeque::new(),
                pending_events: 0,
                pending_bytes: 0,
                closing: false,
                terminal: None,
            }),
            changed: Condvar::new(),
            status,
        });
        let owner = shared.clone();
        let thread = thread::Builder::new()
            .name("ronchi-journal".into())
            .spawn(move || {
                // Any unwinding or early failure must close admission and resolve queued receipts.
                let mut guard = FailureGuard {
                    shared: owner.clone(),
                    clean: false,
                };
                let result = write_loop(store, &owner);
                guard.clean = result.is_ok();
                result
            })?;
        Ok(Self {
            handle: WriterHandle { shared },
            thread: Some(thread),
        })
    }
    pub fn handle(&self) -> WriterHandle {
        self.handle.clone()
    }

    /// Stops admission, drains accepted units, and joins the writer.
    /// The lifecycle owner must settle producers before supplying a terminal fact.
    /// This blocking join belongs outside Tokio worker threads.
    pub fn shutdown(
        mut self,
        terminal: Option<(&Event, JournalState)>,
    ) -> Result<u64, JournalError> {
        let terminal = match terminal {
            Some((_, JournalState::Open)) => return Err(JournalError::InvalidInput),
            Some((event, state)) => Some((encode(std::slice::from_ref(event))?, state)),
            None => None,
        };
        {
            let mut state = self.handle.shared.lock();
            state.closing = true;
            state.terminal = terminal;
            self.handle.shared.changed.notify_all();
        }
        self.thread
            .take()
            .ok_or(JournalError::WriterFailed)?
            .join()
            .map_err(|_| JournalError::WriterFailed)?
    }
}
impl Drop for JournalWriter {
    fn drop(&mut self) {
        // No clean terminal marker is implied by dropping an owner.
        let mut state = self.handle.shared.lock();
        state.closing = true;
        self.handle.shared.changed.notify_all();
        drop(state);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct FailureGuard {
    shared: Arc<Shared>,
    clean: bool,
}
impl Drop for FailureGuard {
    fn drop(&mut self) {
        if !self.clean {
            let mut state = self.shared.lock();
            state.closing = true;
            self.shared.fail();
            for unit in state.queue.drain(..) {
                let _ = unit.reply.send(Err(JournalError::WriterFailed));
            }
            state.pending_events = 0;
            state.pending_bytes = 0;
        }
    }
}

fn write_loop(mut store: JournalStore, shared: &Shared) -> Result<u64, JournalError> {
    loop {
        let mut state = shared.lock();
        while state.queue.is_empty()
            && !state.closing
            && shared.status.borrow().health != WriterHealth::Failed
        {
            state = shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
        if state.queue.is_empty() {
            let failed = shared.status.borrow().health == WriterHealth::Failed;
            let terminal = state.terminal.take();
            drop(state);
            if failed {
                return Err(JournalError::WriterFailed);
            }
            let seq = if let Some((events, terminal)) = terminal {
                store.commit(events, Some(terminal))?
            } else {
                store.snapshot()?.snapshot_seq
            };
            shared.status.send_modify(|s| {
                s.snapshot_seq = seq;
                s.health = WriterHealth::Closed;
            });
            return Ok(seq);
        }
        let deadline = Instant::now() + BATCH_WAIT;
        let mut units = Vec::new();
        let mut count = 0;
        let mut bytes = 0;
        loop {
            if let Some(front) = state.queue.front() {
                if count + front.events.len() > MAX_BATCH_EVENTS
                    || bytes + front.bytes > MAX_BATCH_BYTES
                {
                    break;
                }
                let unit = state.queue.pop_front().ok_or(JournalError::WriterFailed)?;
                count += unit.events.len();
                bytes += unit.bytes;
                units.push(unit);
            } else if !state.closing
                && shared.status.borrow().health != WriterHealth::Failed
                && Instant::now() < deadline
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                state = shared
                    .changed
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            } else {
                break;
            }
            if count == MAX_BATCH_EVENTS || bytes == MAX_BATCH_BYTES || Instant::now() >= deadline {
                break;
            }
        }
        drop(state);
        let mut encoded = Vec::with_capacity(count);
        let mut replies = Vec::with_capacity(units.len());
        for unit in units {
            let len = unit.events.len();
            encoded.extend(unit.events);
            replies.push((len, unit.reply));
        }
        let result = store.commit(encoded, None);
        let mut state = shared.lock();
        state.pending_events -= count;
        state.pending_bytes -= bytes;
        match result {
            Ok(seq) => {
                shared.status.send_modify(|s| s.snapshot_seq = seq);
                let mut watermark = seq - count as u64;
                for (len, reply) in replies {
                    watermark += len as u64;
                    let _ = reply.send(Ok(watermark));
                }
            }
            Err(error) => {
                shared.fail();
                for (_, reply) in replies {
                    let _ = reply.send(Err(JournalError::WriterFailed));
                }
                return Err(error);
            }
        }
    }
}
