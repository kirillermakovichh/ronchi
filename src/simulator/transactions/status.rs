//! Source-attributed lookup observations and activation-owned boundary projection.
use super::*;
use crate::cases::{Activation, status};
use std::sync::{Arc, Weak};

pub(super) struct Read {
    pub method: String,
    pub params: Value,
    pub source: Value,
    pub reference: Value,
    pub activation: Arc<Activation>,
    pub backend: Option<crate::backend::Client>,
}
impl Owner {
    pub(super) fn status_admit(
        &self,
        activation: &Option<Arc<Activation>>,
        signature: &str,
        reference: &Value,
        now: u64,
    ) -> Result<bool, RpcError> {
        let binding: crate::cases::Binding = serde_json::from_value(reference["binding"].clone())
            .map_err(|_| RpcError::internal())?;
        if !status::matches(&binding, signature) {
            return Ok(false);
        }
        if let Some(activation) = activation {
            let settings = binding
                .policy
                .as_ref()
                .and_then(|p| p.settings.status.clone())
                .unwrap_or_default();
            activation
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .admit(signature, now, reference, &settings)?;
            Ok(status::stale(&binding))
        } else {
            Ok(false)
        }
    }
    async fn status_append(
        &mut self,
        kind: &str,
        source: &str,
        data: Value,
        backend: Option<&crate::backend::Client>,
    ) -> Result<u64, RpcError> {
        let (mut data, redacted) = backend
            .map(|backend| backend.redact(&data))
            .unwrap_or((data, false));
        if redacted {
            data["payload_status"] = json!("redacted");
        }
        let mut event = crate::lab::event(kind, self.started, data);
        event.source = source.into();
        if self.backend {
            event.execution_mode = crate::config::Mode::Surfpool;
        }
        let writer = self.writer.clone();
        let receipt = tokio::task::spawn_blocking(move || writer.try_append(&[event]))
            .await
            .map_err(|_| RpcError::internal())?
            .map_err(|_| RpcError::internal())?;
        receipt.committed().await.map_err(|_| RpcError::internal())
    }
    fn status_capture(
        &self,
        activation: &Arc<Activation>,
        signature: &str,
        value: &Value,
        at: u64,
        source_seq: u64,
    ) -> Result<(), RpcError> {
        let mut cache = activation.status.lock().unwrap_or_else(|e| e.into_inner());
        if cache.capture(signature, value, at, source_seq)?.is_some() {
            cache.captured(signature, source_seq);
        }
        Ok(())
    }
    pub(super) fn status_transition(
        &mut self,
        record: &TransactionRecord,
        source_seq: u64,
        observation: &Value,
    ) -> Result<(), RpcError> {
        let Some(activation) = self
            .status_activations
            .remove(&record.signature)
            .and_then(|weak| Weak::upgrade(&weak))
        else {
            return Ok(());
        };
        if record.stage != Phase::Processed {
            return Ok(());
        }
        self.status_capture(
            &activation,
            &record.signature,
            &observation["status_view"],
            observation["observed_simulated_ns"].as_u64().unwrap_or(0),
            source_seq,
        )
    }
    pub(super) async fn status_view(&mut self, read: Read) -> Result<Value, RpcError> {
        let binding: crate::cases::Binding =
            serde_json::from_value(read.reference["binding"].clone())
                .map_err(|_| RpcError::internal())?;
        let policy = binding.policy.as_ref().ok_or_else(RpcError::internal)?;
        let params = crate::rpc::methods::positional(&read.params)?;
        let signatures = if read.method == "getSignatureStatuses" {
            params
                .first()
                .and_then(Value::as_array)
                .ok_or_else(|| RpcError::params("Expected signatures"))?
                .iter()
                .map(crate::protocol::signature)
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![crate::protocol::signature(
                params.first().unwrap_or(&Value::Null),
            )?]
        };
        let mut returned = read.source.clone();
        if read.method == "getSignatureStatuses"
            && read
                .source
                .get("value")
                .and_then(Value::as_array)
                .is_none_or(|v| v.len() != signatures.len())
        {
            return Ok(returned);
        }
        let authority = if self.backend {
            "surfpool"
        } else {
            "rpc_simulator"
        };
        let at = if self.backend {
            read.reference["registered_elapsed_ns"].as_u64()
        } else {
            read.reference["registered_simulated_ns"].as_u64()
        }
        .unwrap_or(0);
        let source_identity = read
            .backend
            .as_ref()
            .map(|backend| backend.authority().to_string())
            .unwrap_or_else(|| format!("simulator:{}", self.clock.genesis_hash()));
        let observed = if self.backend {
            self.started.elapsed().as_nanos()
        } else {
            self.clock.elapsed().as_nanos()
        }
        .min(i64::MAX as u128) as u64;
        for (index, signature) in signatures.iter().enumerate() {
            let value = if read.method == "getSignatureStatuses" {
                &read.source["value"][index]
            } else {
                &read.source
            };
            let source_view = if read.method == "getSignatureStatuses"
                && value.to_string().len() <= 16 * 1024
            {
                value.clone()
            } else {
                json!({"available":!value.is_null(),"slot":value.get("slot"),"version":value.get("version")})
            };
            let source_seq=self.status_append("rpc.lookup_source",authority,json!({"signature":signature,"request":read.reference,"method":read.method,"entry_index":index,"source_view":source_view,"source_available":!value.is_null(),"source_identity":source_identity,"observed_clock_ns":observed,"context":read.source.get("context"),"query":read.params.get(1),"authority":authority}),read.backend.as_ref()).await?;
            if !value.is_null()
                && (!value.is_object()
                    || (read.method == "getSignatureStatuses" && !value["slot"].is_u64()))
            {
                self.status_skip(
                    &read,
                    signature,
                    index,
                    source_seq,
                    "unsupported_source_shape",
                )
                .await?;
                continue;
            }
            if !status::matches(&binding, signature) {
                continue;
            }
            let entry = read
                .activation
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(signature)
                .cloned();
            let Some(entry) = entry else {
                self.status_skip(
                    &read,
                    signature,
                    index,
                    source_seq,
                    "not_accepted_under_activation",
                )
                .await?;
                continue;
            };
            if value.is_null() {
                self.status_skip(&read, signature, index, source_seq, "source_unavailable")
                    .await?;
                continue;
            }
            if status::stale(&binding) && read.method == "getSignatureStatuses" {
                self.status_capture(&read.activation, signature, value, observed, source_seq)?;
            }
            if read.reference["responds"] == false {
                self.status_skip(
                    &read,
                    signature,
                    index,
                    source_seq,
                    "notification_no_response",
                )
                .await?;
                continue;
            }
            let snapshot = read
                .activation
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(signature)
                .and_then(|entry| entry.snapshot.clone());
            let (replacement, reason) = if !status::stale(&binding) {
                if at.saturating_sub(entry.accepted_ns) < status::WINDOW_NS {
                    (Some(Value::Null), "lookup_masked")
                } else {
                    (None, "recovered")
                }
            } else if let Some(snapshot) = &snapshot {
                if snapshot.event_seq == 0 {
                    (None, "snapshot_uncommitted")
                } else if at.saturating_sub(snapshot.captured_ns) >= status::WINDOW_NS {
                    (None, "recovered")
                } else if read.method == "getSignatureStatuses" {
                    (Some(snapshot.value.clone()), "snapshot_reused")
                } else {
                    let commitment = read
                        .params
                        .get(1)
                        .and_then(Value::as_object)
                        .and_then(|cfg| cfg.get("commitment"))
                        .and_then(Value::as_str)
                        .unwrap_or("finalized");
                    let enough = match snapshot.value["confirmationStatus"].as_str() {
                        Some("finalized") => true,
                        Some("confirmed") => commitment == "confirmed",
                        _ => false,
                    };
                    if enough {
                        (None, "snapshot_compatible_transaction")
                    } else {
                        (Some(Value::Null), "snapshot_commitment_not_reached")
                    }
                }
            } else {
                (None, "snapshot_unavailable")
            };
            if let Some(replacement) = replacement.filter(|replacement| replacement != value) {
                if read.method == "getSignatureStatuses" {
                    returned["value"][index] = replacement.clone();
                } else {
                    returned = replacement.clone();
                }
                self.status_append("fault.lookup_changed","ronchi",json!({"signature":signature,"request":read.reference,"policy":policy,"method":read.method,"entry_index":index,"source_event_seq":source_seq,"source_available":true,"returned_available":!replacement.is_null(),"returned_view":replacement,"snapshot_event_seq":snapshot.as_ref().map(|s|s.event_seq),"snapshot_source_seq":snapshot.as_ref().map(|s|s.source_seq),"snapshot_captured_ns":snapshot.as_ref().map(|s|s.captured_ns),"snapshot_view":snapshot.as_ref().map(|s|s.value.clone()),"admission_request_id":entry.admission_request["request_id"],"reason":reason,"authority":authority,"source_identity":source_identity,"cause_refs":[source_seq]}),read.backend.as_ref()).await?;
            } else {
                self.status_skip(&read, signature, index, source_seq, reason)
                    .await?;
            }
        }
        Ok(returned)
    }
    async fn status_skip(
        &mut self,
        read: &Read,
        signature: &str,
        index: usize,
        source_seq: u64,
        reason: &str,
    ) -> Result<(), RpcError> {
        self.status_append("lookup.selection_skipped","ronchi",json!({"signature":signature,"request":read.reference,"method":read.method,"entry_index":index,"source_event_seq":source_seq,"reason":reason}),read.backend.as_ref()).await.map(|_|())
    }
}
