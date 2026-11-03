//! Bounded metadata joins within the Investigation reader's pinned transaction.
use super::JournalReader;
use crate::{
    findings::{self, Evidence, Fact, FactKind, Finding, Rule, State},
    query::{self, Error, EventRef},
};
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::Value;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

const MAX_INCIDENTS: usize = 256;
#[derive(Serialize)]
pub(super) struct RuleReport {
    rule: String,
    #[serde(skip)]
    legacy: Option<Rule>,
    version: u32,
    matched: usize,
    no_match: usize,
    insufficient_evidence: usize,
    unsupported: usize,
    operational_limit: bool,
    state: State,
}
#[derive(Serialize)]
pub(super) struct Report {
    pub findings: Vec<Finding>,
    pub rules: Vec<RuleReport>,
    pub complete: bool,
    pub gaps: Vec<String>,
}
impl JournalReader {
    pub(super) fn findings(
        &self,
        selected_cte: &str,
        values: &[rusqlite::types::Value],
        lab: &str,
        snapshot: u64,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<Report, Error> {
        let mut report = Report {
            findings: vec![],
            rules: [
                Rule::LookupAfterWithholding,
                Rule::SubmissionAfterWithholding,
            ]
            .into_iter()
            .map(|rule| RuleReport {
                rule: serde_json::to_value(rule)
                    .expect("rule serializes")
                    .as_str()
                    .expect("rule name")
                    .into(),
                legacy: Some(rule),
                version: 1,
                matched: 0,
                no_match: 0,
                insufficient_evidence: 0,
                unsupported: 0,
                operational_limit: false,
                state: State::NoMatch,
            })
            .collect(),
            complete: true,
            gaps: vec![],
        };
        // Scope filtering is shared with Timeline. No supporting fact escapes its bounds.
        let sql = format!(
            "{selected_cte} SELECT request_id, min(event_seq) FROM selected WHERE kind IN ('transaction.accepted','fault.response_loss_selected','fault.withholding_started') GROUP BY request_id ORDER BY min(event_seq) LIMIT {}",
            MAX_INCIDENTS + 1
        );
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|_| Error::storage())?;
        let candidates: Vec<(Option<String>, u64)> = statement
            .query_map(rusqlite::params_from_iter(values), |row| {
                Ok((row.get(0)?, row.get::<_, i64>(1)? as u64))
            })
            .map_err(|_| Error::storage())?
            .collect::<rusqlite::Result<_>>()
            .map_err(|_| Error::storage())?;
        drop(statement);
        if candidates.len() > MAX_INCIDENTS {
            report.complete = false;
            report.gaps.push(
                "Analysis stopped at the 256-incident operational limit; narrow the explicit scope"
                    .into(),
            );
            for rule in &mut report.rules {
                rule.operational_limit = true;
            }
        }
        // Reuse the bound selection as a CTE for every indexed metadata join.
        let original = format!(
            "{selected_cte} SELECT event_seq,kind,source,metadata_json FROM selected WHERE request_id=?{} AND kind=?{} ORDER BY event_seq LIMIT 1",
            values.len() + 1,
            values.len() + 2
        );
        let mut original_stmt = self
            .connection
            .prepare(&original)
            .map_err(|_| Error::storage())?;
        for (request, _) in candidates.into_iter().take(MAX_INCIDENTS) {
            query::check(cancel, deadline)?;
            let Some(request) = request else {
                for rule in &mut report.rules {
                    rule.insufficient_evidence += 1;
                }
                report
                    .gaps
                    .push("An incident lacks its internal request identity".into());
                continue;
            };
            let mut originals = Vec::with_capacity(4);
            for kind in [
                "transaction.submission",
                "transaction.accepted",
                "fault.response_loss_selected",
                "fault.withholding_started",
            ] {
                let mut bound = values.to_vec();
                bound.push(request.clone().into());
                bound.push(kind.to_string().into());
                let row = original_stmt
                    .query_row(rusqlite::params_from_iter(bound), row_data)
                    .optional()
                    .map_err(|_| Error::storage())?;
                originals.push(row.map(|r| decode(lab, r)).transpose()?);
            }
            let signature = originals
                .iter()
                .flatten()
                .find_map(|f| (!f.signature.is_empty()).then(|| f.signature.clone()));
            for rule_report in &mut report.rules {
                let Some(rule) = rule_report.legacy else {
                    continue;
                };
                let later = if let (Some(signature), Some(withholding), Some(acceptance)) =
                    (&signature, &originals[3], &originals[1])
                {
                    let predicate = match rule {
                        Rule::LookupAfterWithholding => {
                            "kind='rpc.registered' AND rpc_method IN ('getSignatureStatuses','getTransaction')"
                        }
                        Rule::SubmissionAfterWithholding => "kind='transaction.submission'",
                    };
                    let n = values.len();
                    let signature_join = if self.version >= 6 {
                        format!(
                            "event_seq IN (SELECT event_seq FROM query_signatures WHERE signature=?{})",
                            n + 4
                        )
                    } else {
                        format!(
                            "EXISTS(SELECT 1 FROM json_each(json_extract(metadata_json,'$.transaction_signatures')) WHERE value=?{})",
                            n + 4
                        )
                    };
                    let sql = format!(
                        "{selected_cte} SELECT event_seq,kind,source,metadata_json FROM selected WHERE {predicate} AND event_seq>?{} AND request_id<>?{} AND json_extract(metadata_json,'$.registration_seq')>?{} AND {signature_join} ORDER BY event_seq LIMIT 1",
                        n + 1,
                        n + 2,
                        n + 3,
                    );
                    let mut bound = values.to_vec();
                    bound.extend([
                        rusqlite::types::Value::Integer(withholding.event_ref.event_seq as i64),
                        request.clone().into(),
                        rusqlite::types::Value::Integer(acceptance.registration_seq as i64),
                        signature.clone().into(),
                    ]);
                    self.connection
                        .query_row(&sql, rusqlite::params_from_iter(bound), row_data)
                        .optional()
                        .map_err(|_| Error::storage())?
                        .map(|r| {
                            let mut fact = decode(lab, r)?;
                            fact.signature = signature.clone();
                            Ok::<_, Error>(fact)
                        })
                        .transpose()?
                } else {
                    None
                };
                let evidence = Evidence {
                    lab_id: lab.into(),
                    snapshot_seq: snapshot,
                    submission: originals[0].clone(),
                    acceptance: originals[1].clone(),
                    selection: originals[2].clone(),
                    withholding: originals[3].clone(),
                    later,
                };
                let evaluation = findings::evaluate(rule, &evidence);
                match evaluation.state {
                    State::Matched => rule_report.matched += 1,
                    State::NoMatch => rule_report.no_match += 1,
                    State::InsufficientEvidence => rule_report.insufficient_evidence += 1,
                    State::Unsupported => rule_report.unsupported += 1,
                    State::OperationalLimit => rule_report.operational_limit = true,
                }
                if let Some(finding) = evaluation.finding {
                    report.findings.push(finding);
                }
                report.gaps.extend(evaluation.evidence_gaps);
            }
        }
        // A narrow scope can contain a later action but omit its incident prerequisites.
        if report
            .rules
            .iter()
            .all(|r| r.matched + r.no_match + r.insufficient_evidence + r.unsupported == 0)
        {
            let sql = format!(
                "{selected_cte} SELECT EXISTS(SELECT 1 FROM selected WHERE kind='transaction.submission' OR (kind='rpc.registered' AND rpc_method IN ('getSignatureStatuses','getTransaction')))"
            );
            let potential: bool = self
                .connection
                .query_row(&sql, rusqlite::params_from_iter(values), |r| r.get(0))
                .map_err(|_| Error::storage())?;
            if potential {
                for rule in &mut report.rules {
                    rule.insufficient_evidence += 1;
                }
                report.gaps.push(
                    "Selected actions lack the required incident evidence inside this scope".into(),
                );
            }
        }
        if self.version < 6 {
            report.gaps.push("Legacy correlation may be missing; associations are not reconstructed by inference".into());
        }
        self.status_findings(
            selected_cte,
            values,
            lab,
            snapshot,
            deadline,
            cancel,
            &mut report,
        )?;
        self.confirmation_findings(
            selected_cte,
            values,
            lab,
            snapshot,
            deadline,
            cancel,
            &mut report,
        )?;
        self.timeout_findings(
            selected_cte,
            values,
            lab,
            snapshot,
            deadline,
            cancel,
            &mut report,
        )?;
        self.blockhash_findings(
            selected_cte,
            values,
            lab,
            snapshot,
            deadline,
            cancel,
            &mut report,
        )?;
        for rule in &mut report.rules {
            rule.state = if rule.operational_limit {
                State::OperationalLimit
            } else if rule.matched > 0 {
                State::Matched
            } else if rule.unsupported > 0 {
                State::Unsupported
            } else if rule.insufficient_evidence > 0 {
                State::InsufficientEvidence
            } else {
                State::NoMatch
            };
        }
        report.gaps.sort();
        report.gaps.dedup();
        Ok(report)
    }
}
#[path = "findings/confirmation.rs"]
mod confirmation;
#[path = "findings/status.rs"]
mod status;
#[path = "findings/timeout.rs"]
mod timeout;
type RowData = (u64, String, String, String);
fn row_data(row: &rusqlite::Row<'_>) -> rusqlite::Result<RowData> {
    Ok((
        row.get::<_, i64>(0)? as u64,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
    ))
}
fn decode(lab: &str, (seq, kind, source, text): RowData) -> Result<Fact, Error> {
    if text.len() > 64 * 1024 {
        return Err(Error::new(
            "result_too_large",
            "Retained finding metadata exceeds its item budget",
        ));
    }
    let value: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
    let kind = match kind.as_str() {
        "transaction.submission" => FactKind::Submission,
        "transaction.accepted" => FactKind::Acceptance,
        "fault.response_loss_selected" => FactKind::Selection,
        "fault.withholding_started" => FactKind::Withholding,
        "rpc.registered" => {
            if value["data"]["method"] == "getTransaction" {
                FactKind::TransactionRequest
            } else {
                FactKind::StatusRequest
            }
        }
        _ => return Err(Error::storage()),
    };
    let signature = value["transaction_signature"]
        .as_str()
        .or_else(|| {
            value["transaction_signatures"]
                .get(0)
                .and_then(Value::as_str)
        })
        .unwrap_or("")
        .to_string();
    Ok(Fact {
        event_ref: EventRef {
            lab_id: lab.into(),
            event_seq: seq,
        },
        schema_version: value["schema_version"]
            .as_u64()
            .and_then(|n| n.try_into().ok())
            .unwrap_or(0),
        execution_mode: serde_json::from_value(value["execution_mode"].clone())
            .map_err(|_| Error::storage())?,
        source,
        kind,
        request_id: value["request_id"].as_str().unwrap_or("").into(),
        registration_seq: value["registration_seq"].as_u64().unwrap_or(0),
        signature,
        digest: value["data"]["digest"].as_str().map(String::from),
    })
}

#[path = "findings/blockhash.rs"]
mod blockhash;
