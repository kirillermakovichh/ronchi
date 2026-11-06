//! The window covers only retained registrations inside the caller's exact selection.
use super::*;
use crate::findings::ObservationWindow;
impl JournalReader {
    pub(super) fn response_loss_window(
        &self,
        selected: &str,
        values: &[rusqlite::types::Value],
        evidence: &Evidence,
        analysis_complete: bool,
    ) -> Result<ObservationWindow, Error> {
        let hold = evidence
            .withholding
            .as_ref()
            .expect("validated Finding support");
        let acceptance = evidence
            .acceptance
            .as_ref()
            .expect("validated Finding support");
        let n = values.len();
        let signature_join = if self.version >= 6 {
            format!(
                "event_seq IN (SELECT event_seq FROM query_signatures WHERE signature=?{})",
                n + 3
            )
        } else {
            format!(
                "EXISTS(SELECT 1 FROM json_each(json_extract(metadata_json,'$.transaction_signatures')) WHERE value=?{})",
                n + 3
            )
        };
        let sql = format!("{selected} SELECT
            coalesce(sum(rpc_method IN ('getSignatureStatuses','getTransaction')),0), count(*),
            (SELECT count(*) FROM selected gaps WHERE gaps.kind='rpc.registered' AND gaps.event_seq>?{}
             AND (gaps.source<>'ronchi' OR json_extract(gaps.metadata_json,'$.schema_version') IS NOT 1
             OR json_extract(gaps.metadata_json,'$.execution_mode') IS NOT 'surfpool'
             OR json_extract(gaps.metadata_json,'$.registration_seq') IS NULL))
            FROM selected WHERE kind='rpc.registered' AND source='ronchi' AND json_extract(metadata_json,'$.schema_version')=1 AND json_extract(metadata_json,'$.execution_mode')='surfpool' AND event_seq>?{} AND json_extract(metadata_json,'$.registration_seq')>?{} AND {signature_join}", n+1,n+1,n+2);
        let mut bound = values.to_vec();
        bound.extend([
            rusqlite::types::Value::Integer(hold.event_ref.event_seq as i64),
            rusqlite::types::Value::Integer(acceptance.registration_seq as i64),
            acceptance.signature.clone().into(),
        ]);
        let (lookups, calls, unsupported): (i64, i64, i64) = self
            .connection
            .query_row(&sql, rusqlite::params_from_iter(bound), |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|_| Error::storage())?;
        let sql = format!(
            "{selected} SELECT event_seq,metadata_json FROM selected ORDER BY event_seq DESC LIMIT 1"
        );
        let (end, end_json): (u64, String) = self
            .connection
            .query_row(&sql, rusqlite::params_from_iter(values), |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get(1)?))
            })
            .map_err(|_| Error::storage())?;
        let end_json: Value = serde_json::from_str(&end_json).map_err(|_| Error::storage())?;
        let sql = format!(
            "{selected} SELECT metadata_json FROM selected WHERE event_seq=?{}",
            n + 1
        );
        let mut bound = values.to_vec();
        bound.push((hold.event_ref.event_seq as i64).into());
        let start: String = self
            .connection
            .query_row(&sql, rusqlite::params_from_iter(bound), |r| r.get(0))
            .map_err(|_| Error::storage())?;
        let start: Value = serde_json::from_str(&start).map_err(|_| Error::storage())?;
        Ok(ObservationWindow {
            from_seq:hold.event_ref.event_seq,to_seq:end,
            from_recorded_at:start["recorded_at"].as_str().unwrap_or("").into(),
            to_recorded_at:end_json["recorded_at"].as_str().unwrap_or("").into(),
            status_lookup_count:lookups as u64,later_rpc_count:calls as u64,
            complete_for_selected_records:analysis_complete && self.version>=6 && unsupported==0,
            coverage:"Retained same-signature RPC registrations after withholding, inside the explicit filters and pinned snapshot only. Other RPC paths and client outcomes are unobserved. The window ends at the last selected committed record, not at wall-clock now.".into(),
        })
    }
}
