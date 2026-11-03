//! Only selected, source-attributed transition facts establish an observed delayed schedule.
use super::*;
use crate::findings::{FindingScope, Support};

impl JournalReader {
    #[allow(
        clippy::too_many_arguments,
        reason = "Shares the pinned bounded selection with the legacy evaluator"
    )]
    pub(super) fn confirmation_findings(
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
            rule: "confirmation_delay_observed".into(),
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
            "{selected} SELECT event_seq,source,metadata_json FROM selected WHERE kind='fault.confirmation_schedule_assigned' ORDER BY event_seq LIMIT {}",
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
                .push("Confirmation analysis reached the 256-incident limit".into());
        }
        for (selection_seq, source, text) in rows.into_iter().take(MAX_INCIDENTS) {
            query::check(cancel, deadline)?;
            let assignment: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
            if assignment["execution_mode"] != "simulator" || source != "ronchi" {
                rule.unsupported += 1;
                continue;
            }
            let signature = assignment["transaction_signature"].as_str().unwrap_or("");
            let request = assignment["request_id"].as_str().unwrap_or("");
            let n = values.len();
            let sql = format!(
                "{selected} SELECT event_seq,source,metadata_json FROM selected WHERE (kind='transaction.accepted' AND request_id=?{}) OR (kind='transaction.transition' AND json_extract(metadata_json,'$.transaction_signature')=?{}) ORDER BY event_seq LIMIT 5",
                n + 1,
                n + 2
            );
            let mut bound = values.to_vec();
            bound.extend([request.to_string().into(), signature.to_string().into()]);
            let mut stmt = self
                .connection
                .prepare(&sql)
                .map_err(|_| Error::storage())?;
            let facts: Vec<(u64, String, String)> = stmt
                .query_map(rusqlite::params_from_iter(bound), |r| {
                    Ok((r.get::<_, i64>(0)? as u64, r.get(1)?, r.get(2)?))
                })
                .map_err(|_| Error::storage())?
                .collect::<rusqlite::Result<_>>()
                .map_err(|_| Error::storage())?;
            let mut accepted = None;
            let mut processed = None;
            let mut recovered = None;
            let hold_at = assignment["data"]["schedule"]["hold_at"]
                .as_str()
                .unwrap_or("");
            for (seq, source, text) in facts {
                let fact: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
                if source != "rpc_simulator"
                    || fact["execution_mode"] != "simulator"
                    || fact["transaction_signature"] != signature
                {
                    continue;
                }
                if seq < selection_seq && fact["data"]["accepted_ns"].is_u64() {
                    accepted = Some(seq);
                }
                if fact["data"]["stage"] == hold_at {
                    processed = Some((
                        seq,
                        fact["data"]["scheduled_transition_ns"]
                            .as_u64()
                            .unwrap_or(0),
                    ));
                }
                if fact["data"]["stage"]
                    == if hold_at == "processed" {
                        "confirmed"
                    } else {
                        "finalized"
                    }
                {
                    recovered = Some((
                        seq,
                        fact["data"]["scheduled_transition_ns"]
                            .as_u64()
                            .unwrap_or(0),
                    ));
                }
            }
            let (Some(accepted), Some((start_seq, start)), Some((end_seq, end))) =
                (accepted, processed, recovered)
            else {
                rule.insufficient_evidence += 1;
                report.gaps.push("Confirmation delay requires selected acceptance, held commitment, and recovery transition evidence".into());
                continue;
            };
            if signature.is_empty()
                || !(accepted < selection_seq
                    && selection_seq < start_seq
                    && start_seq < end_seq
                    && end_seq <= snapshot
                    && start < end)
            {
                rule.insufficient_evidence += 1;
                continue;
            }
            rule.matched += 1;
            report.findings.push(Finding {
                finding_id: format!("f1-{}", crate::query::digest(&serde_json::to_vec(&(lab, signature, request, selection_seq, end_seq, crate::cases::confirmation::finding_definition())).expect("typed identity serializes"))), rule_id: "sim.confirmation-delay.observed".into(), version: 1, lab_id: lab.into(),
                scope: FindingScope { signature: signature.into(), accepted_request_id: request.into(), from_seq: accepted, to_seq: end_seq },
                supporting_events: [("acceptance",accepted),("schedule_assignment",selection_seq),("held_commitment",start_seq),("recovery",end_seq)].into_iter().map(|(role,event_seq)| Support { role: role.into(),event_ref: EventRef { lab_id: lab.into(),event_seq } }).collect(),
                statement: format!("The simulator retained {hold_at} commitment for {} ms before the recorded recovery transition.",(end-start)/1_000_000),
                guidance: "Inspect the application's commitment deadline and reconciliation behavior in the linked RPC history.".into(),
                limitations: vec!["Simulated commitment does not establish validator finality, a client deadline, or application failure.".into()],
            });
        }
        report.rules.push(rule);
        Ok(())
    }
}
