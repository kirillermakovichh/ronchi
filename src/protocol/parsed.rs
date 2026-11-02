use super::state::{LoadedAddresses, unavailable};
use crate::rpc::RpcError;
use serde_json::{Value, json};
use solana_message::VersionedMessage;

pub(super) fn project(
    message: &VersionedMessage,
    loaded: &LoadedAddresses,
    out: &mut Value,
) -> Result<(), RpcError> {
    let static_len = message.static_account_keys().len();
    let keys: Vec<String> = message
        .static_account_keys()
        .iter()
        .map(ToString::to_string)
        .chain(loaded.writable.iter().cloned())
        .chain(loaded.readonly.iter().cloned())
        .collect();
    let reserved = reserved_keys();
    let upgradeable_loader = keys
        .iter()
        .any(|key| key == &solana_sdk_ids::bpf_loader_upgradeable::id().to_string());
    let accounts:Vec<_>=keys.iter().enumerate().map(|(i,key)|{
        let declared=if i<static_len {message.is_maybe_writable_with_reserved_addresses(i,Some(reserved))} else {i<static_len+loaded.writable.len()};
        let invoked=message.instructions().iter().any(|ix|usize::from(ix.program_id_index)==i);
        let reserved=reserved.iter().any(|address|address.to_string()==*key);
        json!({"pubkey":key,"signer":message.is_signer(i),"writable":declared&&(!invoked||upgradeable_loader)&&!reserved,"source":if i<static_len{"transaction"}else{"lookupTable"}})
    }).collect();
    let instructions=message.instructions().iter().map(|ix|{
        let program=keys.get(usize::from(ix.program_id_index)).ok_or_else(||unavailable("Instruction program address is unresolved"))?;
        let addresses=ix.accounts.iter().map(|i|keys.get(usize::from(*i)).cloned().ok_or_else(||unavailable("Instruction account address is unresolved"))).collect::<Result<Vec<_>,_>>()?;
        let parsed=if program=="11111111111111111111111111111111" {
            if ix.data.len()==12 && ix.data[..4]==2u32.to_le_bytes() && addresses.len()>=2 {
                Some(("system",json!({"type":"transfer","info":{"source":addresses[0],"destination":addresses[1],"lamports":u64::from_le_bytes(ix.data[4..12].try_into().expect("length checked"))}})))
            } else if ix.data==4u32.to_le_bytes() && addresses.len()>=3 {
                Some(("system",json!({"type":"advanceNonce","info":{"nonceAccount":addresses[0],"recentBlockhashesSysvar":addresses[1],"nonceAuthority":addresses[2]}})))
            } else {None}
        } else if ["MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr","Memo1UhkJRfHyvLMcVucJwxXeuD728EqVDDwQDxFMNo"].contains(&program.as_str()) {
            std::str::from_utf8(&ix.data).ok().map(|text|("spl-memo",json!(text)))
        } else {None};
        Ok(match parsed {
            Some((name,data))=>json!({"program":name,"programId":program,"parsed":data,"stackHeight":null}),
            None=>json!({"programId":program,"accounts":addresses,"data":bs58::encode(&ix.data).into_string(),"stackHeight":null})
        })
    }).collect::<Result<Vec<_>,RpcError>>()?;
    let object = out.as_object_mut().expect("message object");
    object.remove("header");
    out["accountKeys"] = json!(accounts);
    out["instructions"] = json!(instructions);
    Ok(())
}

fn reserved_keys() -> &'static std::collections::HashSet<solana_message::Address> {
    use solana_sdk_ids::*;
    static KEYS: std::sync::LazyLock<std::collections::HashSet<solana_message::Address>> =
        std::sync::LazyLock::new(|| {
            // All-active local profile, matching pinned Agave reserved-account-keys.
            #[allow(deprecated)]
            [
                address_lookup_table::id(),
                bpf_loader::id(),
                bpf_loader_deprecated::id(),
                bpf_loader_upgradeable::id(),
                compute_budget::id(),
                config::id(),
                ed25519_program::id(),
                feature::id(),
                loader_v4::id(),
                secp256k1_program::id(),
                secp256r1_program::id(),
                stake::config::id(),
                stake::id(),
                system_program::id(),
                vote::id(),
                zk_elgamal_proof_program::id(),
                zk_token_proof_program::id(),
                sysvar::clock::id(),
                sysvar::epoch_rewards::id(),
                sysvar::epoch_schedule::id(),
                sysvar::fees::id(),
                sysvar::instructions::id(),
                sysvar::last_restart_slot::id(),
                sysvar::recent_blockhashes::id(),
                sysvar::rent::id(),
                sysvar::rewards::id(),
                sysvar::slot_hashes::id(),
                sysvar::slot_history::id(),
                sysvar::stake_history::id(),
                native_loader::id(),
                sysvar::id(),
            ]
            .into_iter()
            .collect()
        });
    &KEYS
}
