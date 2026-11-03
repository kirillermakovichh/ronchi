//! Streams bounded complete observations under the existing reader snapshot.
use super::JournalReader;
use crate::{
    checks::{
        Definition, Scope, SourceRequest, SourceResponse,
        evaluate::{Result as Evaluation, Selection, Verdict},
        rules::{PredicateResult, Truth},
    },
    query::{self, Error, EventRef},
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
use tokio_util::sync::CancellationToken;
const MAX_EVENTS: usize = 512;
const MAX_BYTES: usize = 4 * 1024 * 1024;
fn incomplete() -> Error {
    Error::new(
        "analysis_incomplete",
        "Check evaluation exceeded its bounded event/byte budget",
    )
}
fn definition(value: Value) -> Result<(String, Definition), Error> {
    let allowed = [
        "check_id",
        "version",
        "source",
        "schema_version",
        "facts",
        "requirements",
        "limits",
        "effective_response_bytes",
    ];
    if value
        .as_object()
        .is_none_or(|fields| fields.keys().any(|name| !allowed.contains(&name.as_str())))
    {
        return Err(Error::new(
            "definition_unavailable",
            "Retained public definition contains unsupported fields",
        ));
    }

    let digest = query::digest(&serde_json::to_vec(&value).map_err(|_| Error::storage())?);
    let mut value = value;
    value
        .as_object_mut()
        .ok_or_else(Error::storage)?
        .remove("effective_response_bytes");
    value["url"] = json!("http://127.0.0.1/");
    let definition: Definition = serde_json::from_value(value).map_err(|_| {
        Error::new(
            "definition_unavailable",
            "Retained Check definition is unsupported",
        )
    })?;
    definition.validate().map_err(|_| {
        Error::new(
            "definition_unavailable",
            "Retained Check definition is invalid or unsupported",
        )
    })?;
    Ok((digest, definition))
}
impl JournalReader {
    pub(crate) fn evaluate_check(
        &self,
        selection: &Selection,
        lab_id: &str,
        snapshot: u64,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<Evaluation, Error> {
        if self.version < 6 {
            return Err(Error::capability(
                "Legacy Journal lacks the indexed Check evidence contract",
            ));
        }
        let text:Option<String>=self.connection.query_row("SELECT json_extract(event_json,'$.data.definition') FROM events JOIN query_events USING(event_seq) WHERE event_seq<=?1 AND kind='check.invocation_started' AND json_extract(event_json,'$.data.definition.check_id')=?2 AND json_extract(event_json,'$.data.definition.version')=?3 ORDER BY event_seq LIMIT 1",params![snapshot as i64,selection.check_id,selection.definition_version],|r|r.get(0)).optional().map_err(|_|Error::storage())?;
        let (digest, definition) = definition(
            serde_json::from_str(&text.ok_or_else(|| {
                Error::new(
                    "definition_unavailable",
                    "No pinned Check definition exists in this snapshot",
                )
            })?)
            .map_err(|_| Error::storage())?,
        )?;
        if selection
            .definition_digest
            .as_ref()
            .is_some_and(|d| d != &digest)
        {
            return Err(Error::new(
                "definition_unavailable",
                "Requested Check digest does not match the retained definition",
            ));
        }
        let mut report = Evaluation::new(
            selection,
            digest.clone(),
            lab_id,
            snapshot,
            definition.source.clone(),
            definition.schema_version,
            &definition.requirements,
        );
        let mut statement=self.connection.prepare("SELECT event_seq,event_json FROM events JOIN query_events USING(event_seq) WHERE event_seq<=?1 AND kind IN ('check.observation','check.invocation_error') AND json_extract(event_json,'$.data.check_id')=?2 AND json_extract(event_json,'$.data.version')=?3 AND json_extract(event_json,'$.data.scope')=?4 ORDER BY event_seq LIMIT 513").map_err(|_|Error::storage())?;
        let mut rows = statement
            .query(params![
                snapshot as i64,
                selection.check_id,
                selection.definition_version,
                serde_json::to_value(&selection.scope)
                    .map_err(|_| Error::storage())?
                    .to_string()
            ])
            .map_err(|_| Error::storage())?;
        let mut count = 0;
        let mut bytes = 0;
        let mut immutable: BTreeMap<(usize, String), (Value, u64)> = BTreeMap::new();
        let mut revisions: BTreeMap<(usize, String, String), (Value, u64)> = BTreeMap::new();
        let mut conflicts = BTreeSet::new();
        let mut conflict_refs: BTreeMap<usize, Vec<EventRef>> = BTreeMap::new();
        let mut state_bytes = 0usize;
        while let Some(row) = rows.next().map_err(|_| Error::storage())? {
            query::check(cancel, deadline)?;
            count += 1;
            if count > MAX_EVENTS {
                return Err(incomplete());
            }
            let seq = row.get::<_, i64>(0).map_err(|_| Error::storage())? as u64;
            let text: String = row.get(1).map_err(|_| Error::storage())?;
            bytes += text.len();
            if bytes > MAX_BYTES {
                return Err(incomplete());
            }
            let event: super::Event = serde_json::from_str(&text).map_err(|_| Error::storage())?;
            let data = &event.data;
            let scope: Scope =
                serde_json::from_value(data["scope"].clone()).map_err(|_| Error::storage())?;
            if scope != selection.scope {
                continue;
            }
            if event.kind == "check.invocation_error" {
                for rule in &mut report.requirements {
                    if rule.verdict == Verdict::Inconclusive {
                        rule.gaps = vec!["source_observation_gap".into()];
                    }
                }
                continue;
            }
            if event.source != "application" || data["retention"] != "complete_validated_selection"
            {
                return Err(Error::new(
                    "invalid_evidence",
                    "Check observation lacks validated application authority",
                ));
            }
            let start = data["cause_refs"][0]["event_seq"]
                .as_u64()
                .filter(|s| *s < seq)
                .ok_or_else(Error::storage)?;
            if data["cause_refs"][0]["lab_id"] != lab_id {
                return Err(Error::new(
                    "invalid_evidence",
                    "Check reference belongs to another Lab",
                ));
            }
            let start_text:String=self.connection.query_row("SELECT event_json FROM events JOIN query_events USING(event_seq) WHERE event_seq=?1 AND kind='check.invocation_started'",[start as i64],|r|r.get(0)).map_err(|_|Error::storage())?;
            bytes += start_text.len();
            if bytes > MAX_BYTES {
                return Err(incomplete());
            }
            let start_event: super::Event =
                serde_json::from_str(&start_text).map_err(|_| Error::storage())?;
            let (observed_digest, _) = definition_from_start(&start_event)?;
            if observed_digest != digest {
                return Err(Error::new(
                    "definition_unavailable",
                    "The same Check version has conflicting retained definitions",
                ));
            }
            let request: SourceRequest =
                serde_json::from_value(start_event.data["source_request"].clone())
                    .map_err(|_| Error::storage())?;
            if request.lab_id != lab_id
                || request.scope != scope
                || request.snapshot_seq >= start
                || request.facts != definition.facts.keys().cloned().collect::<Vec<_>>()
                || request.check_id != selection.check_id
                || request.invocation_id != data["request_id"]
            {
                return Err(Error::new(
                    "invalid_evidence",
                    "Check invocation correlation does not match",
                ));
            }
            let response: SourceResponse =
                serde_json::from_value(data["payload"].clone()).map_err(|_| Error::storage())?;
            let response =
                crate::checks::validate(&definition, &request, response).map_err(|_| {
                    Error::new(
                        "invalid_evidence",
                        "Retained selected facts fail their pinned schema",
                    )
                })?;
            for (index, requirement) in definition.requirements.iter().enumerate() {
                let rule = &mut report.requirements[index];
                let mut consistency_gaps = BTreeSet::new();
                for path in requirement
                    .immutable_paths
                    .iter()
                    .chain(&requirement.revision_paths)
                {
                    if crate::checks::rules::logical(&definition.facts, &response.facts, path)
                        .is_none()
                    {
                        consistency_gaps.insert(format!("unavailable:{path}"));
                    }
                }
                for path in &requirement.immutable_paths {
                    if let Some(value) =
                        crate::checks::rules::logical(&definition.facts, &response.facts, path)
                    {
                        let key = (index, path.clone());
                        if immutable.get(&key).is_some_and(|(old, _)| {
                            !crate::checks::rules::same_value(&definition.facts, path, old, value)
                        }) {
                            conflicts.insert(index);
                            let previous = immutable[&key].1;
                            conflict_refs.entry(index).or_insert_with(|| {
                                vec![
                                    EventRef {
                                        lab_id: lab_id.into(),
                                        event_seq: previous,
                                    },
                                    EventRef {
                                        lab_id: lab_id.into(),
                                        event_seq: seq,
                                    },
                                ]
                            });
                        } else {
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                immutable.entry(key)
                            {
                                state_bytes += path.len() + value.to_string().len();
                                if state_bytes > 256 * 1024 {
                                    return Err(incomplete());
                                }
                                entry.insert((value.clone(), seq));
                            }
                        }
                    }
                }
                if let Some(revision) = &response.revision {
                    for path in &requirement.revision_paths {
                        if let Some(value) =
                            crate::checks::rules::logical(&definition.facts, &response.facts, path)
                        {
                            let key = (index, revision.clone(), path.clone());
                            if revisions.get(&key).is_some_and(|(old, _)| {
                                !crate::checks::rules::same_value(
                                    &definition.facts,
                                    path,
                                    old,
                                    value,
                                )
                            }) {
                                conflicts.insert(index);
                                let previous = revisions[&key].1;
                                conflict_refs.entry(index).or_insert_with(|| {
                                    vec![
                                        EventRef {
                                            lab_id: lab_id.into(),
                                            event_seq: previous,
                                        },
                                        EventRef {
                                            lab_id: lab_id.into(),
                                            event_seq: seq,
                                        },
                                    ]
                                });
                            } else {
                                if let std::collections::btree_map::Entry::Vacant(entry) =
                                    revisions.entry(key)
                                {
                                    state_bytes +=
                                        path.len() + revision.len() + value.to_string().len();
                                    if state_bytes > 256 * 1024 {
                                        return Err(incomplete());
                                    }
                                    entry.insert((value.clone(), seq));
                                }
                            }
                        }
                    }
                }
                let supports = RefCell::new(Vec::new());
                let join_error = RefCell::new(None);
                let join = |signature: &str| {
                    let found=self.connection.query_row("SELECT q.event_seq FROM query_signatures s JOIN query_events q ON q.event_seq=s.event_seq JOIN events e ON e.event_seq=q.event_seq WHERE s.signature=?1 AND q.event_seq<=?2 AND q.kind='transaction.transition' AND json_extract(e.event_json,'$.source')='rpc_simulator' AND json_extract(e.event_json,'$.execution_mode')='simulator' AND json_extract(e.event_json,'$.data.stage')='finalized' ORDER BY q.event_seq LIMIT 1",params![signature,snapshot as i64],|r|r.get::<_,i64>(0)).optional();
                    let found = match found {
                        Ok(value) => value,
                        Err(_) => {
                            *join_error.borrow_mut() = Some(Error::storage());
                            None
                        }
                    };
                    if let Some(event_seq) = found {
                        supports.borrow_mut().push(EventRef {
                            lab_id: lab_id.into(),
                            event_seq: event_seq as u64,
                        });
                        PredicateResult {
                            truth: Truth::True,
                            gaps: BTreeSet::new(),
                        }
                    } else {
                        PredicateResult {
                            truth: Truth::Unknown,
                            gaps: BTreeSet::from(["missing_original_finality".into()]),
                        }
                    }
                };
                let observed_at_ms = time::OffsetDateTime::parse(
                    &response.observed_at,
                    &time::format_description::well_known::Rfc3339,
                )
                .ok()
                .and_then(|at| i64::try_from(at.unix_timestamp_nanos() / 1_000_000).ok());
                let context = crate::checks::rules::Context {
                    finality: &join,
                    scope: Some(&selection.scope),
                    observed_at_ms,
                };
                let pass =
                    requirement
                        .pass
                        .evaluate_in(&definition.facts, &response.facts, &context);
                let fail =
                    requirement
                        .fail
                        .evaluate_in(&definition.facts, &response.facts, &context);
                let complete =
                    requirement
                        .complete
                        .evaluate_in(&definition.facts, &response.facts, &context);
                if let Some(error) = join_error.into_inner() {
                    return Err(error);
                }
                query::check(cancel, deadline)?;
                let previous = rule.verdict;
                if conflicts.contains(&index) {
                    rule.verdict = Verdict::Inconclusive;
                    rule.reason =
                        "Contradictory source facts under declared consistency paths".into();
                    rule.gaps = vec!["source_contradiction".into()];
                } else if fail.truth == Truth::True {
                    rule.verdict = Verdict::Fail;
                    rule.reason =
                        "Declared failure predicate has positive supporting evidence".into();
                    rule.gaps = fail.gaps.union(&consistency_gaps).cloned().collect();
                } else if rule.verdict != Verdict::Fail
                    && pass.truth == Truth::True
                    && complete.truth == Truth::True
                    && fail.truth == Truth::False
                    && fail.gaps.is_empty()
                    && pass.gaps.is_empty()
                    && complete.gaps.is_empty()
                    && consistency_gaps.is_empty()
                {
                    rule.verdict = Verdict::Pass;
                    rule.reason =
                        "Positive requirement and declared complete interval are supported".into();
                    rule.gaps.clear();
                } else if rule.verdict == Verdict::Inconclusive {
                    rule.gaps = pass
                        .gaps
                        .union(&complete.gaps)
                        .cloned()
                        .chain(fail.gaps)
                        .chain(consistency_gaps)
                        .collect();
                    if rule.gaps.is_empty() {
                        rule.gaps
                            .push("missing_positive_or_complete_evidence".into());
                    }
                }
                if let Some(references) = conflict_refs.get(&index) {
                    rule.supporting = references.clone();
                } else if rule.verdict != previous
                    || rule.supporting.is_empty()
                    || (rule.verdict == Verdict::Fail && fail.truth == Truth::True)
                {
                    rule.supporting = vec![
                        EventRef {
                            lab_id: lab_id.into(),
                            event_seq: start,
                        },
                        EventRef {
                            lab_id: lab_id.into(),
                            event_seq: seq,
                        },
                    ];
                    for reference in supports.into_inner() {
                        if !rule.supporting.contains(&reference) {
                            rule.supporting.push(reference);
                        }
                    }
                    if rule.supporting.len() > 32 {
                        return Err(incomplete());
                    }
                }
            }
        }
        report.aggregate();
        Ok(report)
    }
}
fn definition_from_start(event: &super::Event) -> Result<(String, Definition), Error> {
    if event.source != "ronchi" {
        return Err(Error::storage());
    }
    definition(event.data["definition"].clone())
}
