//! Syntactic checks for methods whose ordinary source may validate state remotely.
use crate::rpc::{
    RpcError,
    methods::{ContextConfig, config, positional},
};
use serde::Deserialize;
use serde_json::Value;
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Account {
    encoding: Option<String>,
    commitment: Option<crate::simulator::clock::Commitment>,
    min_context_slot: Option<u64>,
    data_slice: Option<Slice>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Slice {
    offset: u64,
    length: u64,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommitmentConfig {
    commitment: Option<crate::simulator::clock::Commitment>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Airdrop {
    commitment: Option<crate::simulator::clock::Commitment>,
    recent_blockhash: Option<String>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Simulation {
    encoding: Option<String>,
    commitment: Option<crate::simulator::clock::Commitment>,
    min_context_slot: Option<u64>,
    sig_verify: Option<bool>,
    replace_recent_blockhash: Option<bool>,
    inner_instructions: Option<bool>,
    accounts: Option<Accounts>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Accounts {
    encoding: Option<String>,
    addresses: Vec<String>,
}
fn pubkey(v: &Value) -> Result<(), RpcError> {
    let Some(s) = v.as_str() else {
        return Err(RpcError::params("Expected a base58 public key"));
    };
    let mut bytes = [0; 32];
    if s.len() > 44 || bs58::decode(s).onto(&mut bytes).ok() != Some(32) {
        return Err(RpcError::params("Expected a 32-byte base58 public key"));
    }
    Ok(())
}
fn keys(v: &Value, max: usize) -> Result<(), RpcError> {
    let a = v
        .as_array()
        .filter(|a| a.len() <= max)
        .ok_or_else(|| RpcError::params("Invalid public key array size"))?;
    for v in a {
        pubkey(v)?;
    }
    Ok(())
}
fn count(a: &[Value], min: usize, max: usize) -> Result<(), RpcError> {
    if a.len() < min || a.len() > max {
        return Err(RpcError::params("Invalid positional parameter count"));
    }
    Ok(())
}
fn account(v: Option<&Value>) -> Result<(), RpcError> {
    let cfg: Account = config(v)?;
    if cfg.encoding.as_ref().is_some_and(|e| {
        !["binary", "base58", "base64", "base64+zstd", "jsonParsed"].contains(&e.as_str())
    }) {
        return Err(RpcError::params("Unknown account encoding"));
    }
    let _ = (cfg.commitment, cfg.min_context_slot);
    if let Some(slice) = cfg.data_slice {
        let _ = (slice.offset, slice.length);
    }
    Ok(())
}
pub(crate) async fn validate(method: &str, params: &Value) -> Result<(), RpcError> {
    let a = positional(params)?;
    match method {
        "getBalance" => {
            count(a, 1, 2)?;
            pubkey(&a[0])?;
            let _: ContextConfig = config(a.get(1))?;
        }
        "getTokenAccountBalance" => {
            count(a, 1, 2)?;
            pubkey(&a[0])?;
            let cfg: CommitmentConfig = config(a.get(1))?;
            let _ = cfg.commitment;
        }
        "getAccountInfo" => {
            count(a, 1, 2)?;
            pubkey(&a[0])?;
            account(a.get(1))?;
        }
        "getMultipleAccounts" => {
            count(a, 1, 2)?;
            keys(&a[0], 100)?;
            account(a.get(1))?;
        }
        "getTokenAccountsByOwner" => {
            count(a, 2, 3)?;
            pubkey(&a[0])?;
            let filter = a[1]
                .as_object()
                .filter(|f| f.len() == 1)
                .ok_or_else(|| RpcError::params("Expected one mint or programId filter"))?;
            let (k, v) = filter.iter().next().expect("one filter");
            if k != "mint" && k != "programId" {
                return Err(RpcError::params("Unknown token owner filter"));
            }
            pubkey(v)?;
            account(a.get(2))?;
        }
        "getMinimumBalanceForRentExemption" => {
            count(a, 1, 2)?;
            if !a[0].is_u64() {
                return Err(RpcError::params("Expected a data length"));
            }
            let cfg: CommitmentConfig = config(a.get(1))?;
            let _ = cfg.commitment;
        }
        "getRecentPrioritizationFees" => {
            count(a, 0, 1)?;
            if let Some(v) = a.first() {
                keys(v, 128)?;
            }
        }
        "requestAirdrop" => {
            count(a, 2, 3)?;
            pubkey(&a[0])?;
            if !a[1].is_u64() {
                return Err(RpcError::params("Expected a lamport amount"));
            }
            let cfg: Airdrop = config(a.get(2))?;
            let _ = cfg.commitment;
            if let Some(hash) = cfg.recent_blockhash {
                crate::rpc::methods::hash(Some(&Value::String(hash)))?;
            }
        }
        "simulateTransaction" => {
            count(a, 1, 2)?;
            let encoded = a[0]
                .as_str()
                .ok_or_else(|| RpcError::params("Expected encoded transaction"))?
                .to_owned();
            let cfg: Simulation = config(a.get(1))?;
            if cfg.sig_verify == Some(true) && cfg.replace_recent_blockhash == Some(true) {
                return Err(RpcError::params(
                    "sigVerify conflicts with replaceRecentBlockhash",
                ));
            }
            let _ = (cfg.commitment, cfg.min_context_slot, cfg.inner_instructions);
            if let Some(accounts) = cfg.accounts {
                if accounts.encoding.as_ref().is_some_and(|e| {
                    !["binary", "base58", "base64", "base64+zstd", "jsonParsed"]
                        .contains(&e.as_str())
                }) {
                    return Err(RpcError::params("Unknown simulation account encoding"));
                }
                keys(&serde_json::json!(accounts.addresses), 100)?;
            }
            let encoding = cfg.encoding.unwrap_or_else(|| "base58".into());
            tokio::task::spawn_blocking(move || {
                crate::protocol::validate_simulation(
                    &encoded,
                    &encoding,
                    cfg.sig_verify.unwrap_or(false),
                )
            })
            .await
            .map_err(|_| RpcError::internal())??;
        }
        _ => {}
    }
    Ok(())
}
