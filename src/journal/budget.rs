use super::JournalError;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

pub const DEFAULT_DISK_BYTES: u64 = 1024 * 1024 * 1024;
pub const MIN_SHUTDOWN_RESERVE: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DiskLimits {
    pub total_bytes: u64,
    pub shutdown_reserve_bytes: u64,
}
impl Default for DiskLimits {
    fn default() -> Self {
        Self {
            total_bytes: DEFAULT_DISK_BYTES,
            shutdown_reserve_bytes: MIN_SHUTDOWN_RESERVE,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct DiskUsage {
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub reserved_bytes: u64,
    pub checkpoint_growth_bytes: u64,
    pub shutdown_reserve_bytes: u64,
    pub total_bytes: u64,
}
#[derive(Clone)]
pub(super) struct Budget(Arc<Mutex<Account>>);
pub(super) struct Account {
    root: PathBuf,
    database: PathBuf,
    limits: DiskLimits,
    page_size: u64,
    pages: u64,
    next: u64,
    pending: BTreeMap<u64, (u64, u64, bool)>, // growth pages, disk liability
    failed: bool,
}
pub(super) struct Permit {
    budget: Budget,
    id: u64,
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.budget.lock().pending.remove(&self.id);
    }
}
fn io(error: std::io::Error) -> JournalError {
    JournalError::Filesystem(error)
}
fn add(a: u64, b: u64) -> Result<u64, JournalError> {
    a.checked_add(b).ok_or(JournalError::DiskBudget)
}
fn mul(a: u64, b: u64) -> Result<u64, JournalError> {
    a.checked_mul(b).ok_or(JournalError::DiskBudget)
}

impl Budget {
    pub(super) fn new(
        root: &Path,
        database: &std::ffi::OsStr,
        limits: DiskLimits,
    ) -> Result<Self, JournalError> {
        if limits.shutdown_reserve_bytes < MIN_SHUTDOWN_RESERVE
            || limits.total_bytes <= limits.shutdown_reserve_bytes
        {
            return Err(JournalError::InvalidInput);
        }
        let root = root.canonicalize().map_err(io)?;
        if !root.is_dir() {
            return Err(JournalError::InvalidInput);
        }
        let budget = Self(Arc::new(Mutex::new(Account {
            database: root.join(database),
            root,
            limits,
            page_size: 4096,
            pages: 0,
            next: 0,
            pending: BTreeMap::new(),
            failed: false,
        })));
        // Initial schema, WAL, SHM and filesystem allocation overhead.
        budget.lock().check(1024 * 1024, true)?;
        Ok(budget)
    }
    pub(super) fn lock(&self) -> MutexGuard<'_, Account> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub(super) fn initialize(&self, page_size: u64, pages: u64) -> Result<(), JournalError> {
        let mut account = self.lock();
        account.page_size = page_size;
        account.pages = pages;
        account.check(0, true)
    }
    pub(super) fn publish_metadata(&self, data: &[u8]) -> Result<(), JournalError> {
        if data.len() > 64 * 1024 {
            return Err(JournalError::InvalidInput);
        }
        let mut account = self.lock();
        let result = (|| {
            let extra = (data.len() as u64 * 2)
                + fs2::allocation_granularity(&account.root).map_err(io)? * 4;
            account.check(extra, false)?;
            crate::files::atomic_bytes(&account.root.join("manifest.json"), data).map_err(io)?;
            account.check(0, false)
        })();
        if result.is_err() {
            account.failed = true;
        }
        result
    }
    pub(super) fn failed(&self) -> bool {
        self.lock().failed
    }
    pub(super) fn usage(&self) -> Result<DiskUsage, JournalError> {
        self.lock().usage()
    }
    pub(super) fn reserve(&self, encoded: &[String]) -> Result<Permit, JournalError> {
        let mut account = self.lock();
        if account.failed {
            return Err(JournalError::WriterFailed);
        }
        let result = (|| {
            let bytes = encoded
                .iter()
                .try_fold(0u64, |n, s| add(n, s.len() as u64))?;
            // Version-3 event and protocol projections: overflow pages, leaf/interior splits, and fixed metadata.
            let growth = add(
                add(
                    bytes.div_ceil(account.page_size - 128),
                    mul(encoded.len() as u64, 10)?,
                )?,
                16,
            )?;
            let pending_growth = account
                .pending
                .values()
                .filter(|(_, _, deferred)| !*deferred)
                .try_fold(0, |n, (p, _, _)| add(n, *p))?;
            let target = add(add(account.pages, pending_growth)?, growth)?;
            let liability = account.liability(growth, target)?;
            account.next = account
                .next
                .checked_add(1)
                .ok_or(JournalError::DiskBudget)?;
            let id = account.next;
            account.pending.insert(id, (growth, liability, false));
            if let Err(error) = account.check(0, true) {
                account.pending.remove(&id);
                return Err(error);
            }
            Ok(Permit {
                budget: self.clone(),
                id,
            })
        })();
        if result.is_err() {
            account.failed = true;
        }
        result
    }
    pub(super) fn reserve_transition(&self) -> Result<Permit, JournalError> {
        let mut account = self.lock();
        if account.failed {
            return Err(JournalError::WriterFailed);
        }
        let result = (|| {
            let granularity = fs2::allocation_granularity(&account.root)
                .map_err(io)?
                .max(4096);
            // Bounded event/transaction/nonce projections at the default 1 GiB quota; no cache spilling.
            // Reserve 256 dirty frames plus 32 growth pages, SHM and allocation rounding.
            let liability = add(
                add(
                    mul(256, account.page_size + 24)?,
                    mul(32, account.page_size)?,
                )?,
                add(65536, mul(granularity, 4)?)?,
            )?;
            account.next = account
                .next
                .checked_add(1)
                .ok_or(JournalError::DiskBudget)?;
            let id = account.next;
            account.pending.insert(id, (32, liability, true));
            if let Err(error) = account.check(0, true) {
                account.pending.remove(&id);
                return Err(error);
            }
            Ok(Permit {
                budget: self.clone(),
                id,
            })
        })();
        if result.is_err() {
            account.failed = true;
        }
        result
    }
    pub(super) fn activate(&self, permit: &Permit) -> Result<(), JournalError> {
        if !Arc::ptr_eq(&self.0, &permit.budget.0) {
            return Err(JournalError::InvalidInput);
        }
        let mut account = self.lock();
        let entry = account
            .pending
            .get_mut(&permit.id)
            .ok_or(JournalError::InvalidInput)?;
        if !entry.2 {
            return Err(JournalError::InvalidInput);
        }
        entry.2 = false;
        Ok(())
    }
    /// Serializes filesystem growth with admissions. Permits remain charged through commit/checkpoint.
    pub(super) fn commit<T>(
        &self,
        permits: &[Permit],
        terminal: bool,
        operation: impl FnOnce(u64) -> Result<(T, u64), JournalError>,
    ) -> Result<T, JournalError> {
        let mut account = self.lock();
        let result = (|| {
            let growth = if terminal {
                // Terminal records consume the shutdown reserve, not normal admission capacity.
                64
            } else {
                permits.iter().try_fold(0, |n, p| {
                    if !Arc::ptr_eq(&self.0, &p.budget.0) {
                        return Err(JournalError::InvalidInput);
                    }
                    add(
                        n,
                        account
                            .pending
                            .get(&p.id)
                            .ok_or(JournalError::InvalidInput)?
                            .0,
                    )
                })?
            };
            // Recheck real files and free capacity immediately before writing.
            account.check(
                if terminal {
                    account.liability(64, add(account.pages, 64)?)?
                } else {
                    0
                },
                false,
            )?;
            let target = add(account.pages, growth)?;
            let (value, pages) = operation(target)?;
            account.pages = pages;
            for permit in permits {
                account.pending.remove(&permit.id);
            }
            account.check(0, false)?;
            Ok(value)
        })();
        if result.is_err() {
            account.failed = true;
        }
        result
    }
}
impl Account {
    fn liability(&self, growth: u64, target: u64) -> Result<u64, JournalError> {
        let granularity = fs2::allocation_granularity(&self.root)
            .map_err(io)?
            .max(4096);
        // cache_spill=OFF bounds WAL frames to one per dirty page per transaction.
        // Charge all potential frames, checkpoint growth, SHM index and allocation rounding.
        add(
            add(
                mul(growth, self.page_size)?,
                mul(target, self.page_size + 24)?,
            )?,
            add(mul(target.div_ceil(4096) + 1, 32768)?, mul(granularity, 4)?)?,
        )
    }

