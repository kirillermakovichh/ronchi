//! Positive custom template over scoped, pinned source and transport facts.
use super::*;
use crate::findings::{FindingScope, Support};
impl JournalReader {
    #[allow(
        clippy::too_many_arguments,
        reason = "Uses the existing bounded pinned reader"
    )]
    pub(super) fn custom_findings(
        &self,
        selected: &str,
        values: &[rusqlite::types::Value],
        lab: &str,
        snapshot: u64,
        deadline: Instant,
        cancel: &CancellationToken,
        report: &mut Report,
    ) -> Result<(), Error> {
        let mut rule = RuleReport {
            rule: "custom_lookup_hidden_then_queried".into(),
            legacy: None,
            version: 1,
            matched: 0,
            no_match: 0,
            insufficient_evidence: 0,
            unsupported: 0,
            operational_limit: false,
            state: State::NoMatch,
        };
        let sql = format!(
            "{selected} SELECT event_seq,source,metadata_json FROM selected WHERE kind='fault.custom_lookup_changed' ORDER BY event_seq LIMIT {}",
            MAX_INCIDENTS + 1
        );
        let mut stmt = self
            .connection
            .prepare(&sql)
            .map_err(|_| Error::storage())?;
        let rows: Vec<(u64, String, String)> = stmt
            .query_map(rusqlite::params_from_iter(values), |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get(1)?, r.get(2)?))
            })
            .map_err(|_| Error::storage())?
            .collect::<rusqlite::Result<_>>()
            .map_err(|_| Error::storage())?;
        if rows.len() > MAX_INCIDENTS {
            rule.operational_limit = true;
            report.complete = false;
            report
                .gaps
                .push("Custom template analysis reached the 256-incident limit".into());
        }
        let mut emitted = std::collections::BTreeSet::new();
        for (effect_seq, source, text) in rows.into_iter().take(MAX_INCIDENTS) {
            query::check(cancel, deadline)?;
            let effect: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
            let data = &effect["data"];
            let custom = &effect["policy_ref"]["custom"];
            let signature = effect["transaction_signature"].as_str().unwrap_or("");
            let request = effect["request_id"].as_str().unwrap_or("");
            let mode = effect["execution_mode"].as_str().unwrap_or("");
            if source != "ronchi"
                || data["payload_status"] == "redacted"
                || data["source_available"] != true
                || data["returned_available"] != false
                || !matches!(
                    data["selection"]["primitive"].as_str(),
                    Some("rpc.lookup-null@1" | "rpc.lookup-snapshot@1")
                )
                || custom["origin"] != "custom"
            {
                continue;
            }
            for definition in custom["findings"].as_array().into_iter().flatten() {
                if definition["template"] != "lookup-hidden-then-queried@1"
                    || definition["facts"]["hidden"]["step"] != data["selection"]["step"]
                {
                    continue;
                }
                let key = (
                    effect["policy_ref"]["revision"].to_string(),
                    definition["id"].to_string(),
                    signature.to_owned(),
                    data["selection"]["cycle"].to_string(),
                );
                if emitted.contains(&key) {
                    continue;
                }
                let n = values.len();
                let mut bound = values.to_vec();
                bound.extend([
                    request.to_string().into(),
                    (data["source_event_seq"].as_u64().unwrap_or(0) as i64).into(),
                ]);
                let sql = format!(
                    "{selected} SELECT event_seq,kind,source,metadata_json FROM selected WHERE (request_id=?{} AND kind IN ('rpc.response_prepared','rpc.boundary')) OR event_seq=?{} ORDER BY event_seq LIMIT 3",
                    n + 1,
                    n + 2
                );
                let mut stmt = self
                    .connection
                    .prepare(&sql)
                    .map_err(|_| Error::storage())?;
                let facts: Vec<(u64, String, String, String)> = stmt
                    .query_map(rusqlite::params_from_iter(bound), |r| {
                        Ok((r.get::<_, i64>(0)? as u64, r.get(1)?, r.get(2)?, r.get(3)?))
                    })
                    .map_err(|_| Error::storage())?
                    .collect::<rusqlite::Result<_>>()
                    .map_err(|_| Error::storage())?;
                let mut source_seq = None;
                let mut prepared = None;
                let mut boundary = None;
                let index = data["entry_index"].as_u64().unwrap_or(u64::MAX) as usize;
                for (seq, kind, source, text) in facts {
                    let fact: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
                    if fact["execution_mode"] != mode
                        || fact["schema_version"] != 1
                        || fact["data"]["payload_status"] == "redacted"
                    {
                        continue;
                    }
                    if kind == "rpc.custom_lookup_source"
                        && seq == data["source_event_seq"].as_u64().unwrap_or(0)
                        && source
                            == if mode == "surfpool" {
                                "surfpool"
                            } else {
                                "rpc_simulator"
                            }
                        && fact["transaction_signature"] == signature
                        && fact["data"]["source_available"] == true
                        && fact["data"]["source_identity"] == data["source_identity"]
                    {
                        source_seq = Some(seq);
                    }
                    if source == "ronchi"
                        && kind == "rpc.response_prepared"
                        && fact["data"]["prepared_error_code"].is_null()
                        && fact["data"]["lookup_boundary_digests"][index] == data["returned_digest"]
                    {
                        prepared = Some(seq);
                    }
                    if source == "ronchi"
                        && kind == "rpc.boundary"
                        && fact["data"]["rpc_response"] == "result"
                        && fact["data"]["transport"] == "http_write_completed"
                    {
                        boundary = Some(seq);
                    }
                }
                let (Some(source_seq), Some(prepared), Some(boundary)) =
                    (source_seq, prepared, boundary)
                else {
                    rule.insufficient_evidence += 1;
                    report.gaps.push("Custom template requires authoritative source, changed boundary digest, and completed server write inside the selected scope".into());
                    continue;
                };
                if !(source_seq < effect_seq
                    && effect_seq < prepared
                    && prepared < boundary
                    && boundary <= snapshot)
                {
                    rule.insufficient_evidence += 1;
                    continue;
                }
                let mut bound = values.to_vec();
                bound.push((effect_seq as i64).into());
                let sql = format!(
                    "{selected} SELECT received.event_seq,received.source,received.metadata_json,selection.event_seq FROM selected AS selection JOIN selected AS received ON received.kind='rpc.registered' AND received.request_id=selection.request_id WHERE selection.kind='custom.selection' AND selection.source='ronchi' AND received.event_seq>?{} AND selection.rpc_method IN ('getSignatureStatuses','getTransaction') AND json_extract(selection.metadata_json,'$.transaction_signature')=?{} AND json_extract(selection.metadata_json,'$.data.selection.step')=?{} AND json_extract(selection.metadata_json,'$.data.selection.cycle')=?{} AND json_extract(selection.metadata_json,'$.policy_ref.revision')=?{} AND json_extract(received.metadata_json,'$.data.registered_elapsed_ns')>?{} ORDER BY received.event_seq LIMIT 1",
                    n + 1,
                    n + 2,
                    n + 3,
                    n + 4,
                    n + 5,
                    n + 6
                );
                bound.extend([
                    signature.to_string().into(),
                    definition["facts"]["later"]["step"]
                        .as_str()
                        .unwrap_or("")
                        .to_string()
                        .into(),
                    (data["selection"]["cycle"].as_u64().unwrap_or(0) as i64).into(),
                    (effect["policy_ref"]["revision"].as_u64().unwrap_or(0) as i64).into(),
                    (effect["elapsed_ns"].as_u64().unwrap_or(0) as i64).into(),
                ]);
                let later: Option<(u64, String, String, u64)> = self
                    .connection
                    .query_row(&sql, rusqlite::params_from_iter(bound), |r| {
                        Ok((
                            r.get::<_, i64>(0)? as u64,
                            r.get(1)?,
                            r.get(2)?,
                            r.get::<_, i64>(3)? as u64,
                        ))
                    })
                    .optional()
                    .map_err(|_| Error::storage())?;
                let Some((later, source, text, selection_seq)) = later else {
                    rule.no_match += 1;
                    continue;
                };
                let later_fact: Value =
                    serde_json::from_str(&text).map_err(|_| Error::storage())?;
                let interval = later_fact["data"]["registered_elapsed_ns"]
                    .as_u64()
                    .zip(effect["elapsed_ns"].as_u64())
                    .and_then(|(later, first)| later.checked_sub(first));
                if source != "ronchi"
                    || later_fact["execution_mode"] != mode
                    || interval.is_none_or(|ns| {
                        ns > definition["within_ms"].as_u64().unwrap_or(0) * 1_000_000
                    })
                {
                    rule.no_match += 1;
                    continue;
                }
                rule.matched += 1;
                emitted.insert(key);
                let id = crate::query::digest(
                    &serde_json::to_vec(&(
                        lab, custom, definition, signature, source_seq, effect_seq, boundary, later,
                    ))
                    .expect("JSON serializes"),
                );
                report.findings.push(Finding{finding_id:format!("f1-{id}"),rule_id:format!("{}.{}",custom["id"].as_str().unwrap_or("custom"),definition["id"].as_str().unwrap_or("rule")),version:1,lab_id:lab.into(),scope:FindingScope::RpcRequest{request_id:request.into(),rpc_method:data["method"].as_str().unwrap_or("").into(),signature:Some(signature.into()),from_seq:source_seq,to_seq:later.max(boundary)},supporting_events:[("authoritative_lookup",source_seq),("lookup_hidden",effect_seq),("prepared_lookup",prepared),("rpc_boundary",boundary),("later_lookup_received",later),("later_step_selection",selection_seq)].into_iter().map(|(role,event_seq)|Support{role:role.into(),event_ref:EventRef{lab_id:lab.into(),event_seq}}).collect(),statement:"Ronchi replaced an available authoritative lookup with null and later received another lookup for the same signature under the declared sequence.".into(),guidance:custom["guidance"].as_str().unwrap_or("").into(),limitations:vec!["Custom author provenance does not establish a maintainer-verified Coverage Claim.".into(),"Server writes and later lookups do not establish client receipt, causal retry, application correctness, or economic effects.".into()]});
            }
        }
        report.rules.push(rule);
        Ok(())
    }
}
