use super::{JournalReader, query_index};
use crate::query::{self, Cursor, Error, Gap, Limits, Operation, Page, Request, Response, View};
use rusqlite::{params_from_iter, types::Value as SqlValue};
use serde_json::{Value, json};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

struct Selection {
    sql: String,
    values: Vec<SqlValue>,
}
impl Selection {
    fn new(request: &Request, snapshot: u64, position: u64, indexed: bool) -> Self {
        let s = &request.arguments.scope;
        let mut value = Self {
            sql: "q.event_seq>? AND q.event_seq<=?".into(),
            values: vec![
                SqlValue::Integer(position as i64),
                SqlValue::Integer(snapshot as i64),
            ],
        };
        if let Some(from) = s.from_seq {
            value.sql.push_str(" AND q.event_seq>=?");
            value.values.push(SqlValue::Integer(from as i64));
        }
        if let Some(to) = s.to_seq {
            value.sql.push_str(" AND q.event_seq<=?");
            value.values.push(SqlValue::Integer(to as i64));
        }
        value.list(
            "q.kind",
            s.kind.iter().map(|s| SqlValue::Text(s.clone())).collect(),
        );
        value.list(
            "q.rpc_method",
            s.rpc_method
                .iter()
                .map(|s| SqlValue::Text(s.clone()))
                .collect(),
        );
        value.list(
            "q.activation_id",
            s.case_activation_id
                .iter()
                .map(|n| SqlValue::Integer(*n as i64))
                .collect(),
        );
        value.list(
            "q.event_seq",
            s.event_refs
                .iter()
                .map(|r| SqlValue::Integer(r.event_seq as i64))
                .collect(),
        );
        if !s.signature.is_empty() {
            let placeholders = std::iter::repeat_n("?", s.signature.len())
                .collect::<Vec<_>>()
                .join(",");
            if indexed {
                value.sql.push_str(&format!(" AND (EXISTS(SELECT 1 FROM query_signatures s WHERE s.event_seq=q.event_seq AND s.signature IN ({placeholders})) OR (q.request_id IS NOT NULL AND EXISTS(SELECT 1 FROM query_events related JOIN query_signatures s ON s.event_seq=related.event_seq WHERE related.request_id=q.request_id AND related.event_seq<=? AND s.signature IN ({placeholders}))))"));
                value
                    .values
                    .extend(s.signature.iter().cloned().map(SqlValue::Text));
                value.values.push(SqlValue::Integer(snapshot as i64));
                value
                    .values
                    .extend(s.signature.iter().cloned().map(SqlValue::Text));
            } else {
                value.sql.push_str(&format!(" AND EXISTS(SELECT 1 FROM json_each(json_extract(q.metadata_json,'$.transaction_signatures')) s WHERE s.value IN ({placeholders}))"));
                value
                    .values
                    .extend(s.signature.iter().cloned().map(SqlValue::Text));
            }
        }
        for (bound, op) in [(&s.since, ">="), (&s.until, "<")] {
            if let Some(bound) = bound {
                let (seconds, nanos) = query::time_value(bound);
                value
                    .sql
                    .push_str(&format!(" AND (q.recorded_sec,q.recorded_nano){op}(?,?)"));
                value.values.push(SqlValue::Integer(seconds));
                value.values.push(SqlValue::Integer(i64::from(nanos)));
            }
        }
        value
    }
    fn list(&mut self, column: &str, values: Vec<SqlValue>) {
        if values.is_empty() {
            return;
        }
        self.sql.push_str(&format!(
            " AND {column} IN ({})",
            std::iter::repeat_n("?", values.len())
                .collect::<Vec<_>>()
                .join(",")
        ));
        self.values.extend(values);
    }
}
impl JournalReader {
    pub(crate) fn connection_for_query_progress(
        &mut self,
        deadline: Instant,
        cancel: CancellationToken,
    ) -> rusqlite::Result<()> {
        self.connection.busy_timeout(
            deadline
                .saturating_duration_since(Instant::now())
                .min(std::time::Duration::from_millis(50)),
        )?;
        self.connection.progress_handler(
            1000,
            Some(move || cancel.is_cancelled() || Instant::now() >= deadline),
        )?;
        query_index::functions(&self.connection)
    }
    fn query_cte(&self) -> String {
        if self.version >= 6 {
            "WITH q AS (SELECT * FROM query_events)".into()
        } else {
            format!(
                "WITH q AS ({})",
                query_index::projection("SELECT event_seq,event_json FROM events", false)
            )
        }
    }
    pub(crate) fn investigate(
        &self,
        request: Request,
        owner_alive: bool,
        basis: String,
        limits: &Limits,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<Response, Error> {
        query::check(cancel, deadline)?;
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|_| Error::storage())?;
        let latest = self.snapshot(owner_alive).map_err(|_| Error::storage())?;
        if latest.lab_id != request.lab_id {
            return Err(Error::invalid(
                "Selected immutable Lab identity does not match the Journal",
            ));
        }
        let fingerprint = request.fingerprint();
        let cursor = if let Some(token) = &request.arguments.cursor {
            let cursor = Cursor::decode(token)?;
            if cursor.fingerprint != fingerprint || cursor.snapshot > latest.snapshot_seq {
                return Err(Error::cursor());
            }
            cursor
        } else {
            Cursor {
                v: 1,
                snapshot: request
                    .arguments
                    .snapshot_seq
                    .unwrap_or(latest.snapshot_seq),
                position: 0,
                fingerprint,
                state: latest.state,
            }
        };
        if cursor.snapshot > latest.snapshot_seq
            || request
                .arguments
                .snapshot_seq
                .is_some_and(|n| n != cursor.snapshot)
        {
            return Err(Error::cursor());
        }
        if request.operation == Operation::Summary && request.arguments.cursor.is_some() {
            return Err(Error::cursor());
        }
        let mut gaps=vec![Gap{code:"application_coverage_unknown".into(),message:"Journal selection does not establish complete application observations or business correctness".into()}];
        if cursor.state == super::JournalState::Interrupted {
            gaps.push(Gap{code:"journal_interrupted".into(),message:"The Journal ended without clean finalization; later boundary observations may be missing".into()});
        }
        if self.version < 6 {
            gaps.push(Gap{code:"legacy_correlation".into(),message:"This legacy schema projects retained fields in SQL; missing request/signature associations cannot be invented".into()});
        }
        let cte = self.query_cte();
        if (request.arguments.scope.since.is_some()||request.arguments.scope.until.is_some())&&self.connection.query_row(&format!("{cte} SELECT EXISTS(SELECT 1 FROM q WHERE recorded_sec IS NULL AND event_seq<=?1)"),[cursor.snapshot as i64],|row|row.get::<_,bool>(0)).map_err(|_|Error::storage())?{
        gaps.push(Gap{code:"invalid_recorded_time".into(),message:"Some retained records cannot be matched against the selected time interval".into()});
    }
        let mut response = Response {
            contract_version: 1,
            request_id: request.request_id.clone(),
            lab_id: request.lab_id.clone(),
            selection_basis: basis,
            result: json!({"items":[]}),
            view: View {
                snapshot_seq: cursor.snapshot,
                scope: request.arguments.scope.clone(),
                fields: request.arguments.fields.clone(),
                include_payload: request.arguments.include_payload,
                observation_boundary: vec!["rpc".into(), "simulator".into()],
                evidence_gaps: gaps,
                journal_state: cursor.state,
                selection_complete: true,
            },
            page: Page {
                next_cursor: None,
                truncated: false,
                reason: None,
            },
        };
        let mut effective_request = request.clone();
        let mut analysis = None;
        if matches!(request.operation, Operation::Summary | Operation::Findings)
            || request.arguments.scope.finding_id.is_some()
        {
            let selection = Selection::new(&request, cursor.snapshot, 0, self.version >= 6);
            let selected_cte = format!(
                "{cte}, selected AS NOT MATERIALIZED (SELECT * FROM q WHERE {})",
                selection.sql
            );
            let report = self.findings(
                &selected_cte,
                &selection.values,
                &request.lab_id,
                cursor.snapshot,
                deadline,
                cancel,
            )?;
            response
                .view
                .evidence_gaps
                .extend(report.gaps.iter().map(|message| Gap {
                    code: "finding_evidence_gap".into(),
                    message: message.clone(),
                }));
            if !report.complete {
                response.view.selection_complete = false;
            }
            if let Some(id) = &request.arguments.scope.finding_id {
                let finding=report.findings.iter().find(|f|&f.finding_id==id).ok_or_else(||Error::new(if report.complete {"finding_unavailable"} else {"analysis_incomplete"},"Finding cannot be resolved inside the selected Lab, scope, snapshot, and rule versions"))?;
                // Keep all caller bounds. Add only the supporting reference intersection.
                let supports: Vec<_> = finding
                    .supporting_events
                    .iter()
                    .map(|s| s.event_ref.clone())
                    .collect();
                if effective_request.arguments.scope.event_refs.is_empty() {
                    effective_request.arguments.scope.event_refs = supports;
                } else {
                    effective_request
                        .arguments
                        .scope
                        .event_refs
                        .retain(|r| supports.contains(r));
                    if effective_request.arguments.scope.event_refs.is_empty() {
                        return Err(Error::invalid(
                            "Selected references exclude the Finding support",
                        ));
                    }
                }
            }
            analysis = Some(report);
        }
        let selected = Selection::new(
            &effective_request,
            cursor.snapshot,
            cursor.position,
            self.version >= 6,
        );
        if request.operation == Operation::Findings {
            let report = analysis.as_ref().expect("Findings was evaluated");
            response.result = json!({"items":[],"analysis":{"findings":if report.complete{"performed"}else{"operational_limit"},"rules":report.rules}});
            let start = usize::try_from(cursor.position).map_err(|_| Error::cursor())?;
            if start > report.findings.len() {
                return Err(Error::cursor());
            }
            let mut items = Vec::new();
            for (index, finding) in report
                .findings
                .iter()
                .enumerate()
                .skip(start)
                .take(usize::from(request.arguments.limit))
            {
                query::check(cancel, deadline)?;
                let mut item = finding_item(finding, &request, cursor.snapshot);
                let object = item.as_object_mut().expect("Finding object");
                object.retain(|key, _| {
                    [
                        "finding_id",
                        "rule_id",
                        "version",
                        "lab_id",
                        "scope",
                        "supporting_events",
                        "evidence_arguments",
                    ]
                    .contains(&key.as_str())
                        || request.arguments.fields.contains(key)
                });
                items.push(item);
                let more = index + 1 < report.findings.len();
                response.page.next_cursor = more.then(|| {
                    Cursor {
                        v: cursor.v,
                        snapshot: cursor.snapshot,
                        position: (index + 1) as u64,
                        fingerprint: cursor.fingerprint.clone(),
                        state: cursor.state,
                    }
                    .encode()
                });
                response.view.selection_complete = report.complete && !more;
                response.result["items"] = json!(items);
                if query::encoded_len(&response, limits.response_bytes).is_err() {
                    items.pop();
                    if items.is_empty() {
                        return Err(Error::new(
                            "result_too_large",
                            "A Finding does not fit the response budget",
                        ));
                    }
                    response.page.next_cursor = Some(
                        Cursor {
                            v: cursor.v,
                            snapshot: cursor.snapshot,
                            position: index as u64,
                            fingerprint: cursor.fingerprint.clone(),
                            state: cursor.state,
                        }
                        .encode(),
                    );
                    response.page.truncated = true;
                    response.page.reason = Some("response_bytes".into());
                    response.view.selection_complete = false;
                    response.result["items"] = json!(items);
                    break;
                }
            }
        } else if request.operation == Operation::Summary {
            let counts:(i64,i64,i64,i64,i64)=self.connection.query_row(&format!("{cte} SELECT count(*),coalesce(sum(q.kind='rpc.registered'),0),coalesce(sum(q.kind='transaction.accepted'),0),coalesce(sum(q.kind='fault.response_loss_selected'),0),coalesce(sum(q.kind='rpc.boundary'),0) FROM q WHERE {}",selected.sql),params_from_iter(selected.values.iter()),|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).map_err(|_|Error::storage())?;
            let total_rpc: i64 = self
                .connection
                .query_row(
                    &format!(
                        "{cte} SELECT count(*) FROM q WHERE kind='rpc.registered' AND event_seq<=?1"
                    ),
                    [cursor.snapshot as i64],
                    |row| row.get(0),
                )
                .map_err(|_| Error::storage())?;
            let context: String = self.connection.query_row(
                &format!("{cte} SELECT json_object('execution_mode',(SELECT json_extract(metadata_json,'$.execution_mode') FROM q WHERE event_seq<=?1 ORDER BY event_seq LIMIT 1),'case_binding',json((SELECT json_extract(metadata_json,'$.data.binding') FROM q WHERE kind='case.changed' AND event_seq<=?1 ORDER BY event_seq DESC LIMIT 1)))"),
                [cursor.snapshot as i64],|row|row.get(0)
            ).map_err(|_|Error::storage())?;
            let mut context: Value =
                serde_json::from_str(&context).map_err(|_| Error::storage())?;
            context["case_state"] = json!(if self.version < 5 {
                "unknown"
            } else if context["case_binding"]["policy"].is_null() {
                "not_armed"
            } else {
                "armed"
            });
            let report = analysis.as_ref().expect("Summary was evaluated");
            let findings: Vec<_> = report
                .findings
                .iter()
                .take(5)
                .map(|f| finding_item(f, &request, cursor.snapshot))
                .collect();
            let mut result = json!({"lab_context":context,"analysis":{"findings":if report.complete{"performed"}else{"operational_limit"},"rules":report.rules},"findings":findings,"findings_more":report.findings.len()>5});
            if request
                .arguments
                .fields
                .iter()
                .any(|field| field == "counts")
            {
                result["counts"] = json!({"total":counts.0,"rpc_registrations":counts.1,"accepted_transactions":counts.2,"selected_faults":counts.3,"boundaries":counts.4});
            }
            if request
                .arguments
                .fields
                .iter()
                .any(|field| field == "traffic_state")
            {
                result["traffic_state"] = json!(if total_rpc == 0 {
                    "no_rpc_traffic"
                } else if counts.0 == 0 {
                    "no_match"
                } else {
                    "evidence_available"
                });
            }
            response.result = result;
        } else {
            let sql = format!(
                "{cte} SELECT q.event_seq FROM q WHERE {} ORDER BY q.event_seq LIMIT ?",
                selected.sql
            );
            let mut values = selected.values;
            values.push(SqlValue::Integer(i64::from(request.arguments.limit) + 1));
            let mut statement = self
                .connection
                .prepare(&sql)
                .map_err(|_| Error::storage())?;
            let ids: Vec<i64> = statement
                .query_map(params_from_iter(values.iter()), |row| row.get(0))
                .map_err(|_| Error::storage())?
                .collect::<rusqlite::Result<_>>()
                .map_err(|_| Error::storage())?;
            drop(statement);
            let mut fields: Vec<String> = query::REQUIRED
                .iter()
                .map(|s| (*s).to_string())
                .chain(request.arguments.fields.iter().cloned())
                .filter(|s| !["event_ref", "kind", "source", "payload"].contains(&s.as_str()))
                .collect();
            fields.sort();
            fields.dedup();
            let mut properties = vec![
                "'event_ref',json_object('lab_id',?1,'event_seq',q.event_seq)".into(),
                "'kind',q.kind".into(),
                "'source',q.source".into(),
            ];
            for field in fields {
                properties.push(if field == "rpc_method" {
                    "'rpc_method',q.rpc_method".into()
                } else {
                    format!("'{field}',json_extract(q.metadata_json,'$.{field}')")
                });
            }
            let sql = format!(
                "{cte} SELECT json_object({}) FROM q WHERE q.event_seq=?2",
                properties.join(",")
            );
            let mut statement = self
                .connection
                .prepare(&sql)
                .map_err(|_| Error::storage())?;
            let mut items = Vec::new();
            'records: for (index, id) in ids
                .iter()
                .take(usize::from(request.arguments.limit))
                .enumerate()
            {
                query::check(cancel, deadline)?;
                let text: String = statement
                    .query_row(rusqlite::params![request.lab_id, id], |row| row.get(0))
                    .map_err(|_| Error::storage())?;
                if text.len() > MAX_PROJECTED_ITEM {
                    return Err(Error::new(
                        "result_too_large",
                        "Projected record exceeds the bounded item envelope",
                    ));
                }
                let mut item: Value = serde_json::from_str(&text).map_err(|_| Error::storage())?;
                if request.arguments.include_payload {
                    let sql = format!(
                        "{cte} SELECT q.payload_bytes,q.payload_encoding,CASE WHEN json_extract(q.metadata_json,'$.payload_status')='available' THEN substr(CAST({payload_value} AS BLOB),1,?2) ELSE NULL END FROM q JOIN events e ON e.event_seq=q.event_seq WHERE q.event_seq=?1",
                        payload_value =
                            query_index::payload_value("e.event_json", "q.payload_path")
                    );
                    let (length, encoding, bytes): (Option<i64>, Option<String>, Option<Vec<u8>>) =
                        self.connection
                            .query_row(
                                &sql,
                                rusqlite::params![id, limits.preview_bytes as i64],
                                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                            )
                            .map_err(|_| Error::storage())?;
                    item["payload"] = if let Some(bytes) = bytes {
                        let valid = std::str::from_utf8(&bytes)
                            .map_or_else(|e| e.valid_up_to(), |s| s.len());
                        json!({"preview":std::str::from_utf8(&bytes[..valid]).map_err(|_|Error::storage())?,"retained_bytes":length,"encoding":encoding,"payload_truncated":length.is_some_and(|n|n>valid as i64)})
                    } else {
                        Value::Null
                    };
                }
                items.push(item);
                response.result = json!({"items":items});
                let more = index + 1 < ids.len();
                response.page.next_cursor = more.then(|| {
                    Cursor {
                        position: *id as u64,
                        ..Cursor {
                            v: cursor.v,
                            snapshot: cursor.snapshot,
                            position: cursor.position,
                            fingerprint: cursor.fingerprint.clone(),
                            state: cursor.state,
                        }
                    }
                    .encode()
                });
                response.view.selection_complete = !more;
                if query::encoded_len(&response, limits.response_bytes).is_err() {
                    response.page.truncated = true;
                    response.page.reason = Some("response_bytes".into());
                    response.view.selection_complete = false;
                    if items.len() == 1 {
                        // The first item can reduce its preview before failing its envelope.
                        response.page.truncated = false;
                        response.page.reason = None;
                        response.view.selection_complete = !more;
                        if fit_preview(&mut items, &mut response, limits.response_bytes) {
                            continue 'records;
                        }
                    }
                    loop {
                        items.pop();
                        let Some(last) = items.last() else {
                            return Err(Error::new(
                                "result_too_large",
                                "Mandatory record does not fit; select fewer fields or a smaller interval",
                            ));
                        };
                        response.page.next_cursor = Some(
                            Cursor {
                                position: last["event_ref"]["event_seq"]
                                    .as_u64()
                                    .ok_or_else(Error::storage)?,
                                v: cursor.v,
                                snapshot: cursor.snapshot,
                                fingerprint: cursor.fingerprint.clone(),
                                state: cursor.state,
                            }
                            .encode(),
                        );
                        response.result = json!({"items":items});
                        if fit_preview(&mut items, &mut response, limits.response_bytes) {
                            break;
                        }
                    }
                    break;
                }
            }
        }
        if analysis.as_ref().is_some_and(|report| !report.complete) {
            response.view.selection_complete = false;
        }
        query::check(cancel, deadline)?;
        if query::encoded_len(&response, limits.response_bytes).is_err() {
            return Err(Error::new(
                "result_too_large",
                "The required envelope does not fit; narrow the selection",
            ));
        }
        transaction.commit().map_err(|_| Error::storage())?;
        Ok(response)
    }
}
const MAX_PROJECTED_ITEM: usize = 256 * 1024;

fn fit_preview(items: &mut [Value], response: &mut Response, budget: usize) -> bool {
    loop {
        response.result = json!({"items":items});
        if query::encoded_len(response, budget).is_ok() {
            return true;
        }
        let Some(item) = items.last_mut() else {
            return false;
        };
        let Some(preview) = item["payload"]["preview"]
            .as_str()
            .filter(|s| !s.is_empty())
        else {
            return false;
        };
        let mut length = preview.len() / 2;
        while !preview.is_char_boundary(length) {
            length -= 1;
        }
        let preview = preview[..length].to_string();
        item["payload"]["preview"] = json!(preview);
        item["payload"]["payload_truncated"] = json!(true);
    }
}

fn finding_item(finding: &crate::findings::Finding, request: &Request, snapshot: u64) -> Value {
    let mut value = serde_json::to_value(finding).expect("typed Finding serializes");
    let mut scope = request.arguments.scope.clone();
    scope.finding_id = Some(finding.finding_id.clone());
    value["evidence_arguments"] = serde_json::to_value(query::Arguments {
        scope,
        snapshot_seq: Some(snapshot),
        ..Default::default()
    })
    .expect("typed evidence selection serializes");
    value
}