    fn usage(&self) -> Result<DiskUsage, JournalError> {
        let mut entries = 0;
        let used_bytes = tree_bytes(&self.root, 0, &mut entries)?;
        let stats = fs2::statvfs(&self.root).map_err(io)?;
        let reserved_bytes = self
            .pending
            .values()
            .try_fold(0, |n, (_, b, _)| add(n, *b))?;
        let pending_pages = self
            .pending
            .values()
            .try_fold(0, |n, (p, _, _)| add(n, *p))?;
        let terminal_liability = self.liability(64, add(add(self.pages, pending_pages)?, 64)?)?;
        let file_bytes = match std::fs::metadata(&self.database) {
            Ok(m) => m.len(),
            Err(e) if self.pages == 0 && e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(e) => return Err(io(e)),
        };
        let checkpoint_growth_bytes = mul(self.pages, self.page_size)?.saturating_sub(file_bytes);
        Ok(DiskUsage {
            checkpoint_growth_bytes,
            used_bytes,
            available_bytes: stats.available_space(),
            reserved_bytes,
            shutdown_reserve_bytes: self.limits.shutdown_reserve_bytes.max(terminal_liability),
            total_bytes: self.limits.total_bytes,
        })
    }
    fn check(&self, additional: u64, keep_shutdown_reserve: bool) -> Result<(), JournalError> {
        let usage = self.usage()?;
        let reserve = if keep_shutdown_reserve {
            usage.shutdown_reserve_bytes
        } else {
            0
        };
        let future = add(
            add(
                add(usage.reserved_bytes, usage.checkpoint_growth_bytes)?,
                additional,
            )?,
            reserve,
        )?;
        if add(usage.used_bytes, future)? > usage.total_bytes {
            return Err(JournalError::DiskBudget);
        }
        if future > usage.available_bytes {
            return Err(JournalError::DiskSpace);
        }
        Ok(())
    }
}
fn tree_bytes(path: &Path, depth: usize, entries: &mut usize) -> Result<u64, JournalError> {
    *entries += 1;
    if depth > 32 || *entries > 16_384 {
        return Err(JournalError::DiskBudget);
    }
    let metadata = std::fs::symlink_metadata(path).map_err(io)?;
    if metadata.file_type().is_symlink() {
        return Err(JournalError::InvalidInput);
    }
    #[cfg(unix)]
    let mut bytes = {
        use std::os::unix::fs::MetadataExt;
        metadata.len().max(metadata.blocks().saturating_mul(512))
    };
    #[cfg(not(unix))]
    let mut bytes = metadata.len();
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path).map_err(io)? {
            bytes = add(
                bytes,
                tree_bytes(&entry.map_err(io)?.path(), depth + 1, entries)?,
            )?;
        }
    } else if !metadata.is_file() {
        return Err(JournalError::InvalidInput);
    }
    Ok(bytes)
}
