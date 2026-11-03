use super::{
    Binding, Error, Profile, Store,
    client::{self, Outcome},
    now_ms,
};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
pub(super) struct Active {
    pub id: String,
    pub state: String,
    pub deadline: i64,
    pub payload_digest: String,
    pub identity: Option<Identity>,
    pub ambiguous: bool,
    pub observations: usize,
}
#[derive(Clone)]
pub(super) struct Identity {
    pub signature: String,
    pub wire: String,
    pub generation: u8,
    pub validity: Value,
}
pub(super) fn build(
    key: &SigningKey,
    id: &str,
    digest: &str,
    generation: u8,
    latest: &Value,
) -> Result<Identity, Error> {
    use base64::Engine;
    let blockhash = latest["value"]["blockhash"]
        .as_str()
        .ok_or(Error::Unavailable)?;
    if blockhash.len() > 44 {
        return Err(Error::Unavailable);
    }
    let hash = bs58::decode(blockhash)
        .into_vec()
        .map_err(|_| Error::Unavailable)?;
    if hash.len() != 32 {
        return Err(Error::Unavailable);
    }
    let height = latest["value"]["lastValidBlockHeight"]
        .as_u64()
        .ok_or(Error::Unavailable)?;
    let slot = latest["context"]["slot"]
        .as_u64()
        .ok_or(Error::Unavailable)?;
    // One legacy fixture with an inert program. The generation discriminator is signed.
    let mut message = vec![1, 0, 1, 2];
    message.extend(key.verifying_key().to_bytes());
    message.extend([7u8; 32]);
    message.extend(hash);
    let discriminator =
        crate::query::digest(format!("reference-v1:{id}:{digest}:{generation}").as_bytes());
    message.extend([1, 1, 0, 64]);
    message.extend(discriminator.as_bytes());
    let signature = key.sign(&message).to_bytes();
    let signature_text = bs58::encode(signature).into_string();
    let mut wire = vec![1];
    wire.extend(signature);
    wire.extend(message);
    Ok(Identity {
        signature: signature_text,
        wire: base64::engine::general_purpose::STANDARD.encode(wire),
        generation,
        validity: json!({"blockhash":blockhash,"last_valid_block_height":height,"context_slot":slot}),
    })
}
pub(super) async fn barrier(
    directory: &std::path::Path,
    phase: &str,
    stop: &CancellationToken,
) -> Result<(), Error> {
    #[cfg(debug_assertions)]
    if std::env::var("RONCHI_REFERENCE_TEST_BARRIER")
        .ok()
        .as_deref()
        == Some(phase)
    {
        let path = directory.to_path_buf();
        let phase = phase.to_string();
        tokio::task::spawn_blocking(move || {
            crate::workspace::atomic_json(&path.join("barrier.json"), &json!({"phase":phase}))
        })
        .await
        .map_err(|_| Error::Storage)?
        .map_err(|_| Error::Storage)?;
        loop {
            if stop.is_cancelled() {
                return Ok(());
            }
            if directory.join("barrier.release").try_exists()? {
                break;
            }
            tokio::select! {_ =stop.cancelled()=>return Ok(()),_=tokio::time::sleep(Duration::from_millis(10))=>{}}
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = (directory, phase, stop);
    Ok(())
}
async fn valid_source(project: &std::path::Path, binding: &Binding) -> bool {
    match super::public_binding(&crate::workspace::Workspace::new(project), &binding.lab_id).await {
        Ok(current) => {
            current.lab_id == binding.lab_id
                && current.genesis_hash == binding.genesis_hash
                && current.rpc_address == binding.rpc_address
        }
        Err(_) => false,
    }
}
#[derive(PartialEq, Eq)]
enum StatusDecision {
    Terminal,
    Absent,
    Continuing,
    Cancelled,
}
async fn observe_status(
    store: &Store,
    active: &Active,
    binding: &Binding,
    stop: &CancellationToken,
    prepared: bool,
) -> Result<StatusDecision, Error> {
    let identity = active.identity.as_ref().ok_or(Error::Storage)?;
    let outcome = client::rpc(
        binding.rpc_address,
        "getSignatureStatuses",
        json!([[identity.signature],{"searchTransactionHistory":true}]),
        remaining(active),
        stop,
    )
    .await;
    if matches!(outcome, Outcome::Cancelled) {
        return Ok(StatusDecision::Cancelled);
    }
    let status = outcome.response().and_then(|v| v["value"].get(0)).cloned();
    let safe_status = status.filter(|value| crate::query::encoded_len(value, 512).is_ok());
    let next = if let Some(value) = safe_status.as_ref().filter(|v| v.is_object()) {
        if value.get("err").is_some_and(|v| !v.is_null()) {
            "failed"
        } else if value.get("err").is_some_and(Value::is_null)
            && value["confirmationStatus"] == "finalized"
        {
            "succeeded"
        } else {
            "reconciling"
        }
    } else if prepared && matches!(safe_status, Some(Value::Null)) {
        "prepared"
    } else {
        "reconciling"
    };
    let data = json!({"signature":identity.signature,"client_outcome":outcome_name(&outcome),"status":super::optional(safe_status.clone())});
    store.record(&active.id, next, "status", data).await?;
    if matches!(next, "succeeded" | "failed") {
        return Ok(StatusDecision::Terminal);
    }
    Ok(if prepared && matches!(safe_status, Some(Value::Null)) {
        StatusDecision::Absent
    } else {
        StatusDecision::Continuing
    })
}
fn outcome_name(outcome: &Outcome) -> Value {
    match outcome {
        Outcome::Response(_) => json!({"outcome":"response"}),
        _ => serde_json::to_value(outcome).expect("typed client outcome serializes"),
    }
}
fn remaining(active: &Active) -> Duration {
    Duration::from_millis((active.deadline - now_ms()).clamp(1, 5000) as u64)
}
pub(super) async fn run(
    store: Store,
    key: SigningKey,
    binding: Binding,
    project: PathBuf,
    directory: PathBuf,
    stop: CancellationToken,
    requested_mismatch: bool,
) -> Result<(), Error> {
    let recovered = store.recover().await?;
    let mut precheck = recovered
        .filter(|a| a.state == "prepared")
        .and_then(|a| a.identity.map(|i| (a.id, i.generation)));
    loop {
        if stop.is_cancelled() {
            return Ok(());
        }
        let Some(active) = store.active().await? else {
            tokio::select! {_ =stop.cancelled()=>return Ok(()),_=store.changed.notified()=>{}}
            continue;
        };
        if active.deadline <= now_ms() || active.observations >= 124 {
            store.record(&active.id,"unresolved","source_gap",json!({"reason":if active.deadline<=now_ms(){"original_horizon_ended"}else{"observation_limit"}})).await?;
            barrier(&directory, "terminal", &stop).await?;
            continue;
        }
        if requested_mismatch {
            store
                .record(
                    &active.id,
                    "unresolved",
                    "source_gap",
                    json!({"reason":"requested_lab_identity_mismatch"}),
                )
                .await?;
            continue;
        }
        // Revalidate immutable Lab and public local genesis before each unit of RPC work.
        if !valid_source(&project, &binding).await {
            if stop.is_cancelled() {
                return Ok(());
            }
            store
                .record(
                    &active.id,
                    "unresolved",
                    "source_gap",
                    json!({"reason":"pinned_lab_unavailable_or_identity_mismatch"}),
                )
                .await?;
            continue;
        }
        if stop.is_cancelled() {
            return Ok(());
        }
        if active.deadline <= now_ms() {
            continue;
        }
        match active.state.as_str() {
            "accepted" => {
                let outcome = client::rpc(
                    binding.rpc_address,
                    "getLatestBlockhash",
                    json!([{"commitment":"finalized"}]),
                    remaining(&active),
                    &stop,
                )
                .await;
                if matches!(outcome, Outcome::Cancelled) {
                    return Ok(());
                }
                let identity = outcome.response().and_then(|latest| {
                    build(&key, &active.id, &active.payload_digest, 0, latest).ok()
                });
                if let Some(identity) = identity {
                    barrier(&directory, "before_prepare_commit", &stop).await?;
                    if stop.is_cancelled() {
                        return Ok(());
                    }
                    store.prepare(&active.id, identity).await?;
                    barrier(&directory, "prepared", &stop).await?;
                } else {
                    store.record(&active.id,"unresolved","source_gap",json!({"reason":"validity_inputs_unavailable","client_outcome":outcome_name(&outcome)})).await?;
                }
            }
            "prepared" => {
                let identity = active.identity.as_ref().ok_or(Error::Storage)?;
                if precheck.as_ref() == Some(&(active.id.clone(), identity.generation)) {
                    precheck = None;
                    if observe_status(&store, &active, &binding, &stop, true).await?
                        != StatusDecision::Absent
                    {
                        continue;
                    }
                    let validity = client::rpc(
                        binding.rpc_address,
                        "isBlockhashValid",
                        json!([identity.validity["blockhash"],{"commitment":"finalized"}]),
                        remaining(&active),
                        &stop,
                    )
                    .await;
                    if matches!(validity, Outcome::Cancelled) {
                        return Ok(());
                    }
                    if validity.response().is_none_or(|v| v["value"] != true) {
                        store.record(&active.id,"unresolved","source_gap",json!({"reason":"prepared_identity_validity_not_confirmed","client_outcome":outcome_name(&validity)})).await?;
                        continue;
                    }
                    store
                        .record(
                            &active.id,
                            "prepared",
                            "validity",
                            json!({"signature":identity.signature,"blockhash_valid":true}),
                        )
                        .await?;
                    continue;
                }
                let attempt = store.start_attempt(&active.id, identity.generation).await?;
                barrier(&directory, "attempt_start", &stop).await?;
                if stop.is_cancelled() {
                    return Ok(());
                }
                let outcome=client::rpc(binding.rpc_address,"sendTransaction",json!([identity.wire,{"encoding":"base64","skipPreflight":true,"maxRetries":0}]),remaining(&active),&stop).await;
                if matches!(outcome, Outcome::Cancelled) {
                    return Ok(());
                }
                barrier(&directory, "rpc_result", &stop).await?;
                if stop.is_cancelled() {
                    return Ok(());
                }
                let outcome = if matches!(&outcome,Outcome::Response(value) if value.as_str()!=Some(identity.signature.as_str()))
                {
                    Outcome::InvalidResponse
                } else {
                    outcome
                };
                store
                    .complete_attempt(&active.id, attempt, &outcome)
                    .await?;
            }
            "reconciling" => {
                let identity = active.identity.as_ref().ok_or(Error::Storage)?;
                if binding.profile == Profile::Unsafe
                    && identity.generation == 0
                    && active.ambiguous
                {
                    if active.deadline <= now_ms() {
                        store
                            .record(
                                &active.id,
                                "unresolved",
                                "source_gap",
                                json!({"reason":"original_horizon_ended_before_send"}),
                            )
                            .await?;
                        continue;
                    }
                    let outcome = client::rpc(
                        binding.rpc_address,
                        "getLatestBlockhash",
                        json!([{"commitment":"finalized"}]),
                        remaining(&active),
                        &stop,
                    )
                    .await;
                    if matches!(outcome, Outcome::Cancelled) {
                        return Ok(());
                    }
                    if let Some(identity) = outcome.response().and_then(|latest| {
                        build(&key, &active.id, &active.payload_digest, 1, latest).ok()
                    }) {
                        store.prepare(&active.id, identity).await?;
                        barrier(&directory, "replacement_prepared", &stop).await?;
                        continue;
                    }
                    store.record(&active.id,"reconciling","source_gap",json!({"reason":"replacement_validity_unavailable","client_outcome":outcome_name(&outcome)})).await?;
                } else if observe_status(&store, &active, &binding, &stop, false).await?
                    == StatusDecision::Terminal
                {
                    barrier(&directory, "terminal", &stop).await?;
                    continue;
                }
                tokio::select! {_ =stop.cancelled()=>return Ok(()),_=tokio::time::sleep(Duration::from_secs(1))=>{}}
            }
            _ => return Err(Error::Storage),
        }
    }
}
