//! Expiry Findings use selected source facts and the actual submission result.
use super::*;
use crate::findings::{FindingScope, Support};
use rusqlite::types::Value as Sql;
struct Fact {
    seq: u64,
    source: String,
    meta: Value,
}
fn decoded((seq, source, text): (u64, String, String)) -> Result<Fact, Error> {
    if text.len() > 64 * 1024 {
        return Err(Error::new(
            "result_too_large",
            "Blockhash evidence exceeds its metadata budget",
        ));
    }
    Ok(Fact {
        seq,
        source,
        meta: serde_json::from_str(&text).map_err(|_| Error::storage())?,
    })
}
impl JournalReader {
    #[allow(
        clippy::too_many_arguments,
        reason = "Shares the bounded pinned selection with the other maintained rules"
    )]
    pub(super) fn blockhash_findings(
        &self,
        selected: &str,
        values: &[Sql],
        lab: &str,
        snapshot: u64,
        deadline: Instant,
        cancel: &CancellationToken,
        report: &mut Report,
    ) -> Result<(), Error> {
        let mut rule = RuleReport {
            rule: "expired_recent_blockhash_submission".into(),
            legacy: None,
            version: 1,
            matched: 0,
            no_match: 0,
            insufficient_evidence: 0,
            unsupported: 0,
            operational_limit: false,
            state: State::NoMatch,
        };
        let n = values.len();
        let one = |predicate: String, extra: Vec<Sql>| -> Result<Option<Fact>, Error> {
            let sql = format!(
                "{selected} SELECT event_seq,source,metadata_json FROM selected WHERE {predicate} ORDER BY event_seq LIMIT 1"
            );
            let mut bound = values.to_vec();
            bound.extend(extra);
            self.connection
                .query_row(&sql, rusqlite::params_from_iter(bound), |row| {
                    Ok((row.get::<_, i64>(0)? as u64, row.get(1)?, row.get(2)?))
                })
                .optional()
                .map_err(|_| Error::storage())?
                .map(decoded)
                .transpose()
        };
        let sql = format!(
            "{selected} SELECT s.event_seq,s.source,s.metadata_json FROM selected s WHERE s.kind='transaction.signed_attempt' AND json_extract(s.metadata_json,'$.data.lifetime')='recent_blockhash' AND json_extract(s.metadata_json,'$.data.expiry_claim_eligible')=1 AND EXISTS(SELECT 1 FROM selected r WHERE r.kind='fault.blockhash_returned' AND json_extract(r.metadata_json,'$.data.blockhash')=json_extract(s.metadata_json,'$.data.blockhash') AND json_extract(r.metadata_json,'$.data.expiry_observed_elapsed_ns') IS NOT NULL AND r.event_seq<s.event_seq) ORDER BY s.event_seq LIMIT {}",
            MAX_INCIDENTS + 1
        );
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|_| Error::storage())?;
        let attempts: Vec<(u64, String, String)> = statement
            .query_map(rusqlite::params_from_iter(values), |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get(1)?, row.get(2)?))
            })
            .map_err(|_| Error::storage())?
            .collect::<rusqlite::Result<_>>()
            .map_err(|_| Error::storage())?;
        if attempts.len() > MAX_INCIDENTS {
            rule.operational_limit = true;
            report.complete = false;
            report
                .gaps
                .push("Expired blockhash analysis reached the 256-submission limit".into());
        }
        if attempts.is_empty()&&one("kind='fault.blockhash_returned' AND json_extract(metadata_json,'$.data.expiry_observed_elapsed_ns') IS NOT NULL".into(),vec![])?.is_some(){rule.no_match+=1;report.gaps.push("An expired cached tuple was selected, but no matching recent-blockhash submission was observed in this selection".into());}
        if attempts.is_empty()&&rule.no_match==0&&one("kind='transaction.signed_attempt' AND json_extract(metadata_json,'$.policy_ref.case_id')='tx-blockhash-expired'".into(),vec![])?.is_some(){
            rule.insufficient_evidence+=1;report.gaps.push("The selected blockhash submission lacks its earlier source tuple and height context; remove narrow signature or method filters to inspect that support".into());
        }
        for attempt in attempts.into_iter().take(MAX_INCIDENTS) {
            query::check(cancel, deadline)?;
            let attempt = decoded(attempt)?;
            let data = &attempt.meta["data"];
            let Some(signature) = attempt.meta["transaction_signature"].as_str() else {
                rule.insufficient_evidence += 1;
                continue;
            };
            let Some(request) = attempt.meta["request_id"].as_str() else {
                rule.insufficient_evidence += 1;
                continue;
            };
            let Some(hash) = data["blockhash"].as_str() else {
                rule.insufficient_evidence += 1;
                continue;
            };
            let Some(authority) = data["source_identity"].as_str() else {
                rule.insufficient_evidence += 1;
                continue;
            };
            let mode = attempt.meta["execution_mode"].as_str().unwrap_or("");
            let source = match mode {
                "simulator" => "rpc_simulator",
                "surfpool" => "surfpool",
                _ => {
                    rule.unsupported += 1;
                    continue;
                }
            };
            if attempt.source != "ronchi" {
                rule.unsupported += 1;
                continue;
            }
            let returned = one(
                format!(
                    "kind='fault.blockhash_returned' AND event_seq<?{} AND json_extract(metadata_json,'$.data.blockhash')=?{} AND json_extract(metadata_json,'$.data.source_identity')=?{} AND json_extract(metadata_json,'$.data.expiry_observed_elapsed_ns') IS NOT NULL",
                    n + 1,
                    n + 2,
                    n + 3
                ),
                vec![
                    (attempt.seq as i64).into(),
                    hash.to_string().into(),
                    authority.to_string().into(),
                ],
            )?;
            let Some(returned) = returned else {
                rule.insufficient_evidence += 1;
                continue;
            };
            let view = &returned.meta["data"];
            if view["signature_filter"].as_array().is_some_and(|filter| {
                !filter.is_empty() && !filter.iter().any(|value| value == signature)
            }) {
                rule.no_match += 1;
                continue;
            }
            let Some(capture_request) = view["capture_request_id"].as_str() else {
                rule.insufficient_evidence += 1;
                continue;
            };
            let Some(read_request) = returned.meta["request_id"].as_str() else {
                rule.insufficient_evidence += 1;
                continue;
            };
            let Some(bound) = view["last_valid_block_height"].as_u64() else {
                rule.insufficient_evidence += 1;
                continue;
            };
            let Some(commitment) = view["commitment"].as_str() else {
                rule.insufficient_evidence += 1;
                continue;
            };
            let captured = one(
                format!("kind='rpc.blockhash_source' AND request_id=?{}", n + 1),
                vec![capture_request.to_string().into()],
            )?;
            let height = one(
                format!(
                    "kind='rpc.blockhash_height' AND request_id=?{} AND event_seq<?{} AND json_extract(metadata_json,'$.data.source_identity')=?{} AND json_extract(metadata_json,'$.data.commitment')=?{} AND json_extract(metadata_json,'$.data.block_height')>?{}",
                    n + 1,
                    n + 2,
                    n + 3,
                    n + 4,
                    n + 5
                ),
                vec![
                    read_request.to_string().into(),
                    (returned.seq as i64).into(),
                    authority.to_string().into(),
                    commitment.to_string().into(),
                    (bound.min(i64::MAX as u64) as i64).into(),
                ],
            )?;
            let read_boundary = one(
                format!(
                    "kind='rpc.boundary' AND request_id=?{} AND json_extract(metadata_json,'$.data.rpc_response')='result'",
                    n + 1
                ),
                vec![read_request.to_string().into()],
            )?;
            let admission = one(
                format!(
                    "kind='rpc.admission_result' AND request_id=?{} AND event_seq>?{}",
                    n + 1,
                    n + 2
                ),
                vec![request.to_string().into(), (attempt.seq as i64).into()],
            )?;
            let boundary = one(
                format!(
                    "kind='rpc.boundary' AND request_id=?{} AND event_seq>?{}",
                    n + 1,
                    n + 2
                ),
                vec![request.to_string().into(), (attempt.seq as i64).into()],
            )?;
            let processing = if mode == "simulator" {
                one(
                    format!(
                        "kind='transaction.processing_prevented' AND request_id=?{} AND json_extract(metadata_json,'$.data.reason')='recent_blockhash_expired'",
                        n + 1
                    ),
                    vec![request.to_string().into()],
                )?
            } else {
                one(
                    format!(
                        "kind='rpc.blockhash_status' AND event_seq>?{} AND json_extract(metadata_json,'$.transaction_signature')=?{} AND json_extract(metadata_json,'$.data.source_identity')=?{}",
                        n + 1,
                        n + 2,
                        n + 3
                    ),
                    vec![
                        (attempt.seq as i64).into(),
                        signature.to_string().into(),
                        authority.to_string().into(),
                    ],
                )?
            };
            let (
                Some(captured),
                Some(height),
                Some(read_boundary),
                Some(admission),
                Some(boundary),
                Some(processing),
            ) = (
                captured,
                height,
                read_boundary,
                admission,
                boundary,
                processing,
            )
            else {
                rule.insufficient_evidence += 1;
                report.gaps.push("Expired submission requires selected genuine tuple, same-view source height, stale read boundary, submission admission, processing/status, and submission boundary evidence".into());
                continue;
            };
            if processing.meta["data"]["source_redacted"] == true {
                rule.insufficient_evidence += 1;
                report.gaps.push("Expired submission processing/status support is redacted inside this selection".into());
                continue;
            }
            let original = &captured.meta["data"];
            if height.meta["data"]["block_height"]
                .as_u64()
                .is_none_or(|height| height <= bound)
                || returned.source != "ronchi"
                || read_boundary.source != "ronchi"
                || boundary.source != "ronchi"
                || read_boundary.meta["execution_mode"] != mode
            {
                rule.insufficient_evidence += 1;
                continue;
            }
            if captured.source != source
                || height.source != source
                || admission.source != source
                || processing.source != source
                || [
                    &captured,
                    &height,
                    &returned,
                    &admission,
                    &boundary,
                    &processing,
                ]
                .iter()
                .any(|fact| fact.meta["execution_mode"] != mode)
                || original["source_identity"] != authority
                || original["commitment"] != commitment
                || original["tuple"]["blockhash"] != hash
                || original["tuple"]["lastValidBlockHeight"] != bound
                || original["tuple"]["contextSlot"] != view["context_slot"]
                || admission.meta["data"]["source_identity"] != authority
                || admission.meta["transaction_signature"] != signature
                || !(captured.seq < height.seq
                    && height.seq < returned.seq
                    && returned.seq < attempt.seq
                    && attempt.seq < admission.seq
                    && admission.seq < boundary.seq
                    && boundary.seq <= snapshot)
            {
                rule.insufficient_evidence += 1;
                continue;
            }
            if mode == "simulator"
                && processing.meta["data"]["observed_block_height"]
                    .as_u64()
                    .is_none_or(|value| value <= bound)
            {
                rule.insufficient_evidence += 1;
                continue;
            }
            let acceptance = if mode == "simulator" {
                one(
                    format!("kind='transaction.accepted' AND request_id=?{}", n + 1),
                    vec![request.to_string().into()],
                )?
            } else {
                None
            };
            if mode == "simulator"
                && acceptance.as_ref().is_none_or(|fact| {
                    fact.source != "rpc_simulator"
                        || fact.meta["execution_mode"] != mode
                        || fact.meta["data"]["blockhash"] != hash
                        || fact.meta["transaction_signature"] != signature
                })
            {
                rule.insufficient_evidence += 1;
                continue;
            }
            let mut supports = vec![
                ("original_tuple", &captured),
                ("expiry_height", &height),
                ("stale_tuple_view", &returned),
                ("stale_read_boundary", &read_boundary),
                ("signed_submission", &attempt),
                ("admission_result", &admission),
                ("processing_or_status", &processing),
                ("submission_boundary", &boundary),
            ];
            if let Some(acceptance) = &acceptance {
                supports.push(("acceptance", acceptance));
            }
            let last = supports
                .iter()
                .map(|(_, fact)| fact.seq)
                .max()
                .unwrap_or(boundary.seq);
            let supporting_events: Vec<Support> = supports
                .into_iter()
                .map(|(role, fact)| Support {
                    role: role.into(),
                    event_ref: EventRef {
                        lab_id: lab.into(),
                        event_seq: fact.seq,
                    },
                })
                .collect();
            let scope = if mode == "simulator" {
                FindingScope::AcceptedTransaction {
                    signature: signature.into(),
                    accepted_request_id: request.into(),
                    from_seq: captured.seq,
                    to_seq: last,
                }
            } else {
                FindingScope::RpcRequest {
                    request_id: request.into(),
                    rpc_method: "sendTransaction".into(),
                    signature: Some(signature.into()),
                    from_seq: captured.seq,
                    to_seq: last,
                }
            };
            let outcome = admission.meta["data"]["admission"]
                .as_str()
                .unwrap_or("unknown");
            let statement = if mode == "simulator" {
                format!(
                    "A signed submission used a recent blockhash after its source height exceeded {bound}; the simulator accepted its signature and recorded processing prevented by expiry."
                )
            } else {
                format!(
                    "A signed submission used a recent blockhash after the same backend view exceeded {bound}; Surfpool returned {outcome}. Its collected status remains a separate source observation."
                )
            };
            let definition = crate::cases::blockhash::finding_definition();
            let finding_id = format!(
                "f1-{}",
                crate::query::digest(
                    &serde_json::to_vec(&(definition, lab, &scope, &supporting_events))
                        .expect("typed finding serializes")
                )
            );
            report.findings.push(Finding{finding_id,rule_id:"recent-blockhash.expired-submission".into(),version:1,lab_id:lab.into(),scope,supporting_events,statement,guidance:"Inspect whether the application obtains a fresh blockhash and tracks any replacement signature.".into(),limitations:vec!["Source height and lastValidBlockHeight establish this bounded expiry comparison; they do not prove a client received a response or reused a specific RPC call.".into(),"A backend error or null status does not establish non-execution, application intent, or an economic outcome.".into()]});
            rule.matched += 1;
        }
        report.rules.push(rule);
        Ok(())
    }
}
