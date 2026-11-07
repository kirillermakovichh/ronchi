//! Custom selection shares the actor's registration order and retained identity authority.
use super::*;
use crate::cases::sequence::{Runtime, Selection};
impl Owner {
    pub(super) async fn custom_register(
        &mut self,
        items: &[Value],
        real: u64,
        sim: u64,
        hold: &std::sync::Arc<crate::cases::Hold>,
    ) -> Vec<Result<Vec<Selection>, RpcError>> {
        let Some(activation) = self.activation.clone() else {
            return vec![];
        };
        let Some(runtime) = &activation.custom else {
            return vec![];
        };
        let mut decisions = Vec::with_capacity(items.len());
        for item in items {
            decisions.push(self.custom_select(runtime, item, real, sim, hold).await);
        }
        decisions
    }
    async fn custom_select(
        &mut self,
        runtime: &Runtime,
        item: &Value,
        real: u64,
        sim: u64,
        hold: &std::sync::Arc<crate::cases::Hold>,
    ) -> Result<Vec<Selection>, RpcError> {
        let valid = item.is_object()
            && item["jsonrpc"] == "2.0"
            && item["method"].is_string()
            && item
                .get("id")
                .is_none_or(|v| v.is_null() || v.is_number() || v.is_string())
            && item
                .get("params")
                .is_none_or(|v| v.is_array() || v.is_object());
        if !valid {
            return Ok(vec![]);
        }
        let method = item["method"].as_str().unwrap_or("");
        let artifact = &runtime.artifact;
        if artifact["filter"]["methods"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && !a.iter().any(|m| m == method))
        {
            return Ok(vec![]);
        }
        let params = item.get("params").unwrap_or(&Value::Null);
        let mut signatures = vec![];
        if self.backend {
            let prepared = crate::backend::prepare(method, params).await?;
            crate::cases::timeout::validation::validate(method, params).await?;
            if let Some(signed) = prepared.signed {
                signatures.push(signed.signature);
            }
        } else if TRANSACTION_METHODS.contains(&method) {
            let operation = Transactions::prepare(
                &self.clock,
                method,
                params,
                Value::Null,
                Default::default(),
                None,
            )
            .await?;
            match operation {
                Operation::Submit(signed, _, _, _) => {
                    signatures.push(signed.signature.clone());
                    Box::pin(
                        self.query(Operation::ValidateSubmit(signed), Duration::from_nanos(sim)),
                    )
                    .await?;
                }
                Operation::Fee(message, cfg) => {
                    Box::pin(self.query(
                        Operation::ValidateFee(message, cfg),
                        Duration::from_nanos(sim),
                    ))
                    .await?;
                }
                Operation::Statuses(values, _) => signatures = values,
                Operation::Lookup(signature, cfg) => {
                    signatures.push(signature.clone());
                    Box::pin(
                        self.query(Operation::Lookup(signature, cfg), Duration::from_nanos(sim)),
                    )
                    .await?;
                }
                _ => return Err(RpcError::internal()),
            }
        } else if crate::upstream::READ_METHODS.contains(&method) {
            if !self.upstream_enabled {
                return Err(RpcError::unsupported(
                    "This read requires an explicitly configured Read Upstream",
                ));
            }
            crate::cases::timeout::validation::validate(method, params).await?;
        } else {
            crate::rpc::methods::validate(&self.clock, method, params)?;
        }
        if signatures.is_empty() {
            if method == "getSignatureStatuses" {
                signatures = params[0]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
            }
            if method == "getTransaction" {
                signatures = params[0].as_str().into_iter().map(str::to_owned).collect();
            }
        }
        if method != "getSignatureStatuses"
            && let Some(commitments) = artifact["filter"]
                .get("commitments")
                .and_then(Value::as_array)
                .filter(|a| !a.is_empty())
        {
            let commitment = params
                .get(1)
                .and_then(|v| v.get("commitment"))
                .and_then(Value::as_str)
                .or_else(|| {
                    params
                        .get(0)
                        .and_then(|v| v.get("commitment"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("finalized");
            if !commitments.iter().any(|v| v == commitment) {
                return Ok(vec![]);
            }
        }
        if let Some(filter) = artifact["filter"]
            .get("signatures")
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty())
        {
            signatures.retain(|s| filter.iter().any(|v| v == s));
            if signatures.is_empty() {
                return Ok(vec![]);
            }
        }
        let signature_method = matches!(
            method,
            "sendTransaction" | "getSignatureStatuses" | "getTransaction"
        );
        let mut seen = std::collections::BTreeSet::new();
        signatures.retain(|s| seen.insert(s.clone()));
        let mut eligible = vec![];
        for signature in signatures {
            let key = signature.clone();
            let backend = self.backend;
            let admission = self
                .read(move |reader| {
                    if backend {
                        reader
                            .backend_transaction(&key)
                            .map(|v| v.map(|v| v["admission_request"].clone()))
                    } else {
                        reader
                            .transaction(&key)
                            .map(|v| v.map(|v| v.admission_request))
                    }
                })
                .await
                .map_err(|_| RpcError::internal())?;
            if method != "sendTransaction" && admission.is_none() {
                continue;
            }
            if artifact["filter"]["transaction_origin"] == "accepted_under_this_activation"
                && let Some(admission) = admission
                && admission["binding"]["policy"]["revision"].as_u64()
                    != self.binding.policy.as_ref().map(|p| p.revision)
            {
                continue;
            }
            eligible.push(signature);
        }
        if signature_method && eligible.is_empty() {
            return Ok(vec![]);
        }
        runtime.select_many(
            &eligible,
            real,
            sim,
            method,
            item.get("id").is_some(),
            |selections| {
                if selections
                    .iter()
                    .any(|s| s.primitive == "rpc.timeout-before-dispatch@1")
                {
                    hold.reserve(
                        &self.holds,
                        16,
                        self.binding.registration_seq,
                        self.binding
                            .policy
                            .as_ref()
                            .expect("custom policy")
                            .revision,
                    )?;
                }
                Ok(())
            },
        )
    }
    pub(super) async fn custom_lookup(
        &mut self,
        read: super::status::Read,
        selections: &[Selection],
    ) -> Result<Value, RpcError> {
        let Some(runtime) = read.activation.custom.as_ref() else {
            return Ok(read.source);
        };
        let signatures: Vec<String> = if read.method == "getSignatureStatuses" {
            read.params[0]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        } else {
            read.params[0]
                .as_str()
                .into_iter()
                .map(str::to_owned)
                .collect()
        };
        if read.method == "getSignatureStatuses"
            && read.source["value"]
                .as_array()
                .is_none_or(|values| values.len() != signatures.len())
        {
            return Ok(read.source);
        }
        let authority = if self.backend {
            "surfpool"
        } else {
            "rpc_simulator"
        };
        let source_id = read
            .backend
            .as_ref()
            .map(|b| b.authority().to_string())
            .unwrap_or_else(|| format!("simulator:{}", self.clock.genesis_hash()));
        let eligible: Vec<_> = signatures
            .iter()
            .enumerate()
            .filter_map(|(index, signature)| {
                selections
                    .iter()
                    .find(|s| s.signature.as_ref() == Some(signature))
                    .map(|selection| (index, signature, selection))
            })
            .collect();
        let mut returned = read.source.clone();
        for entries in eligible.chunks(8) {
            let sources = entries.iter().map(|(index, signature, _)| {
                let source = if read.method == "getSignatureStatuses" { &read.source["value"][*index] } else { &read.source };
                let source_view = if read.method == "getSignatureStatuses" && source.to_string().len() <= 16 * 1024 {
                    source.clone()
                } else {
                    json!({"available":!source.is_null(),"slot":source.get("slot")})
                };
                ("rpc.custom_lookup_source",json!({"signature":signature,"request":read.reference,"method":read.method,"source_available":!source.is_null(),"source_view":source_view,"source_digest":crate::query::digest(&serde_json::to_vec(source).expect("JSON serializes")),"source_identity":source_id,"entry_index":index}))
            }).collect();
            let source_end = self
                .status_append_page(authority, sources, read.backend.as_ref())
                .await?;
            let mut effects = Vec::with_capacity(entries.len());
            for (offset, (index, signature, selection)) in entries.iter().enumerate() {
                let source = if read.method == "getSignatureStatuses" {
                    &read.source["value"][*index]
                } else {
                    &read.source
                };
                // The writer receipt is this unit's final sequence, even in a shared commit.
                let source_seq = source_end - entries.len() as u64 + offset as u64 + 1;
                let mut no_change_reason = None;
                let replacement = if source.is_null() || read.reference["responds"] == false {
                    source.clone()
                } else {
                    match selection.primitive.as_str() {
                        "rpc.lookup-null@1" => Value::Null,
                        "rpc.lookup-snapshot@1" => {
                            let query = format!(
                                "{}:{}",
                                read.method,
                                read.params.get(1).unwrap_or(&Value::Null)
                            );
                            let mut probe_error = None;
                            let mut probe_outcome = "result";
                            let actual = if read.method == "getSignatureStatuses" {
                                source.clone()
                            } else if let Some(backend) = &read.backend {
                                match backend
                                    .call("getSignatureStatuses", &json!([[signature]]))
                                    .await
                                {
                                    Ok(value)
                                        if value["value"]
                                            .as_array()
                                            .is_some_and(|values| values.len() == 1) =>
                                    {
                                        value["value"][0].clone()
                                    }
                                    Ok(_) => {
                                        probe_outcome = "unsupported_source_shape";
                                        Value::Null
                                    }
                                    Err(error) => {
                                        probe_outcome = "source_failure";
                                        probe_error = Some(error);
                                        Value::Null
                                    }
                                }
                            } else {
                                match Box::pin(self.query(
                                    Operation::Statuses(vec![(*signature).clone()], true),
                                    self.clock.elapsed(),
                                ))
                                .await?
                                {
                                    Reply::Value(value) => value["value"][0].clone(),
                                    Reply::Registered(_) => return Err(RpcError::internal()),
                                }
                            };
                            if read.method != "getSignatureStatuses" {
                                self.status_append("rpc.custom_lifecycle_source",authority,json!({"request":read.reference,"signature":signature,"source_identity":source_id,"status_view":actual,"source_outcome":probe_outcome,"error_code":probe_error.as_ref().map(|error|error.code),"error_classification":probe_error.as_ref().and_then(|error|error.data.as_ref()).and_then(|data|data.get("code"))}),read.backend.as_ref()).await?;
                            }
                            if probe_outcome != "result" {
                                no_change_reason = Some("lifecycle_probe_unavailable");
                            }
                            match runtime.lifecycle(selection, &actual)? {
                                None => source.clone(),
                                Some(lifecycle) => {
                                    if read.method == "getSignatureStatuses" {
                                        lifecycle
                                    } else {
                                        let commitment = read
                                            .params
                                            .get(1)
                                            .and_then(|p| p.get("commitment"))
                                            .and_then(Value::as_str)
                                            .unwrap_or("finalized");
                                        let compatible = lifecycle["confirmationStatus"]
                                            == "finalized"
                                            || (lifecycle["confirmationStatus"] == "confirmed"
                                                && commitment == "confirmed");
                                        if compatible {
                                            runtime.snapshot(selection, &query, source)?
                                        } else {
                                            Value::Null
                                        }
                                    }
                                }
                            }
                        }
                        _ => source.clone(),
                    }
                };
                let changed = replacement != *source;
                if read.method == "getSignatureStatuses" {
                    returned["value"][*index] = replacement.clone();
                } else {
                    returned = replacement.clone();
                }
                effects.push((if changed{"fault.custom_lookup_changed"}else{"custom.lookup_observed"},json!({"signature":signature,"request":read.reference,"method":read.method,"selection":selection,"source_event_seq":source_seq,"source_available":!source.is_null(),"source_identity":source_id,"entry_index":index,"returned_available":!replacement.is_null(),"returned_digest":crate::query::digest(&serde_json::to_vec(&replacement).expect("JSON serializes")),"reason":if source.is_null(){"source_unavailable"}else if changed{"lookup_changed"}else{no_change_reason.unwrap_or("observed_no_change")}})));
            }
            self.status_append_page("ronchi", effects, read.backend.as_ref())
                .await?;
        }
        Ok(returned)
    }
}
