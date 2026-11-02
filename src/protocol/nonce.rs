//! Nonce lifetime only. This does not implement general System Program execution.
use super::state::{LoadedAddresses, unavailable};
use crate::rpc::RpcError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use solana_message::VersionedMessage;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NonceAccount {
    pub address: String,
    pub authority: String,
    pub nonce: String,
    #[serde(default)]
    pub slot: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NonceUse {
    pub address: String,
    pub authority: String,
}
pub(super) fn resolve(
    message: &VersionedMessage,
    loaded: &LoadedAddresses,
) -> Result<Option<NonceUse>, RpcError> {
    let Some(ix) = message.instructions().first() else {
        return Ok(None);
    };
    let Some(program) = message
        .static_account_keys()
        .get(usize::from(ix.program_id_index))
    else {
        return Ok(None);
    };
    if program != &solana_sdk_ids::system_program::id() || ix.data != 4u32.to_le_bytes() {
        return Ok(None);
    }
    if ix.accounts.len() < 3 {
        return Err(RpcError::params(
            "Nonce advance requires nonce, sysvar, and authority accounts",
        ));
    }
    let keys: Vec<_> = message
        .static_account_keys()
        .iter()
        .map(ToString::to_string)
        .chain(loaded.writable.iter().cloned())
        .chain(loaded.readonly.iter().cloned())
        .collect();
    let get = |index: u8| {
        keys.get(usize::from(index))
            .cloned()
            .ok_or_else(|| unavailable("Nonce account address is unresolved"))
    };
    let account = usize::from(ix.accounts[0]);
    let header = message.header();
    let signers = usize::from(header.num_required_signatures);
    let static_len = message.static_account_keys().len();
    let writable = if account < signers {
        account < signers - usize::from(header.num_readonly_signed_accounts)
    } else if account < static_len {
        account < static_len - usize::from(header.num_readonly_unsigned_accounts)
    } else {
        account < static_len + loaded.writable.len()
    };
    if !writable
        || !message.is_signer(usize::from(ix.accounts[2]))
        || get(ix.accounts[1])? != solana_sdk_ids::sysvar::recent_blockhashes::id().to_string()
    {
        return Err(RpcError::params(
            "Nonce advance requires a writable account, recent-blockhash sysvar, and signing authority",
        ));
    }
    Ok(Some(NonceUse {
        address: get(ix.accounts[0])?,
        authority: get(ix.accounts[2])?,
    }))
}
pub(crate) fn next_value(blockhash: &str) -> Result<String, RpcError> {
    let hash = bs58::decode(blockhash)
        .into_vec()
        .map_err(|_| RpcError::internal())?;
    Ok(bs58::encode(Sha256::digest(
        [b"DURABLE_NONCE".as_slice(), &hash].concat(),
    ))
    .into_string())
}
