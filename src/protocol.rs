//! Signed wire decoding is isolated from simulator state and RPC transport.
use crate::rpc::RpcError;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use solana_message::VersionedMessage;
use solana_transaction::versioned::{TransactionVersion, VersionedTransaction};

pub struct SignedTransaction {
    pub signature: String,
    pub wire_base64: String,
    pub digest: String,
    pub version: i16,
    pub blockhash: String,
}
fn decode_wire(bytes: &[u8]) -> Result<VersionedTransaction, RpcError> {
    let config = wincode::config::Configuration::default()
        .with_preallocation_size_limit::<16384>()
        .with_deserialization_size_limit::<4096>();
    let tx: VersionedTransaction = wincode::config::deserialize_exact(bytes, config)
        .map_err(|_| RpcError::params("Invalid signed transaction wire representation"))?;
    if tx.sanitize().is_err() {
        return Err(RpcError::params("Transaction failed structural validation"));
    }
    let max = if matches!(tx.message, VersionedMessage::V1(_)) {
        4096
    } else {
        1232
    };
    if bytes.len() > max {
        return Err(RpcError::params(
            "Transaction exceeds its wire-format packet limit",
        ));
    }
    if wincode::serialize(&tx).map_err(|_| RpcError::params("Transaction cannot be encoded"))?
        != bytes
    {
        return Err(RpcError::params("Noncanonical signed transaction encoding"));
    }
    Ok(tx)
}
pub fn decode(encoded: &str, encoding: &str) -> Result<SignedTransaction, RpcError> {
    if encoded.len() > 6000 {
        return Err(RpcError::params("Encoded transaction exceeds its bound"));
    }
    let bytes = match encoding {
        "base64" => STANDARD
            .decode(encoded)
            .map_err(|_| RpcError::params("Invalid base64 transaction"))?,
        "base58" => bs58::decode(encoded)
            .into_vec()
            .map_err(|_| RpcError::params("Invalid base58 transaction"))?,
        _ => {
            return Err(RpcError::params(
                "sendTransaction supports base58 and base64 encoding",
            ));
        }
    };
    if bytes.len() > 4096 {
        return Err(RpcError::params(
            "Transaction exceeds the largest supported packet bound",
        ));
    }
    let tx = decode_wire(&bytes)?;
    tx.verify_and_hash_message().map_err(|_| RpcError {
        code: -32003,
        message: "Transaction signature verification failed".into(),
        data: None,
    })?;
    if tx.uses_durable_nonce() {
        return Err(RpcError::unsupported(
            "Durable nonce account state is unavailable in this Lab; use configured local execution",
        ));
    }
    let signature = tx
        .signatures
        .first()
        .ok_or_else(|| RpcError::params("A signed identity is required"))?
        .to_string();
    let version = match tx.version() {
        TransactionVersion::Legacy(_) => -1,
        TransactionVersion::Number(v) => i16::from(v),
    };
    Ok(SignedTransaction {
        signature,
        wire_base64: STANDARD.encode(&bytes),
        digest: Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        version,
        blockhash: tx.message.recent_blockhash().to_string(),
    })
}
pub fn signature(value: &Value) -> Result<String, RpcError> {
    let value = value
        .as_str()
        .ok_or_else(|| RpcError::params("Expected a base58 transaction signature"))?;
    let mut bytes = [0u8; 64];
    if value.len() > 88 || bs58::decode(value).onto(&mut bytes).ok() != Some(64) {
        return Err(RpcError::params(
            "Expected a 64-byte base58 transaction signature",
        ));
    }
    Ok(value.to_string())
}
pub fn encode_lookup(wire: &str, encoding: &str) -> Result<Value, RpcError> {
    let bytes = STANDARD.decode(wire).map_err(|_| RpcError::internal())?;
    match encoding {
        "base64" => Ok(json!([wire, "base64"])),
        "base58" => Ok(json!([bs58::encode(&bytes).into_string(), "base58"])),
        "json" => {
            let tx = decode_wire(&bytes)?;
            if matches!(tx.message, VersionedMessage::V1(_)) {
                return Err(RpcError::unsupported(
                    "The v1 JSON projection is not verified by this partial profile; select base64",
                ));
            }
            let header = tx.message.header();
            let mut message = json!({"header":{"numRequiredSignatures":header.num_required_signatures,"numReadonlySignedAccounts":header.num_readonly_signed_accounts,"numReadonlyUnsignedAccounts":header.num_readonly_unsigned_accounts},"accountKeys":tx.message.static_account_keys().iter().map(ToString::to_string).collect::<Vec<_>>(),"recentBlockhash":tx.message.recent_blockhash().to_string(),"instructions":tx.message.instructions().iter().map(|ix|json!({"programIdIndex":ix.program_id_index,"accounts":ix.accounts,"data":bs58::encode(&ix.data).into_string()})).collect::<Vec<_>>()});
            if let VersionedMessage::V0(v0) = &tx.message {
                message["addressTableLookups"]=json!(v0.address_table_lookups.iter().map(|lookup|json!({"accountKey":lookup.account_key.to_string(),"writableIndexes":lookup.writable_indexes,"readonlyIndexes":lookup.readonly_indexes})).collect::<Vec<_>>());
            }
            Ok(
                json!({"signatures":tx.signatures.iter().map(ToString::to_string).collect::<Vec<_>>(),"message":message}),
            )
        }
        "jsonParsed" => Err(RpcError::unsupported(
            "Parsed instruction and lookup-state projection is unavailable in this partial profile; select json or base64",
        )),
        _ => Err(RpcError::params("Unsupported getTransaction encoding")),
    }
}
