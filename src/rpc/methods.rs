use super::RpcError;
use crate::simulator::clock::{Clock, Commitment};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ContextConfig {
    #[serde(default)]
    pub commitment: Option<Commitment>,
    pub min_context_slot: Option<u64>,
}
pub(super) fn positional(params: &Value) -> Result<&[Value], RpcError> {
    match params {
        Value::Null => Ok(&[]),
        Value::Array(values) => Ok(values),
        _ => Err(RpcError::params(
            "Solana methods require positional parameters",
        )),
    }
}
pub(super) fn config<T: serde::de::DeserializeOwned + Default>(
    value: Option<&Value>,
) -> Result<T, RpcError> {
    match value {
        None | Some(Value::Null) => Ok(T::default()),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| RpcError::params("Invalid configuration keys, types, or values")),
    }
}
pub(super) fn check_context(
    clock: &Clock,
    cfg: &ContextConfig,
) -> Result<crate::simulator::clock::View, RpcError> {
    let view = clock.view(cfg.commitment.unwrap_or_default());
    if cfg.min_context_slot.is_some_and(|min| min > view.slot) {
        return Err(RpcError {
            code: -32016,
            message: "Minimum context slot has not been reached".into(),
            data: Some(json!({"contextSlot":view.slot})),
        });
    }
    Ok(view)
}
pub(super) fn hash(value: Option<&Value>) -> Result<&str, RpcError> {
    let text = value
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::params("Expected a base58 blockhash"))?;
    let mut bytes = [0u8; 32];
    if text.len() > 44 || bs58::decode(text).onto(&mut bytes).ok() != Some(32) {
        return Err(RpcError::params("Expected a 32-byte base58 blockhash"));
    }
    Ok(text)
}
pub(super) fn execute(clock: &Clock, method: &str, params: &Value) -> Result<Value, RpcError> {
    let args = positional(params)?;
    match method {
        "getHealth" | "getVersion" | "getGenesisHash" => {
            if !args.is_empty() {
                return Err(RpcError::params("This method accepts no parameters"));
            }
            Ok(match method {
                "getHealth" => json!("ok"),
                "getVersion" => {
                    json!({"solana-core":format!("0.0.0-ronchi.{}",env!("CARGO_PKG_VERSION")),"ronchi":{"version":env!("CARGO_PKG_VERSION"),"profile":"local-bootstrap-v1","compatibility":"partial"}})
                }
                _ => json!(clock.genesis_hash()),
            })
        }
        "getSlot" | "getBlockHeight" | "getLatestBlockhash" | "isBlockhashValid" => {
            let offset = usize::from(method == "isBlockhashValid");
            if args.len() > offset + 1 || args.len() < offset {
                return Err(RpcError::params("Invalid positional parameter count"));
            }
            let cfg: ContextConfig = config(args.get(offset))?;
            let view = check_context(clock, &cfg)?;
            Ok(match method {
                "getSlot" => json!(view.slot),
                "getBlockHeight" => json!(view.block_height),
                "getLatestBlockhash" => {
                    json!({"context":{"slot":view.slot},"value":{"blockhash":clock.blockhash(view.block_height),"lastValidBlockHeight":view.block_height+clock.profile.validity_window_blocks}})
                }
                _ => {
                    json!({"context":{"slot":view.slot},"value":clock.hash_height(hash(args.first())?,view).is_some()})
                }
            })
        }
        "sendTransaction"
        | "simulateTransaction"
        | "getTransaction"
        | "getSignatureStatuses"
        | "getFeeForMessage"
        | "getBalance"
        | "getAccountInfo"
        | "getMultipleAccounts"
        | "getTokenAccountBalance"
        | "getTokenAccountsByOwner"
        | "getMinimumBalanceForRentExemption"
        | "getRecentPrioritizationFees"
        | "requestAirdrop" => Err(RpcError::unsupported(
            "This method is not implemented by the current local profile",
        )),
        _ => Err(RpcError {
            code: -32601,
            message: "Method not found".into(),
            data: None,
        }),
    }
}
