//! Materialized navigation/projection in the existing Journal transaction.
use rusqlite::{Connection, functions::FunctionFlags};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(super) fn functions(connection: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8
        | FunctionFlags::SQLITE_DETERMINISTIC
        | FunctionFlags::SQLITE_INNOCUOUS;
    connection.create_scalar_function("ronchi_seconds", 1, flags, |ctx| {
        let value: String = ctx.get(0)?;
        Ok(OffsetDateTime::parse(&value, &Rfc3339)
            .ok()
            .map(|time| time.unix_timestamp()))
    })?;
    connection.create_scalar_function("ronchi_nanos", 1, flags, |ctx| {
        let value: String = ctx.get(0)?;
        Ok(OffsetDateTime::parse(&value, &Rfc3339)
            .ok()
            .map(|time| time.nanosecond()))
    })
}
pub(super) fn payload_value(event: &str, path: &str) -> String {
    format!(
        "(CASE WHEN {path} IS NULL THEN NULL WHEN json_type({event},{path})='text' THEN json_extract({event},{path}) ELSE {event} -> {path} END)"
    )
}
/// The legacy adapter also projects in SQL; no original event body is deserialized by the reader.
pub(super) fn projection(input: &str, resolve: bool) -> String {
    let tx_ref = if resolve {
        "(SELECT json_extract(record_json,'$.admission_request') FROM transactions WHERE signature=direct_sig)"
    } else {
        "NULL"
    };
    let prior = |field: &str| {
        if resolve && field == "rpc_method" {
            "(SELECT rpc_method FROM query_events WHERE request_id=req_id AND kind='rpc.registered' ORDER BY event_seq LIMIT 1)".into()
        } else if resolve {
            format!(
                "(SELECT json_extract(metadata_json,'$.{field}') FROM query_events WHERE request_id=req_id AND kind='rpc.registered' ORDER BY event_seq LIMIT 1)"
            )
        } else {
            "NULL".into()
        }
    };
    format!(
        r#"WITH raw AS ({input}),
    basic AS (SELECT event_seq,event_json,json_extract(event_json,'$.kind') kind,json_extract(event_json,'$.source') source,
      coalesce(json_extract(event_json,'$.data.signature'),json_extract(event_json,'$.data.transaction_signature')) direct_sig FROM raw),
    refs AS (SELECT *,coalesce(json_extract(event_json,'$.data.request'),json_extract(event_json,'$.data.admission_request'),
      CASE WHEN json_type(event_json,'$.data.request_id')='text' THEN json_extract(event_json,'$.data') END,{tx_ref},'{{}}') request_json FROM basic),
    ids AS (SELECT *,json_extract(request_json,'$.request_id') req_id,
      coalesce(json_extract(event_json,'$.data.policy'),json_extract(event_json,'$.data.binding.policy'),json_extract(request_json,'$.binding.policy')) policy,
      CASE WHEN json_type(event_json,'$.data.wire_base64')='text' THEN '$.data.wire_base64'
           WHEN json_type(event_json,'$.data.payload') IS NOT NULL THEN '$.data.payload'
           WHEN json_type(event_json,'$.data.result.transaction') IS NOT NULL THEN '$.data.result.transaction'
           WHEN json_type(event_json,'$.data.source_result.transaction') IS NOT NULL THEN '$.data.source_result.transaction' END payload_path FROM refs),
    methods AS (SELECT *,coalesce(json_extract(event_json,'$.data.method'),{prior_method},
      CASE WHEN kind IN ('transaction.accepted','transaction.submission','transaction.transition','fault.response_loss_selected','fault.withholding_started') THEN 'sendTransaction' END) method FROM ids),
    signatures AS (SELECT *,coalesce(direct_sig,CASE WHEN method='sendTransaction' AND json_type(event_json,'$.data.result')='text' THEN json_extract(event_json,'$.data.result') END) sig,
      coalesce(CASE WHEN json_type(event_json,'$.data.signatures')='array' THEN json_extract(event_json,'$.data.signatures') END,{prior_signatures},'[]') sigs FROM methods)
    SELECT event_seq,kind,source,ronchi_seconds(json_extract(event_json,'$.recorded_at')) recorded_sec,
      ronchi_nanos(json_extract(event_json,'$.recorded_at')) recorded_nano,req_id request_id,method rpc_method,
      coalesce(json_extract(policy,'$.revision'),{prior_activation}) activation_id,
      json_object(
        'schema_version',json_extract(event_json,'$.schema_version'),'recorded_at',json_extract(event_json,'$.recorded_at'),
        'elapsed_ns',json_extract(event_json,'$.elapsed_ns'),'execution_mode',json_extract(event_json,'$.execution_mode'),
        'request_id',req_id,'batch_id',json_extract(request_json,'$.batch_id'),'registration_seq',json_extract(request_json,'$.registration_seq'),
        'transaction_signature',sig,'transaction_signatures',json(CASE WHEN sig IS NOT NULL THEN json_array(sig) ELSE sigs END),
        'policy_ref',json(coalesce(policy,{prior_policy},'null')),'cause_refs',json(coalesce(json_extract(event_json,'$.data.cause_refs'),'[]')),
        'simulated_time_ns',json_extract(event_json,'$.data.simulated_time_ns'),'slot',coalesce(json_extract(event_json,'$.data.slot'),json_extract(event_json,'$.data.processed_slot')),
        'block_height',coalesce(json_extract(event_json,'$.data.block_height'),json_extract(event_json,'$.data.processed_height')),
        'payload_status',CASE WHEN json_extract(event_json,'$.data.payload_status') IN ('redacted','omitted_by_policy','lost') THEN json_extract(event_json,'$.data.payload_status') WHEN json_extract(event_json,'$.data.payload_redacted')=1 THEN 'redacted' WHEN payload_path IS NULL THEN 'absent' ELSE 'available' END,
        'data',json_extract(json_remove(event_json,'$.data.wire_base64','$.data.payload','$.data.raw_body','$.data.body','$.data.attachment',
          '$.data.result.transaction','$.data.source_result.transaction','$.data.request','$.data.admission_request','$.data.signature','$.data.signatures','$.data.cause_refs'),'$.data')) metadata_json,
      payload_path,length(CAST({payload_value} AS BLOB)) payload_bytes,
      CASE WHEN payload_path='$.data.wire_base64' THEN 'base64' WHEN json_type(event_json,payload_path)!='text' THEN 'json' ELSE 'text' END payload_encoding
    FROM signatures"#,
        payload_value = payload_value("event_json", "payload_path"),
        prior_method = prior("rpc_method"),
        prior_signatures = prior("transaction_signatures"),
        prior_activation = prior("policy_ref.revision"),
        prior_policy = prior("policy_ref")
    )
}
pub(super) fn schema() -> String {
    let select = projection(
        "SELECT event_seq,event_json FROM events WHERE event_seq=NEW.event_seq",
        true,
    );
    format!(
        r#"
CREATE TABLE query_events (
 event_seq INTEGER PRIMARY KEY REFERENCES events(event_seq),kind TEXT NOT NULL,source TEXT NOT NULL,
 recorded_sec INTEGER,recorded_nano INTEGER,request_id TEXT,rpc_method TEXT,activation_id INTEGER,
 metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json)),payload_path TEXT,payload_bytes INTEGER,payload_encoding TEXT
) STRICT;
CREATE INDEX query_kind ON query_events(kind,event_seq);
CREATE INDEX query_method ON query_events(rpc_method,event_seq);
CREATE INDEX query_request ON query_events(request_id,event_seq);
CREATE INDEX query_activation ON query_events(activation_id,event_seq);
CREATE INDEX query_time ON query_events(recorded_sec,recorded_nano,event_seq);
CREATE TABLE query_signatures (
 event_seq INTEGER NOT NULL REFERENCES query_events(event_seq),signature TEXT NOT NULL,PRIMARY KEY(signature,event_seq)
) STRICT;
CREATE INDEX query_signature_event ON query_signatures(event_seq,signature);
CREATE TRIGGER query_projection AFTER INSERT ON events BEGIN
 INSERT INTO query_events {select};
 INSERT INTO query_signatures SELECT DISTINCT NEW.event_seq,value FROM json_each((SELECT json_extract(metadata_json,'$.transaction_signatures') FROM query_events WHERE event_seq=NEW.event_seq)) WHERE type='text';
END;
PRAGMA user_version=6;
"#
    )
}
