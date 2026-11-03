//! Only source-attributed non-dispatch, shared hold, boundary and later registration support this rule.
use super::*;
use crate::findings::{FindingScope, Support};
impl JournalReader {
    #[allow(
        clippy::too_many_arguments,
        reason = "Uses the existing pinned bounded Investigation selection"
    )]
    pub(super) fn timeout_findings(
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
            rule: "timeout_later_rpc_attempt".into(),
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
            "{selected} SELECT event_seq,source,metadata_json FROM selected WHERE kind='rpc.dispatch_prevented' ORDER BY event_seq LIMIT {}",
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
            report.gaps.push(
                "Timeout analysis reached the 256-incident limit; narrow the selection".into(),
            );
        }
        for (prevented, source, text) in rows.into_iter().take(MAX_INCIDENTS) {
            query::check(cancel, deadline)?;
            let fact: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
            let request = fact["request_id"].as_str().unwrap_or("");
            let method = fact["data"]["method"].as_str().unwrap_or("");
            let registration = fact["registration_seq"].as_u64().unwrap_or(0);
            let mode = fact["execution_mode"].as_str().unwrap_or("");
            if source != "ronchi"
                || fact["schema_version"] != 1
                || !["simulator", "surfpool"].contains(&mode)
                || fact["data"]["dispatch"] != "prevented"
                || fact["policy_ref"]["case_id"] != "provider-timeout-burst"
            {
                rule.unsupported += 1;
                continue;
            }
            if request.is_empty() || method.is_empty() || registration == 0 {
                rule.insufficient_evidence += 1;
                continue;
            }
            let n = values.len();
            let sql = format!(
                "{selected} SELECT event_seq,kind,source,metadata_json FROM selected WHERE (kind='fault.withholding_started' AND json_extract(metadata_json,'$.registration_seq')=?{}) OR (kind='rpc.boundary' AND request_id=?{}) ORDER BY event_seq LIMIT 2",
                n + 1,
                n + 2
            );
            let mut bound = values.to_vec();
            bound.extend([(registration as i64).into(), request.to_string().into()]);
            let mut stmt = self
                .connection
                .prepare(&sql)
                .map_err(|_| Error::storage())?;
            let support: Vec<(u64, String, String, String)> = stmt
                .query_map(rusqlite::params_from_iter(bound), |r| {
                    Ok((r.get::<_, i64>(0)? as u64, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .map_err(|_| Error::storage())?
                .collect::<rusqlite::Result<_>>()
                .map_err(|_| Error::storage())?;
            let mut held = None;
            let mut boundary = None;
            for (seq, kind, source, text) in support {
                let data: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
                if source != "ronchi"
                    || data["schema_version"] != 1
                    || data["execution_mode"] != mode
                {
                    continue;
                }
                if kind == "fault.withholding_started"
                    && data["data"]["fault"] == "provider-timeout-burst"
                    && data["data"]["dispatch"] == "prevented"
                {
                    held = Some(seq);
                }
                if kind == "rpc.boundary"
                    && data["data"]["dispatch"] == "prevented"
                    && data["data"]["rpc_response"] == "no_transport_response"
                {
                    boundary = Some(seq);
                }
            }
            let (Some(held), Some(boundary)) = (held, boundary) else {
                rule.insufficient_evidence += 1;
                report.gaps.push("Timeout interpretation requires selected hold-start and actual transport boundary evidence".into());
                continue;
            };
            let sql = format!(
                "{selected} SELECT event_seq,source,metadata_json FROM selected WHERE kind='rpc.registered' AND rpc_method=?{} AND event_seq>?{} AND json_extract(metadata_json,'$.registration_seq')>?{} ORDER BY event_seq LIMIT 1",
                n + 1,
                n + 2,
                n + 3
            );
            let mut bound = values.to_vec();
            bound.extend([
                method.to_string().into(),
                (prevented as i64).into(),
                (registration as i64).into(),
            ]);
            let later: Option<(u64, String, String)> = self
                .connection
                .query_row(&sql, rusqlite::params_from_iter(bound), |r| {
                    Ok((r.get::<_, i64>(0)? as u64, r.get(1)?, r.get(2)?))
                })
                .optional()
                .map_err(|_| Error::storage())?;
            let Some((later, source, text)) = later else {
                rule.no_match += 1;
                continue;
            };
            let later_fact: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
            if source != "ronchi"
                || later_fact["execution_mode"] != mode
                || later_fact["schema_version"] != 1
                || prevented >= boundary
                || held >= boundary
                || boundary > snapshot
                || later > snapshot
            {
                rule.insufficient_evidence += 1;
                continue;
            }
            let signature = fact["transaction_signatures"]
                .as_array()
                .filter(|a| a.len() == 1)
                .and_then(|a| a[0].as_str())
                .map(str::to_owned);
            let scope = FindingScope::RpcRequest {
                request_id: request.into(),
                rpc_method: method.into(),
                signature,
                from_seq: prevented.min(held),
                to_seq: boundary.max(later),
            };
            let supporting_events: Vec<_> = [
                ("dispatch_prevented", prevented),
                ("observed_withholding", held),
                ("transport_boundary", boundary),
                ("later_rpc_attempt", later),
            ]
            .into_iter()
            .map(|(role, event_seq)| Support {
                role: role.into(),
                event_ref: EventRef {
                    lab_id: lab.into(),
                    event_seq,
                },
            })
            .collect();
            let id = crate::query::digest(
                &serde_json::to_vec(&(
                    lab,
                    &scope,
                    &supporting_events,
                    crate::cases::timeout::finding_definition(),
                ))
                .expect("typed finding identity serializes"),
            );
            rule.matched += 1;
            report.findings.push(Finding{finding_id:format!("f1-{id}"),rule_id:"provider-timeout-burst.later-rpc-attempt".into(),version:1,lab_id:lab.into(),scope,supporting_events,statement:format!("Ronchi prevented dispatch of {method}, withheld its HTTP response, and later registered another {method} request."),guidance:"Inspect the application's retry deadline and recovery behavior using the linked RPC records.".into(),limitations:vec!["The RPC boundary does not establish a client timeout, causal retry, hidden intent, or application completion.".into(),"Non-dispatch excludes acceptance from this attempt; an earlier attempt may already have been accepted.".into()]});
        }
        report.rules.push(rule);
        Ok(())
    }
}
