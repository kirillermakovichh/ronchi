//! Pure method preparation. No RPC dispatch or evidence mutation occurs here.
use crate::{
    protocol::SignedTransaction,
    rpc::{
        RpcError,
        methods::{config, positional},
    },
};
use serde::Deserialize;
use serde_json::Value;
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Send {
    encoding: Option<String>,
    skip_preflight: Option<bool>,
    preflight_commitment: Option<crate::simulator::clock::Commitment>,
    max_retries: Option<u64>,
    min_context_slot: Option<u64>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Status {
    search_transaction_history: Option<bool>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Lookup {
    encoding: Option<String>,
    commitment: Option<crate::simulator::clock::Commitment>,
    max_supported_transaction_version: Option<u8>,
}
pub struct Prepared {
    pub method: String,
    pub params: Value,
    pub signed: Option<SignedTransaction>,
}
pub async fn prepare(method: &str, params: &Value) -> Result<Prepared, RpcError> {
    if !super::METHODS.contains(&method) {
        return Err(RpcError {
            code: -32601,
            message: "Method not found".into(),
            data: None,
        });
    }
    let args = positional(params)?;
    let mut signed = None;
    match method {
        "getHealth" | "getVersion" | "getGenesisHash" => {
            if !args.is_empty() {
                return Err(RpcError::params("This method accepts no parameters"));
            }
        }
        "getSlot" | "getBlockHeight" | "getLatestBlockhash" | "isBlockhashValid" => {
            let offset = usize::from(method == "isBlockhashValid");
            if args.len() < offset || args.len() > offset + 1 {
                return Err(RpcError::params("Invalid parameter count"));
            }
            let _: crate::rpc::methods::ContextConfig = config(args.get(offset))?;
            if offset == 1 {
                crate::rpc::methods::hash(args.first())?;
            }
        }
        "sendTransaction" => {
            if args.is_empty() || args.len() > 2 {
                return Err(RpcError::params(
                    "Expected signed transaction and optional configuration",
                ));
            }
            let encoded = args[0]
                .as_str()
                .ok_or_else(|| RpcError::params("Expected encoded signed transaction"))?
                .to_owned();
            let cfg: Send = config(args.get(1))?;
            let _validated = (
                cfg.skip_preflight,
                cfg.preflight_commitment,
                cfg.max_retries,
                cfg.min_context_slot,
            );
            let encoding = cfg.encoding.unwrap_or_else(|| "base58".into());
            signed = Some(
                tokio::task::spawn_blocking(move || crate::protocol::decode(&encoded, &encoding))
                    .await
                    .map_err(|_| RpcError::internal())??,
            );
        }
        "getSignatureStatuses" => {
            if args.is_empty() || args.len() > 2 {
                return Err(RpcError::params(
                    "Expected signatures and optional configuration",
                ));
            }
            let signatures = args[0]
                .as_array()
                .filter(|a| a.len() <= 256)
                .ok_or_else(|| RpcError::params("Expected at most 256 signatures"))?;
            for signature in signatures {
                crate::protocol::signature(signature)?;
            }
            let cfg: Status = config(args.get(1))?;
            let _validated = cfg.search_transaction_history;
        }
        "getTransaction" => {
            if args.is_empty() || args.len() > 2 {
                return Err(RpcError::params(
                    "Expected signature and optional configuration",
                ));
            }
            crate::protocol::signature(&args[0])?;
            let cfg: Lookup = if args.get(1).is_some_and(Value::is_string) {
                Lookup {
                    encoding: args[1].as_str().map(String::from),
                    ..Default::default()
                }
            } else {
                config(args.get(1))?
            };
            if cfg.commitment == Some(crate::simulator::clock::Commitment::Processed)
                || cfg.encoding.as_ref().is_some_and(|s| {
                    !["json", "jsonParsed", "base58", "base64"].contains(&s.as_str())
                })
            {
                return Err(RpcError::params(
                    "Unsupported transaction lookup configuration",
                ));
            }
            let _validated = cfg.max_supported_transaction_version;
        }
        "getFeeForMessage" => {
            if args.is_empty() || args.len() > 2 {
                return Err(RpcError::params(
                    "Expected message and optional context configuration",
                ));
            }
            let encoded = args[0]
                .as_str()
                .ok_or_else(|| RpcError::params("Expected base64 message"))?
                .to_owned();
            let _: crate::rpc::methods::ContextConfig = config(args.get(1))?;
            tokio::task::spawn_blocking(move || {
                crate::protocol::fees::FeeMessage::decode(&encoded)
            })
            .await
            .map_err(|_| RpcError::internal())??;
        }
        // The local backend owns method-specific state validation for reads, simulation and explicit local airdrops.
        _ => {}
    }
    Ok(Prepared {
        method: method.into(),
        params: if params.is_null() {
            serde_json::json!([])
        } else {
            params.clone()
        },
        signed,
    })
}
