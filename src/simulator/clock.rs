use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Serialize,
    schemars::JsonSchema,
    clap::ValueEnum,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "lowercase")]
pub enum Commitment {
    Processed,
    Confirmed,
    #[default]
    Finalized,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ClockProfile {
    pub slot_duration_ms: u64,
    pub initial_slot: u64,
    pub initial_block_height: u64,
    pub validity_window_blocks: u64,
}
impl Default for ClockProfile {
    fn default() -> Self {
        Self {
            slot_duration_ms: 400,
            initial_slot: 0,
            initial_block_height: 0,
            validity_window_blocks: 150,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct View {
    pub simulated_time_ns: u64,
    pub slot: u64,
    pub block_height: u64,
}
#[derive(Clone)]
pub struct Clock {
    source: Source,
    pub profile: ClockProfile,
    seed: [u8; 32],
}
#[derive(Clone)]
enum Source {
    Real(tokio::time::Instant),
    Controlled(ManualTime),
}
#[derive(Clone)]
pub struct ManualTime {
    elapsed: Arc<AtomicU64>,
    changed: Arc<Notify>,
}
impl ManualTime {
    /// Advance only the injected simulator clock. This is not a public RPC operation.
    pub fn advance(&self, by: Duration) -> Result<(), &'static str> {
        let ns = u64::try_from(by.as_nanos()).map_err(|_| "Simulator time overflow")?;
        self.elapsed
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(ns))
            .map_err(|_| "Simulator time overflow")?;
        self.changed.notify_waiters();
        Ok(())
    }
}
impl Clock {
    fn make(source: Source, profile: ClockProfile, identity: &[u8]) -> Result<Self, &'static str> {
        if !(1..=60_000).contains(&profile.slot_duration_ms)
            || profile.validity_window_blocks > 10_000
            || profile.initial_slot > u64::MAX / 2
            || profile.initial_block_height > u64::MAX / 2
        {
            return Err("Invalid local clock profile");
        }
        let seed =
            Sha256::digest([b"ronchi-local-network-v1".as_slice(), identity].concat()).into();
        Ok(Self {
            source,
            profile,
            seed,
        })
    }
    pub fn realtime(profile: ClockProfile, identity: &[u8]) -> Result<Self, &'static str> {
        Self::make(Source::Real(tokio::time::Instant::now()), profile, identity)
    }
    pub fn controlled(
        profile: ClockProfile,
        identity: &[u8],
    ) -> Result<(Self, ManualTime), &'static str> {
        let time = ManualTime {
            elapsed: Arc::new(AtomicU64::new(0)),
            changed: Arc::new(Notify::new()),
        };
        Ok((
            Self::make(Source::Controlled(time.clone()), profile, identity)?,
            time,
        ))
    }
    pub fn elapsed(&self) -> Duration {
        match &self.source {
            Source::Real(start) => start.elapsed(),
            Source::Controlled(time) => Duration::from_nanos(time.elapsed.load(Ordering::SeqCst)),
        }
    }
    pub fn view_at(&self, elapsed: Duration, commitment: Commitment) -> View {
        let lag = match commitment {
            Commitment::Processed => 0,
            Commitment::Confirmed => 2,
            Commitment::Finalized => 4,
        };
        let viewed = elapsed.saturating_sub(Duration::from_secs(lag));
        let blocks = (viewed.as_nanos()
            / Duration::from_millis(self.profile.slot_duration_ms).as_nanos())
        .min(u64::MAX as u128 / 2) as u64;
        View {
            simulated_time_ns: elapsed.as_nanos().min(u64::MAX as u128) as u64,
            slot: self.profile.initial_slot + blocks,
            block_height: self.profile.initial_block_height + blocks,
        }
    }
    pub fn view(&self, commitment: Commitment) -> View {
        self.view_at(self.elapsed(), commitment)
    }
    pub fn genesis_hash(&self) -> String {
        bs58::encode(self.seed).into_string()
    }
    pub fn blockhash(&self, height: u64) -> String {
        let mut hash = Sha256::new();
        hash.update(b"ronchi-height-hash-v1");
        hash.update(self.seed);
        hash.update(height.to_le_bytes());
        let digest = hash.finalize();
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&height.to_le_bytes());
        bytes[8..].copy_from_slice(&digest[..24]);
        bs58::encode(bytes).into_string()
    }
    /// Recognize this Lab's synthetic hash domain without scanning retained history.
    pub fn local_hash_height(&self, hash: &str) -> Option<u64> {
        let mut bytes = [0u8; 32];
        if hash.len() > 44 || bs58::decode(hash).onto(&mut bytes).ok() != Some(32) {
            return None;
        }
        let height = u64::from_le_bytes(bytes[..8].try_into().ok()?);
        (height >= self.profile.initial_block_height && self.blockhash(height) == hash)
            .then_some(height)
    }
    pub fn hash_height(&self, hash: &str, view: View) -> Option<u64> {
        self.local_hash_height(hash).filter(|h| {
            *h <= view.block_height && view.block_height - *h <= self.profile.validity_window_blocks
        })
    }
    pub async fn wait_until(&self, elapsed: Duration) {
        match &self.source {
            Source::Real(start) => tokio::time::sleep_until(*start + elapsed).await,
            Source::Controlled(time) => loop {
                let notified = time.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.elapsed() >= elapsed {
                    return;
                }
                notified.await;
            },
        }
    }
}
