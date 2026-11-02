//! Local quote model. No fee is debited and no instruction is executed.
use super::state::{LoadedAddresses, ProtocolState};
use crate::rpc::RpcError;
use base64::{Engine, engine::general_purpose::STANDARD};
use solana_message::VersionedMessage;

pub struct FeeMessage(VersionedMessage);
impl FeeMessage {
    pub fn decode(encoded: &str) -> Result<Self, RpcError> {
        if encoded.len() > 5500 {
            return Err(RpcError::params("Message exceeds its encoded bound"));
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| RpcError::params("Expected a base64 message"))?;
        let cfg = wincode::config::Configuration::default()
            .with_preallocation_size_limit::<16384>()
            .with_deserialization_size_limit::<4096>();
        let message: VersionedMessage = wincode::config::deserialize_exact(&bytes, cfg)
            .map_err(|_| RpcError::params("Invalid message wire representation"))?;
        message
            .sanitize()
            .map_err(|_| RpcError::params("Invalid message structure"))?;
        let signatures = usize::from(message.header().num_required_signatures);
        let max = if matches!(message, VersionedMessage::V1(_)) {
            4096
        } else {
            1232
        };
        let prefix = usize::from(!matches!(message, VersionedMessage::V1(_)));
        if bytes.len() + signatures * 64 + prefix > max
            || wincode::serialize(&message).map_err(|_| RpcError::internal())? != bytes
        {
            return Err(RpcError::params("Noncanonical or oversized message"));
        }
        Ok(Self(message))
    }
    pub fn nonce_use(
        &self,
        state: &ProtocolState,
        slot: u64,
    ) -> Result<Option<super::nonce::NonceUse>, RpcError> {
        super::nonce::resolve(&self.0, &state.resolve(&self.0, slot)?)
    }
    pub fn blockhash(&self) -> String {
        self.0.recent_blockhash().to_string()
    }
    pub fn quote(&self, state: &ProtocolState, slot: u64) -> Result<u64, RpcError> {
        let loaded = state.resolve(&self.0, slot)?;
        quote(&self.0, &loaded)
    }
}
fn quote(message: &VersionedMessage, loaded: &LoadedAddresses) -> Result<u64, RpcError> {
    let keys: Vec<_> = message
        .static_account_keys()
        .iter()
        .map(ToString::to_string)
        .chain(loaded.writable.iter().cloned())
        .chain(loaded.readonly.iter().cloned())
        .collect();
    let mut signatures = u64::from(message.header().num_required_signatures);
    let mut limit = None;
    let mut price = None;
    let mut seen = [false; 5];
    let mut default_units = 0u64;
    for ix in message.instructions() {
        let program = keys
            .get(usize::from(ix.program_id_index))
            .ok_or_else(|| super::state::unavailable("Unresolved fee program address"))?;
        if [
            solana_sdk_ids::ed25519_program::id().to_string(),
            solana_sdk_ids::secp256k1_program::id().to_string(),
            solana_sdk_ids::secp256r1_program::id().to_string(),
        ]
        .contains(program)
        {
            signatures =
                signatures.saturating_add(u64::from(ix.data.first().copied().unwrap_or(0)));
        }
        use solana_sdk_ids::*;
        let builtin = [
            system_program::id(),
            compute_budget::id(),
            bpf_loader_upgradeable::id(),
            bpf_loader_deprecated::id(),
            bpf_loader::id(),
            loader_v4::id(),
            secp256k1_program::id(),
            ed25519_program::id(),
        ]
        .iter()
        .any(|id| id.to_string() == *program);
        default_units = default_units.saturating_add(if builtin { 3_000 } else { 200_000 });
        if program != &compute_budget::id().to_string() {
            continue;
        }
        // V1 carries its own budget in the header; legacy budget instructions have no effect.
        if matches!(message, VersionedMessage::V1(_)) {
            continue;
        }
        let tag = ix.data.first().copied().unwrap_or(0) as usize;
        if !(1..=4).contains(&tag) || seen[tag] {
            return Err(RpcError::params(
                "Invalid or duplicate compute-budget instruction",
            ));
        }
        seen[tag] = true;
        match (tag, ix.data.len()) {
            (1, 5) => {
                let heap = u32::from_le_bytes(ix.data[1..].try_into().expect("length checked"));
                if !(32768..=262144).contains(&heap) || heap % 1024 != 0 {
                    return Err(RpcError::params("Invalid heap frame size"));
                }
            }
            (2, 5) => {
                limit = Some(
                    u32::from_le_bytes(ix.data[1..].try_into().expect("length checked"))
                        .min(1_400_000),
                )
            }
            (3, 9) => {
                price = Some(u64::from_le_bytes(
                    ix.data[1..].try_into().expect("length checked"),
                ))
            }
            (4, 5) => {
                if ix.data[1..] == [0; 4] {
                    return Err(RpcError::params(
                        "Loaded account data limit must be nonzero",
                    ));
                }
            }
            _ => return Err(RpcError::params("Invalid compute-budget instruction data")),
        }
    }
    let priority = if let VersionedMessage::V1(message) = message {
        message.config.priority_fee.unwrap_or(0)
    } else {
        let units = u64::from(limit.unwrap_or(default_units.min(1_400_000) as u32));
        ((u128::from(price.unwrap_or(0)) * u128::from(units)).div_ceil(1_000_000))
            .min(u128::from(u64::MAX)) as u64
    };
    Ok(signatures.saturating_mul(5000).saturating_add(priority))
}
