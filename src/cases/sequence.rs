//! Activation-scoped, registration-ordered schedules over maintained primitives.
use super::{Arm, Binding, custom::Plan};
use crate::rpc::RpcError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Mutex};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Selection {
    pub key: String,
    pub signature: Option<String>,
    pub step: String,
    pub cycle: u64,
    pub primitive: String,
    pub arguments: Value,
}
#[derive(Debug)]
pub(crate) struct Runtime {
    pub plan: Plan,
    pub artifact: Value,
    cache: Mutex<Cache>,
}
#[derive(Clone, Debug, Default)]
struct Cache {
    keys: BTreeMap<String, Cursor>,
    snapshots: BTreeMap<String, Value>,
    snapshot_bytes: usize,
    lifecycles: BTreeMap<String, Value>,
}
#[derive(Clone, Debug)]
struct Cursor {
    step: usize,
    cycle: u64,
    entered_real: u64,
    entered_sim: u64,
    matches: u64,
}
impl Runtime {
    pub fn new(plan: Plan) -> Self {
        let artifact = serde_json::to_value(&plan.artifact).expect("validated artifact serializes");
        Self {
            plan,
            artifact,
            cache: Mutex::new(Cache::default()),
        }
    }
    pub fn provenance(&self) -> Value {
        json!({"origin":"custom","id":self.artifact["id"],"revision":self.artifact["revision"],"digest":self.plan.digest,"primitives":self.plan.resolved_primitives,"finding_templates":self.plan.resolved_templates,"core_compatibility":"ronchi.case/v1","coverage":"author_declared","guidance":self.artifact.get("guidance"),"findings":self.artifact.get("findings")})
    }
    pub fn select_many(
        &self,
        signatures: &[String],
        real: u64,
        sim: u64,
        method: &str,
        responds: bool,
        admit: impl FnOnce(&[Selection]) -> Result<(), RpcError>,
    ) -> Result<Vec<Selection>, RpcError> {
        let mut current = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mut pending = Cache {
            keys: current.keys.clone(),
            ..Default::default()
        };
        let mut selected = vec![];
        if self.artifact["state"]["key"] == "case" {
            if let Some(selection) = self.select_in(
                &mut pending,
                signatures.first().map(String::as_str),
                real,
                sim,
                method,
                responds,
            )? {
                if signatures.is_empty() {
                    selected.push(selection);
                } else {
                    for signature in signatures {
                        let mut entry = selection.clone();
                        entry.signature = Some(signature.clone());
                        selected.push(entry);
                    }
                }
            }
        } else {
            for signature in signatures {
                if let Some(selection) =
                    self.select_in(&mut pending, Some(signature), real, sim, method, responds)?
                {
                    selected.push(selection);
                }
            }
        }
        admit(&selected)?;
        current.keys = pending.keys;
        Ok(selected)
    }
    fn select_in(
        &self,
        cache: &mut Cache,
        signature: Option<&str>,
        real: u64,
        sim: u64,
        method: &str,
        responds: bool,
    ) -> Result<Option<Selection>, RpcError> {
        let key = if self.artifact["state"]["key"] == "case" {
            "case"
        } else {
            signature
                .ok_or_else(|| RpcError::params("Custom Case requires a transaction signature"))?
        };
        if !cache.keys.contains_key(key) {
            let first = &self.artifact["sequence"][0];
            let primitive = first["using"].as_str().expect("primitive");
            if !stage_matches(primitive, method)
                || (!responds
                    && matches!(
                        primitive,
                        "rpc.timeout-before-dispatch@1" | "rpc.withhold-submission-response@1"
                    ))
            {
                return Ok(None);
            }
            if cache.keys.len()
                >= self.artifact["state"]["max_keys"].as_u64().unwrap_or(1024) as usize
            {
                return Err(capacity());
            }
            cache.keys.insert(
                key.into(),
                Cursor {
                    step: 0,
                    cycle: 0,
                    entered_real: real,
                    entered_sim: sim,
                    matches: 0,
                },
            );
        }
        let steps = self.artifact["sequence"]
            .as_array()
            .expect("validated sequence");
        let cursor = cache.keys.get_mut(key).expect("inserted cursor");
        // Each time transition starts the next step at this registration, not in the past.
        if cursor.step < steps.len() {
            let until = &steps[cursor.step]["until"];
            let elapsed = if until["clock"] == "real" {
                real.saturating_sub(cursor.entered_real)
            } else {
                sim.saturating_sub(cursor.entered_sim)
            };
            if until["elapsed_ms"]
                .as_u64()
                .is_some_and(|ms| elapsed >= ms * 1_000_000)
            {
                self.advance(cursor, steps, real, sim);
            }
        }
        if cursor.step >= steps.len() {
            return Ok(None);
        }
        let step = &steps[cursor.step];
        let primitive = step["using"].as_str().expect("primitive");
        if !stage_matches(primitive, method)
            || (!responds
                && matches!(
                    primitive,
                    "rpc.timeout-before-dispatch@1" | "rpc.withhold-submission-response@1"
                ))
        {
            return Ok(None);
        }
        let selection = Selection {
            key: key.into(),
            signature: signature.map(str::to_owned),
            step: step["id"].as_str().expect("step id").into(),
            cycle: cursor.cycle,
            primitive: step["using"].as_str().expect("primitive").into(),
            arguments: step.get("with").cloned().unwrap_or_else(|| json!({})),
        };
        cursor.matches = cursor.matches.saturating_add(1);
        if step["until"]["matches"]
            .as_u64()
            .is_some_and(|n| cursor.matches >= n)
        {
            self.advance(cursor, steps, real, sim);
        }
        Ok(Some(selection))
    }
    fn advance(&self, cursor: &mut Cursor, steps: &[Value], real: u64, sim: u64) {
        let next = steps[cursor.step]["next"].as_str().unwrap_or("done");
        if next == "done" {
            cursor.step = steps.len();
        } else if let Some(index) = steps.iter().position(|s| s["id"] == next) {
            if index <= cursor.step {
                let repeat = &self.artifact["repeat"];
                if repeat["while_active"] == true
                    || repeat["cycles"]
                        .as_u64()
                        .is_some_and(|n| cursor.cycle + 1 < n)
                {
                    cursor.cycle += 1;
                    cursor.step = index;
                } else {
                    cursor.step = steps.len();
                }
            } else {
                cursor.step = index;
            }
        } else {
            cursor.step = steps.len();
        }
        cursor.matches = 0;
        cursor.entered_real = real;
        cursor.entered_sim = sim;
    }
    pub fn lifecycle(
        &self,
        selection: &Selection,
        actual: &Value,
    ) -> Result<Option<Value>, RpcError> {
        let key = format!(
            "{}:{}:{}:{}",
            selection.key,
            selection.signature.as_deref().unwrap_or("case"),
            selection.cycle,
            selection.step
        );
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(value) = cache.lifecycles.get(&key) {
            return Ok(Some(value.clone()));
        }
        if !matches!(
            actual["confirmationStatus"].as_str(),
            Some("processed" | "confirmed" | "finalized")
        ) {
            return Ok(None);
        }
        let bytes = actual.to_string().len();
        if bytes > 16 * 1024 || cache.snapshot_bytes.saturating_add(bytes) > 4 * 1024 * 1024 {
            return Err(capacity());
        }
        cache.snapshot_bytes += bytes;
        cache.lifecycles.insert(key, actual.clone());
        Ok(Some(actual.clone()))
    }
    pub fn snapshot(
        &self,
        selection: &Selection,
        query: &str,
        value: &Value,
    ) -> Result<Value, RpcError> {
        let key = format!(
            "{}:{}:{}:{}:{}",
            selection.key,
            selection.signature.as_deref().unwrap_or("case"),
            selection.cycle,
            selection.step,
            query
        );
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(snapshot) = cache.snapshots.get(&key) {
            return Ok(snapshot.clone());
        }
        let size = value.to_string().len();
        if size > 16 * 1024 || cache.snapshot_bytes.saturating_add(size) > 4 * 1024 * 1024 {
            return Err(capacity());
        }
        cache.snapshot_bytes += size;
        cache.snapshots.insert(key, value.clone());
        Ok(value.clone())
    }
}
pub(crate) fn capacity() -> RpcError {
    RpcError {
        code: -32000,
        message: "Custom Case transient state capacity is exhausted".into(),
        data: Some(json!({"code":"case_capacity_exceeded"})),
    }
}
pub(crate) fn project(binding: &Binding, selections: &[Selection], method: &str) -> Binding {
    let mut binding = binding.clone();
    let Some(policy) = &mut binding.policy else {
        return binding;
    };
    let Some(selection) = selections
        .iter()
        .find(|s| s.primitive == "rpc.timeout-before-dispatch@1")
        .or_else(|| selections.first())
    else {
        policy.settings = Arm {
            case_id: "custom.observe".into(),
            ..Default::default()
        };
        return binding;
    };
    let mut settings = Arm {
        case_id: match selection.primitive.as_str() {
            "rpc.timeout-before-dispatch@1" => "provider-timeout-burst",
            "rpc.withhold-submission-response@1" => "tx-response-lost",
            "sim.confirmation-delay@1" => "tx-confirmation-stalls",
            "rpc.blockhash-reuse@1" => "tx-blockhash-expired",
            _ => "custom.observe",
        }
        .into(),
        ..Default::default()
    };
    settings.hold_ms = selection.arguments["hold_real_ms"]
        .as_u64()
        .unwrap_or(20_000);
    if selection.primitive == "rpc.timeout-before-dispatch@1" {
        settings.timeout = Some(super::timeout::Settings {
            methods: vec![method.into()],
            outage_ms: 120_000,
            recovery_ms: 1,
        });
    }
    if selection.primitive == "sim.confirmation-delay@1" {
        settings.confirmation = Some(super::confirmation::Delay {
            processing_ms: selection.arguments["processing_ms"]
                .as_u64()
                .unwrap_or(2000),
            confirmation_interval_ms: selection.arguments["confirmation_interval_ms"]
                .as_u64()
                .unwrap_or(2000),
            extra_delay_ms: selection.arguments["extra_delay_ms"]
                .as_u64()
                .unwrap_or(30000),
            hold_at: if selection.arguments["hold_at"] == "confirmed" {
                super::confirmation::HoldAt::Confirmed
            } else {
                super::confirmation::HoldAt::Processed
            },
        });
    }
    if selection.primitive == "rpc.blockhash-reuse@1" {
        // The source query's selected commitment is filled by the registration owner.
        settings.blockhash = policy.settings.blockhash.clone();
    }
    policy.settings = settings;
    binding
}

pub(crate) fn stage_matches(primitive: &str, method: &str) -> bool {
    match primitive {
        "rpc.withhold-submission-response@1" | "sim.confirmation-delay@1" => {
            method == "sendTransaction"
        }
        "rpc.lookup-null@1" | "rpc.lookup-snapshot@1" => {
            matches!(method, "getSignatureStatuses" | "getTransaction")
        }
        "rpc.blockhash-reuse@1" => method == "getLatestBlockhash",
        _ => true,
    }
}
