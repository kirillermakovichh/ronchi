//! Findings require actual source, selected replacement, prepared digest, and transport evidence.
use super::*;
use crate::findings::{FindingScope, Support};
impl JournalReader {
    #[allow(
        clippy::too_many_arguments,
        reason = "Shares the existing pinned query and bounded reader budget"
    )]
    pub(super) fn status_findings(
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
            rule: "lookup_view_changed".into(),
            legacy: None,
            version: 1,
            matched: 0,
            no_match: 0,
            insufficient_evidence: 0,
            unsupported: 0,
            operational_limit: false,
            state: State::NoMatch,
        };
        let mut statement=self.connection.prepare(&format!("{selected} SELECT event_seq,source,metadata_json FROM selected WHERE kind='fault.lookup_changed' ORDER BY event_seq LIMIT {}",MAX_INCIDENTS+1)).map_err(|_|Error::storage())?;
        let rows: Vec<(u64, String, String)> = statement
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
                .push("Status analysis reached the 256-incident limit".into());
        }
        for (effect_seq, source, text) in rows.into_iter().take(MAX_INCIDENTS) {
            query::check(cancel, deadline)?;
            let effect: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
            let data = &effect["data"];
            let mode = effect["execution_mode"].as_str().unwrap_or("");
            let expected = if mode == "simulator" {
                "rpc_simulator"
            } else if mode == "surfpool" {
                "surfpool"
            } else {
                rule.unsupported += 1;
                continue;
            };
            if source != "ronchi" || data["payload_status"] == "redacted" {
                rule.unsupported += 1;
                continue;
            }
            let signature = effect["transaction_signature"].as_str().unwrap_or("");
            let request = effect["request_id"].as_str().unwrap_or("");
            let admission = data["admission_request_id"].as_str().unwrap_or("");
            let source_seq = data["source_event_seq"].as_u64().unwrap_or(0);
            let index = data["entry_index"].as_u64().unwrap_or(u64::MAX) as usize;
            let n = values.len();
            let mut bound = values.to_vec();
            bound.extend([
                admission.to_string().into(),
                request.to_string().into(),
                (source_seq as i64).into(),
                (data["snapshot_event_seq"].as_u64().unwrap_or(0) as i64).into(),
            ]);
            let sql = format!(
                "{selected} SELECT event_seq,kind,source,metadata_json FROM selected WHERE (kind='transaction.accepted' AND request_id=?{}) OR (kind IN ('rpc.response_prepared','rpc.boundary') AND request_id=?{}) OR event_seq IN (?{},?{}) ORDER BY event_seq LIMIT 8",
                n + 1,
                n + 2,
                n + 3,
                n + 4
            );
            let mut statement = self
                .connection
                .prepare(&sql)
                .map_err(|_| Error::storage())?;
            let facts: Vec<RowData> = statement
                .query_map(rusqlite::params_from_iter(bound), row_data)
                .map_err(|_| Error::storage())?
                .collect::<rusqlite::Result<_>>()
                .map_err(|_| Error::storage())?;
            let mut accepted = None;
            let mut observed = None;
            let mut prepared = None;
            let mut boundary = None;
            let mut captured = None;
            let returned_digest = crate::query::digest(
                &serde_json::to_vec(&data["returned_view"]).map_err(|_| Error::storage())?,
            );
            for (seq, kind, source, text) in facts {
                let fact: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
                if fact["execution_mode"] != mode || fact["payload_status"] == "redacted" {
                    continue;
                }
                match kind.as_str() {
                    "transaction.accepted"
                        if source == expected
                            && fact["transaction_signature"] == signature
                            && fact["policy_ref"]["revision"]
                                == effect["policy_ref"]["revision"] =>
                    {
                        accepted = Some(seq)
                    }
                    "rpc.lookup_source"
                        if seq == source_seq
                            && source == expected
                            && fact["transaction_signature"] == signature
                            && fact["request_id"] == request
                            && fact["data"]["source_available"] == true
                            && fact["data"]["source_identity"] == data["source_identity"]
                            && fact["data"]["entry_index"] == data["entry_index"] =>
                    {
                        observed = Some((seq, fact.clone()))
                    }
                    "rpc.response_prepared"
                        if source == "ronchi"
                            && fact["data"]["prepared_error_code"].is_null()
                            && fact["data"]["lookup_boundary_digests"][index]
                                == returned_digest =>
                    {
                        prepared = Some(seq)
                    }
                    "rpc.boundary"
                        if source == "ronchi"
                            && fact["data"]["rpc_response"] == "result"
                            && fact["data"]["transport"] == "http_write_completed" =>
                    {
                        boundary = Some(seq)
                    }
                    _ => {}
                }
                if seq == data["snapshot_event_seq"].as_u64().unwrap_or(0)
                    && source == expected
                    && fact["transaction_signature"] == signature
                {
                    let view = if kind == "transaction.transition" {
                        &fact["data"]["status_view"]
                    } else if kind == "rpc.lookup_source" {
                        &fact["data"]["source_view"]
                    } else {
                        &Value::Null
                    };
                    if view == &data["snapshot_view"]
                        && !view.is_null()
                        && fact["data"]["source_identity"] == data["source_identity"]
                    {
                        captured = Some(seq);
                    }
                }
            }
            let (Some(accepted), Some((observed, source_fact)), Some(prepared), Some(boundary)) =
                (accepted, observed, prepared, boundary)
            else {
                rule.insufficient_evidence += 1;
                report.gaps.push("Lookup Finding requires acceptance, authoritative source, selected replacement, prepared lookup digest, and completed boundary inside the selected scope".into());
                continue;
            };
            let stale =
                effect["policy_ref"]["case_id"] == "stale-transaction-read-after-submission";
            if signature.is_empty()
                || request.is_empty()
                || !(accepted < observed
                    && observed < effect_seq
                    && effect_seq < prepared
                    && prepared < boundary
                    && boundary <= snapshot)
                || data["source_available"] != true
                || (stale && captured.is_none_or(|seq| seq >= observed))
                || (!stale && data["returned_view"] != Value::Null)
            {
                rule.insufficient_evidence += 1;
                continue;
            }
            if data["method"] == "getSignatureStatuses"
                && source_fact["data"]["source_view"] == data["returned_view"]
            {
                rule.no_match += 1;
                continue;
            }
            let mut support = vec![
                ("acceptance", accepted),
                ("authoritative_source", observed),
                ("selected_lookup_change", effect_seq),
                ("prepared_lookup", prepared),
                ("rpc_boundary", boundary),
            ];
            if let Some(seq) = captured {
                support.push(("captured_source_view", seq));
            }
            rule.matched += 1;
            report.findings.push(Finding{finding_id:format!("f1-{}",crate::query::digest(&serde_json::to_vec(&(lab,signature,request,effect_seq,boundary,crate::cases::status::finding_definition())).expect("typed identity serializes"))),rule_id:"rpc.lookup-view.changed".into(),version:1,lab_id:lab.into(),scope:FindingScope::AcceptedTransaction{signature:signature.into(),accepted_request_id:admission.into(),from_seq:accepted,to_seq:boundary},supporting_events:support.into_iter().map(|(role,event_seq)|Support{role:role.into(),event_ref:EventRef{lab_id:lab.into(),event_seq}}).collect(),statement:if stale{"Ronchi returned a lookup view constrained by an earlier observed lifecycle snapshot while the authoritative source continued progressing.".into()}else{"Ronchi replaced an available authoritative transaction lookup with null during the selected masking window.".into()},guidance:"Inspect later same-signature queries and the application's reconciliation evidence.".into(),limitations:vec!["A completed server write does not establish client receipt, rejection, retry policy, or business outcome.".into(),if mode=="simulator"{"Simulator lifecycle state does not establish executed instructions or economic effects.".into()}else{"Surfpool evidence describes only the owned local Execution Backend.".into()}]});
        }
        report.rules.push(rule);
        Ok(())
    }
}
