//! Explicit local fixture state. RPC never fills missing accounts from an upstream.
use crate::rpc::RpcError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use solana_message::VersionedMessage;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProtocolState {
    #[serde(default)]
    pub lookup_tables: Vec<LookupTable>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LookupTable {
    pub address: String,
    pub addresses: Vec<String>,
    #[serde(default)]
    pub last_extended_slot: u64,
    #[serde(default)]
    pub last_extended_slot_start_index: u8,
    /// Slot hashes are not invented: only active (not deactivated) fixture tables are usable.
    pub deactivation_slot: Option<u64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LoadedAddresses {
    pub writable: Vec<String>,
    pub readonly: Vec<String>,
}
pub fn address(value: &str) -> bool {
    let mut bytes = [0u8; 32];
    value.len() <= 44 && bs58::decode(value).onto(&mut bytes).ok() == Some(32)
}
pub fn unavailable(message: &str) -> RpcError {
    RpcError {
        code: -32000,
        message: message.into(),
        data: Some(json!({"code":"state_unavailable"})),
    }
}
impl ProtocolState {
    pub fn validate(&self) -> Result<(), String> {
        if self.lookup_tables.len() > 16
            || serde_json::to_vec(self)
                .map_err(|_| "Invalid protocol state")?
                .len()
                > 16 * 1024
        {
            return Err("Simulator state exceeds its 16 KiB/16-table budget".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for table in &self.lookup_tables {
            if !address(&table.address)
                || !seen.insert(&table.address)
                || table.addresses.len() > 256
                || usize::from(table.last_extended_slot_start_index) > table.addresses.len()
                || table.addresses.iter().any(|key| !address(key))
            {
                return Err("Invalid or duplicate simulator lookup table".into());
            }
        }
        Ok(())
    }
    pub fn resolve(
        &self,
        message: &VersionedMessage,
        slot: u64,
    ) -> Result<LoadedAddresses, RpcError> {
        let mut result = LoadedAddresses::default();
        if let VersionedMessage::V0(message) = message {
            for lookup in &message.address_table_lookups {
                let key = lookup.account_key.to_string();
                let table = self
                    .lookup_tables
                    .iter()
                    .find(|table| table.address == key)
                    .ok_or_else(|| {
                        unavailable("Required address lookup table is absent from simulator_state")
                    })?;
                if table.deactivation_slot.is_some() {
                    return Err(unavailable(
                        "Deactivated lookup table requires unavailable slot-hash state",
                    ));
                }
                let active = if slot > table.last_extended_slot {
                    table.addresses.len()
                } else {
                    usize::from(table.last_extended_slot_start_index)
                };
                for (indices, out) in [
                    (&lookup.writable_indexes, &mut result.writable),
                    (&lookup.readonly_indexes, &mut result.readonly),
                ] {
                    for index in indices {
                        if usize::from(*index) >= active {
                            return Err(unavailable(
                                "Lookup address index is unavailable at the selected slot",
                            ));
                        }
                        out.push(table.addresses[usize::from(*index)].clone());
                    }
                }
            }
        }
        Ok(result)
    }
}
