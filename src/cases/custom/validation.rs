use super::*;
use serde_json::{Map, json};
use std::collections::{BTreeMap, BTreeSet};
use yaml_rust2::{
    Yaml,
    parser::{Event, Parser},
    scanner::{Scanner, TScalarStyle, TokenType},
};

const PRIMITIVES: [&str; 7] = [
    "rpc.timeout-before-dispatch@1",
    "rpc.withhold-submission-response@1",
    "rpc.lookup-null@1",
    "rpc.lookup-snapshot@1",
    "sim.confirmation-delay@1",
    "rpc.blockhash-reuse@1",
    "observe.rpc@1",
];
const TEMPLATE: &str = "lookup-hidden-then-queried@1";
const SIGNED_METHODS: [&str; 3] = ["sendTransaction", "getSignatureStatuses", "getTransaction"];
const COMMITMENT_METHODS: [&str; 15] = [
    "getSlot",
    "getBlockHeight",
    "getLatestBlockhash",
    "isBlockhashValid",
    "getTransaction",
    "getFeeForMessage",
    "sendTransaction",
    "getBalance",
    "getAccountInfo",
    "getMultipleAccounts",
    "getTokenAccountBalance",
    "getTokenAccountsByOwner",
    "getMinimumBalanceForRentExemption",
    "simulateTransaction",
    "requestAirdrop",
];
fn error(category: &str, path: &str, message: &str) -> ValidationError {
    ValidationError::new(category, path, message)
}
fn child(path: &str, key: &str) -> String {
    if path == "$" {
        key.into()
    } else {
        format!("{path}.{key}")
    }
}
fn next(parser: &mut Parser<std::str::Chars<'_>>, path: &str) -> Result<Event, ValidationError> {
    parser
        .next_token()
        .map(|(event, _)| event)
        .map_err(|_| error("invalid_yaml", path, "Malformed YAML document"))
}
fn scalar(text: String, style: TScalarStyle, path: &str) -> Result<Value, ValidationError> {
    if style != TScalarStyle::Plain {
        return Ok(Value::String(text));
    }
    let bytes = text.as_bytes();
    if bytes.len() >= 10
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
        && (bytes.len() == 10 || matches!(bytes[10], b'T' | b't' | b' '))
    {
        return Err(error(
            "invalid_yaml",
            path,
            "Implicit timestamps are not supported; quote string values",
        ));
    }
    if matches!(text.as_str(), "Null" | "NULL") {
        return Err(error(
            "invalid_yaml",
            path,
            "Null scalar values are not supported",
        ));
    }
    let decimal = text.strip_prefix(['+', '-']).unwrap_or(&text);
    let integer = if !decimal.is_empty() && decimal.bytes().all(|b| b.is_ascii_digit()) {
        Some(text.parse::<i64>())
    } else if let Some(digits) = text.strip_prefix("0x") {
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| i64::from_str_radix(digits, 16))
    } else if let Some(digits) = text.strip_prefix("0o") {
        (!digits.is_empty() && digits.bytes().all(|b| matches!(b, b'0'..=b'7')))
            .then(|| i64::from_str_radix(digits, 8))
    } else {
        None
    };
    if let Some(integer) = integer {
        return integer.map(|number| json!(number)).map_err(|_| {
            error(
                "limit_exceeded",
                path,
                "Integer exceeds the bounded input range",
            )
        });
    }
    match Yaml::from_str(&text) {
        // The loader accepts extra Rust integer syntax, such as 0x-1 and +-1.
        // Core YAML integer spellings were classified above.
        Yaml::Integer(_) => Ok(Value::String(text)),
        Yaml::Boolean(value) => Ok(json!(value)),
        Yaml::String(value) => Ok(Value::String(value)),
        Yaml::Real(_) => Err(error(
            "invalid_yaml",
            path,
            "Floating-point scalar values are not supported",
        )),
        _ => Err(error(
            "invalid_yaml",
            path,
            "Null and non-core scalar values are not supported",
        )),
    }
}
fn node(
    parser: &mut Parser<std::str::Chars<'_>>,
    event: Event,
    path: &str,
    depth: usize,
) -> Result<Value, ValidationError> {
    if depth > 32 {
        return Err(error(
            "limit_exceeded",
            path,
            "YAML nesting exceeds 32 levels",
        ));
    }
    match event {
        Event::Scalar(value, style, 0, None) => scalar(value, style, path),
        Event::SequenceStart(0, None) => {
            let mut values = Vec::new();
            loop {
                let event = next(parser, path)?;
                if event == Event::SequenceEnd {
                    break;
                }
                let at = format!("{path}[{}]", values.len());
                values.push(node(parser, event, &at, depth + 1)?);
            }
            Ok(Value::Array(values))
        }
        Event::MappingStart(0, None) => {
            let mut values = Map::new();
            loop {
                let event = next(parser, path)?;
                if event == Event::MappingEnd {
                    break;
                }
                let Value::String(key) = node(parser, event, path, depth + 1)? else {
                    return Err(error("invalid_yaml", path, "Mapping keys must be strings"));
                };
                let at = child(path, &key);
                if key == "<<" {
                    return Err(error(
                        "invalid_yaml",
                        &at,
                        "YAML merge keys are not supported",
                    ));
                }
                if values.contains_key(&key) {
                    return Err(error("invalid_yaml", &at, "Duplicate YAML mapping key"));
                }
                let event = next(parser, &at)?;
                values.insert(key, node(parser, event, &at, depth + 1)?);
            }
            Ok(Value::Object(values))
        }
        Event::Alias(_)
        | Event::Scalar(..)
        | Event::SequenceStart(..)
        | Event::MappingStart(..) => Err(error(
            "invalid_yaml",
            path,
            "YAML tags, anchors, and aliases are not supported",
        )),
        _ => Err(error(
            "invalid_yaml",
            path,
            "Expected a scalar, array, or mapping",
        )),
    }
}
fn parse(yaml: &str) -> Result<Value, ValidationError> {
    if yaml.len() > MAX_ARTIFACT_BYTES {
        return Err(error(
            "limit_exceeded",
            "$",
            "Custom Case artifact exceeds 64 KiB",
        ));
    }
    for token in Scanner::new(yaml.chars()) {
        match token.1 {
            TokenType::VersionDirective(major, minor) if (major, minor) != (1, 2) => {
                return Err(error(
                    "invalid_yaml",
                    "$",
                    "Only YAML 1.2 version directives are supported",
                ));
            }
            TokenType::TagDirective(..) => {
                return Err(error(
                    "invalid_yaml",
                    "$",
                    "YAML tag directives are not supported",
                ));
            }
            _ => {}
        }
    }
    let mut parser = Parser::new_from_str(yaml);
    if next(&mut parser, "$")? != Event::StreamStart
        || next(&mut parser, "$")? != Event::DocumentStart
    {
        return Err(error("invalid_yaml", "$", "Expected one YAML document"));
    }
    let first = next(&mut parser, "$")?;
    let value = node(&mut parser, first, "$", 0)?;
    if next(&mut parser, "$")? != Event::DocumentEnd || next(&mut parser, "$")? != Event::StreamEnd
    {
        return Err(error(
            "invalid_yaml",
            "$",
            "Exactly one YAML document is supported",
        ));
    }
    Ok(value)
}
fn fields<'a>(
    value: &'a Value,
    path: &str,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, ValidationError> {
    let object = value
        .as_object()
        .ok_or_else(|| error("invalid_yaml", path, "Expected a mapping"))?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(error(
                "unknown_field",
                &child(path, key),
                "Unknown Custom Case field",
            ));
        }
    }
    Ok(object)
}
fn list<'a>(
    value: Option<&'a Value>,
    path: &str,
    max: usize,
) -> Result<Option<&'a Vec<Value>>, ValidationError> {
    value
        .map(|value| {
            let list = value
                .as_array()
                .ok_or_else(|| error("invalid_yaml", path, "Expected an array"))?;
            if list.len() > max {
                return Err(error(
                    "limit_exceeded",
                    path,
                    "Array exceeds its item limit",
                ));
            }
            Ok(list)
        })
        .transpose()
}
fn bounded(value: Option<&Value>, path: &str, min: u64, max: u64) -> Result<(), ValidationError> {
    if let Some(value) = value
        && value.as_u64().is_none_or(|n| n < min || n > max)
    {
        return Err(error(
            "limit_exceeded",
            path,
            "Expected an integer within the declared bounds",
        ));
    }
    Ok(())
}
fn shape(value: &Value) -> Result<(), ValidationError> {
    let root = fields(
        value,
        "$",
        &[
            "schema",
            "id",
            "revision",
            "description",
            "requires",
            "modes",
            "filter",
            "state",
            "sequence",
            "findings",
            "guidance",
            "repeat",
        ],
    )?;
    if root.get("schema").and_then(Value::as_str) != Some("ronchi.case/v1") {
        return Err(error(
            "unsupported_schema",
            "schema",
            "Expected schema ronchi.case/v1",
        ));
    }
    let required = fields(
        root.get("requires").unwrap_or(&Value::Null),
        "requires",
        &["primitives", "finding_templates"],
    )?;
    list(
        required.get("primitives"),
        "requires.primitives",
        MAX_ARTIFACT_BYTES,
    )?;
    list(
        required.get("finding_templates"),
        "requires.finding_templates",
        MAX_ARTIFACT_BYTES,
    )?;
    let modes = list(root.get("modes"), "modes", 2)?.ok_or_else(|| {
        error(
            "unsupported_mode",
            "modes",
            "Declared execution modes are required",
        )
    })?;
    if modes
        .iter()
        .any(|mode| !matches!(mode.as_str(), Some("simulator" | "surfpool")))
    {
        return Err(error("unsupported_mode", "modes", "Unknown execution mode"));
    }
    let filter = fields(
        root.get("filter").unwrap_or(&Value::Null),
        "filter",
        &["methods", "signatures", "commitments", "transaction_origin"],
    )?;
    for key in ["methods", "signatures", "commitments"] {
        list(filter.get(key), &format!("filter.{key}"), 32)?;
    }
    let state = fields(
        root.get("state").unwrap_or(&Value::Null),
        "state",
        &["key", "scope", "max_keys", "on_exhausted"],
    )?;
    bounded(state.get("max_keys"), "state.max_keys", 1, 1024)?;
    if let Some(steps) = list(root.get("sequence"), "sequence", 8)? {
        for (index, step) in steps.iter().enumerate() {
            let path = format!("sequence[{index}]");
            let step = fields(step, &path, &["id", "using", "with", "until", "next"])?;
            let until = fields(
                step.get("until").unwrap_or(&Value::Null),
                &format!("{path}.until"),
                &["elapsed_ms", "clock", "matches"],
            )?;
            bounded(
                until.get("elapsed_ms"),
                &format!("{path}.until.elapsed_ms"),
                1,
                120000,
            )?;
            bounded(
                until.get("matches"),
                &format!("{path}.until.matches"),
                1,
                10000,
            )?;
        }
    }
    if let Some(rules) = list(root.get("findings"), "findings", 8)? {
        for (index, rule) in rules.iter().enumerate() {
            let path = format!("findings[{index}]");
            let rule = fields(
                rule,
                &path,
                &["id", "template", "facts", "same", "order", "within_ms"],
            )?;
            bounded(
                rule.get("within_ms"),
                &format!("{path}.within_ms"),
                1,
                300000,
            )?;
            if let Some(facts) = rule.get("facts").and_then(Value::as_object) {
                for (name, fact) in facts {
                    fields(fact, &format!("{path}.facts.{name}"), &["kind", "step"])?;
                }
            }
        }
    }
    if let Some(repeat) = root.get("repeat") {
        let repeat = fields(repeat, "repeat", &["cycles", "while_active"])?;
        bounded(repeat.get("cycles"), "repeat.cycles", 1, 100)?;
    }
    Ok(())
}
fn identifier(value: &str, path: &str) -> Result<(), ValidationError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        Err(error(
            "limit_exceeded",
            path,
            "Identifiers require 1..64 ASCII letters, digits, dots, underscores, or hyphens",
        ))
    } else {
        Ok(())
    }
}
fn primitive<'a>(name: &'a str, path: &str) -> Result<&'a str, ValidationError> {
    if PRIMITIVES.contains(&name) {
        return Ok(name);
    }
    let base = name.split('@').next().unwrap_or("");
    if PRIMITIVES
        .iter()
        .any(|known| known.split('@').next() == Some(base))
    {
        Err(error(
            "incompatible_version",
            path,
            "The exact primitive version is unavailable",
        ))
    } else {
        Err(error(
            "unknown_primitive",
            path,
            "The primitive is not in the maintained catalog",
        ))
    }
}
fn template(name: &str, path: &str) -> Result<(), ValidationError> {
    if name == TEMPLATE {
        Ok(())
    } else if name.split('@').next() == Some("lookup-hidden-then-queried") {
        Err(error(
            "incompatible_version",
            path,
            "The exact Finding template version is unavailable",
        ))
    } else {
        Err(error(
            "invalid_finding_rule",
            path,
            "The Finding template is not in the maintained catalog",
        ))
    }
}
fn stage_methods(primitive: &str) -> &'static [&'static str] {
    match primitive {
        "rpc.withhold-submission-response@1" | "sim.confirmation-delay@1" => &["sendTransaction"],
        "rpc.lookup-null@1" | "rpc.lookup-snapshot@1" => {
            &["getSignatureStatuses", "getTransaction"]
        }
        "rpc.blockhash-reuse@1" => &["getLatestBlockhash"],
        _ => &crate::backend::METHODS,
    }
}
fn accepts_method(primitive: &str, method: &str, key: &str) -> bool {
    stage_methods(primitive).contains(&method)
        && (key != "transaction_signature" || SIGNED_METHODS.contains(&method))
}
fn arguments(step: &Step, index: usize) -> Result<(), ValidationError> {
    if step.with.is_null() {
        return Ok(());
    }
    let path = format!("sequence[{index}].with");
    let allowed: &[&str] = match step.using.as_str() {
        "rpc.timeout-before-dispatch@1" | "rpc.withhold-submission-response@1" => &["hold_real_ms"],
        "sim.confirmation-delay@1" => &[
            "processing_ms",
            "confirmation_interval_ms",
            "extra_delay_ms",
            "hold_at",
        ],
        "rpc.blockhash-reuse@1" => &["expiry_window_real_ms"],
        _ => &[],
    };
    let arguments = fields(&step.with, &path, allowed)?;
    for (key, value) in arguments {
        if key == "hold_at" {
            if !matches!(value.as_str(), Some("processed" | "confirmed")) {
                return Err(error(
                    "invalid_sequence",
                    &format!("{path}.hold_at"),
                    "hold_at must be processed or confirmed",
                ));
            }
        } else {
            bounded(Some(value), &format!("{path}.{key}"), 1, 120000)?;
        }
    }
    Ok(())
}
fn semantics(artifact: &Artifact, mode: Mode) -> Result<(), ValidationError> {
    identifier(&artifact.id, "id")?;
    identifier(&artifact.revision, "revision")?;
    for (path, text) in [
        ("description", &artifact.description),
        ("guidance", &artifact.guidance),
    ] {
        if text.as_ref().is_some_and(|text| text.chars().count() > 512) {
            return Err(error("limit_exceeded", path, "Text exceeds 512 characters"));
        }
    }
    if artifact.modes.is_empty() || !artifact.modes.contains(&mode) {
        return Err(error(
            "unsupported_mode",
            "modes",
            "The selected Lab mode is not declared by this Case",
        ));
    }
    if artifact.modes.len() == 2 && artifact.modes[0] == artifact.modes[1] {
        return Err(error(
            "unsupported_mode",
            "modes",
            "Execution modes must be unique",
        ));
    }
    for (index, name) in artifact.requires.primitives.iter().enumerate() {
        primitive(name, &format!("requires.primitives[{index}]"))?;
        if name == "sim.confirmation-delay@1" && artifact.modes.contains(&Mode::Surfpool) {
            return Err(error(
                "unsupported_mode",
                &format!("requires.primitives[{index}]"),
                "Required simulator confirmation capability is unavailable in Surfpool mode",
            ));
        }
    }
    for (index, name) in artifact.requires.finding_templates.iter().enumerate() {
        template(name, &format!("requires.finding_templates[{index}]"))?;
    }
    if !matches!(
        artifact.state.key.as_str(),
        "case" | "transaction_signature"
    ) || artifact.state.scope != "activation"
        || artifact.state.on_exhausted != "error"
    {
        return Err(error(
            "invalid_filter",
            "state",
            "State requires case or transaction_signature, activation scope, and on_exhausted:error",
        ));
    }
    if !matches!(
        artifact.filter.transaction_origin.as_str(),
        "accepted_under_this_activation" | "known_local"
    ) {
        return Err(error(
            "invalid_filter",
            "filter.transaction_origin",
            "Unknown transaction origin",
        ));
    }
    for (index, method) in artifact.filter.methods.iter().enumerate() {
        if !crate::backend::METHODS.contains(&method.as_str()) {
            return Err(error(
                "invalid_filter",
                &format!("filter.methods[{index}]"),
                "Unknown or unimplemented RPC method",
            ));
        }
    }
    if artifact.modes.contains(&Mode::Simulator)
        && let Some(index) =
            artifact.filter.methods.iter().position(|method| {
                matches!(method.as_str(), "simulateTransaction" | "requestAirdrop")
            })
    {
        return Err(error(
            "unsupported_mode",
            &format!("filter.methods[{index}]"),
            "This RPC method requires an Execution Backend",
        ));
    }
    for (index, signature) in artifact.filter.signatures.iter().enumerate() {
        crate::protocol::signature(&json!(signature)).map_err(|_| {
            error(
                "invalid_filter",
                &format!("filter.signatures[{index}]"),
                "Expected an exact first transaction signature",
            )
        })?;
    }
    if artifact
        .filter
        .commitments
        .iter()
        .any(|value| !matches!(value.as_str(), "processed" | "confirmed" | "finalized"))
    {
        return Err(error(
            "invalid_filter",
            "filter.commitments",
            "Unknown normalized commitment",
        ));
    }
    if artifact.sequence.is_empty() {
        return Err(error(
            "invalid_sequence",
            "sequence",
            "At least one step is required",
        ));
    }
    let methods: Vec<&str> = if artifact.filter.methods.is_empty() {
        crate::backend::METHODS
            .iter()
            .copied()
            .filter(|method| {
                !artifact.modes.contains(&Mode::Simulator)
                    || !matches!(*method, "simulateTransaction" | "requestAirdrop")
            })
            .collect()
    } else {
        artifact.filter.methods.iter().map(String::as_str).collect()
    };
    let mut ids = BTreeMap::new();
    let mut looped = false;
    for (index, step) in artifact.sequence.iter().enumerate() {
        let path = format!("sequence[{index}]");
        identifier(&step.id, &format!("{path}.id"))?;
        if step.id == "done" || ids.insert(step.id.as_str(), index).is_some() {
            return Err(error(
                "invalid_sequence",
                &format!("{path}.id"),
                "Step IDs must be unique and cannot use terminal name done",
            ));
        }
        primitive(&step.using, &format!("{path}.using"))?;
        if !artifact.requires.primitives.contains(&step.using) {
            return Err(error(
                "incompatible_version",
                &format!("{path}.using"),
                "The exact primitive must be declared in requires.primitives",
            ));
        }
        if step.using == "sim.confirmation-delay@1" && artifact.modes.contains(&Mode::Surfpool) {
            return Err(error(
                "unsupported_mode",
                &format!("{path}.using"),
                "Authoritative confirmation delay requires simulator mode",
            ));
        }
        if step.using == "rpc.blockhash-reuse@1" && artifact.state.key != "case" {
            return Err(error(
                "invalid_filter",
                "state.key",
                "Blockhash capture requires the case state key",
            ));
        }
        if !methods.iter().any(|method| {
            accepts_method(&step.using, method, &artifact.state.key)
                && (artifact.filter.signatures.is_empty() || SIGNED_METHODS.contains(method))
        }) {
            return Err(error(
                "invalid_filter",
                &format!("{path}.using"),
                "No declared method is compatible with this primitive stage and state key",
            ));
        }
        arguments(step, index)?;
        match (
            &step.until.elapsed_ms,
            &step.until.clock,
            &step.until.matches,
        ) {
            (Some(_), Some(clock), None) if matches!(clock.as_str(), "real" | "simulator") => {
                if clock == "simulator" && artifact.modes.contains(&Mode::Surfpool) {
                    return Err(error(
                        "unsupported_mode",
                        &format!("{path}.until.clock"),
                        "Simulator step clocks are unavailable in Surfpool mode",
                    ));
                }
            }
            (None, None, Some(_)) => {}
            _ => {
                return Err(error(
                    "invalid_sequence",
                    &format!("{path}.until"),
                    "until requires elapsed_ms with one supported clock, or matches",
                ));
            }
        }
    }
    if !artifact.filter.methods.is_empty()
        && artifact.filter.methods.iter().any(|method| {
            !artifact
                .sequence
                .iter()
                .any(|step| accepts_method(&step.using, method, &artifact.state.key))
        })
    {
        return Err(error(
            "invalid_filter",
            "filter.methods",
            "A method is incompatible with every selected primitive stage",
        ));
    }
    if !artifact.filter.signatures.is_empty()
        && !methods.iter().any(|method| SIGNED_METHODS.contains(method))
    {
        return Err(error(
            "invalid_filter",
            "filter.signatures",
            "Signature filters require a signature-bearing method",
        ));
    }
    if !artifact.filter.commitments.is_empty()
        && !methods
            .iter()
            .any(|method| COMMITMENT_METHODS.contains(method))
    {
        return Err(error(
            "invalid_filter",
            "filter.commitments",
            "Commitment filters require a method with supported commitment selection",
        ));
    }
    for (index, step) in artifact.sequence.iter().enumerate() {
        if step.next == "done" {
            continue;
        }
        let Some(&target) = ids.get(step.next.as_str()) else {
            return Err(error(
                "invalid_sequence",
                &format!("sequence[{index}].next"),
                "Unknown next step",
            ));
        };
        if target <= index {
            if target != 0 || artifact.repeat.is_none() {
                return Err(error(
                    "invalid_sequence",
                    &format!("sequence[{index}].next"),
                    "Backward edges require a declared repeat to the first step",
                ));
            }
            looped = true;
        }
    }
    let mut reached = BTreeSet::new();
    let mut index = 0;
    while reached.insert(index) {
        let next = &artifact.sequence[index].next;
        if next == "done" {
            break;
        }
        index = *ids.get(next.as_str()).expect("next targets were checked");
    }
    if reached.len() != artifact.sequence.len() {
        return Err(error(
            "invalid_sequence",
            "sequence",
            "Every declared step must be reachable from the first step",
        ));
    }
    if let Some(repeat) = &artifact.repeat
        && (!matches!(
            (repeat.cycles, repeat.while_active),
            (Some(_), None) | (None, Some(true))
        ) || !looped
            || !artifact
                .sequence
                .iter()
                .any(|step| step.until.elapsed_ms.is_some()))
    {
        return Err(error(
            "invalid_sequence",
            "repeat",
            "Repeat requires one bound, a loop to the first step, and a positive clock-bound step",
        ));
    }
    let mut finding_ids = BTreeSet::new();
    for (index, rule) in artifact.findings.iter().enumerate() {
        let path = format!("findings[{index}]");
        identifier(&rule.id, &format!("{path}.id"))?;
        if !finding_ids.insert(&rule.id) {
            return Err(error(
                "invalid_finding_rule",
                &format!("{path}.id"),
                "Finding rule IDs must be unique",
            ));
        }
        template(&rule.template, &format!("{path}.template"))?;
        if !artifact.requires.finding_templates.contains(&rule.template) {
            return Err(error(
                "incompatible_version",
                &format!("{path}.template"),
                "The exact Finding template must be declared in requires.finding_templates",
            ));
        }
        if rule.same != "transaction_signature"
            || rule.order != ["hidden", "later"]
            || rule.facts.len() != 2
            || !rule.facts.contains_key("hidden")
            || !rule.facts.contains_key("later")
        {
            return Err(error(
                "invalid_finding_rule",
                &path,
                "Template requires hidden/later fact slots, their declared order, and transaction_signature correlation",
            ));
        }
        let hidden = &rule.facts["hidden"];
        let later = &rule.facts["later"];
        if hidden.kind != "lookup_hidden" || later.kind != "lookup_received" {
            return Err(error(
                "invalid_finding_rule",
                &format!("{path}.facts"),
                "Fact kinds do not satisfy the published template slots",
            ));
        }
        let (Some(&hide), Some(&observe)) =
            (ids.get(hidden.step.as_str()), ids.get(later.step.as_str()))
        else {
            return Err(error(
                "invalid_finding_rule",
                &format!("{path}.facts"),
                "A Finding selector references an unknown step",
            ));
        };
        if hide > observe
            || !["rpc.lookup-null@1", "rpc.lookup-snapshot@1"]
                .contains(&artifact.sequence[hide].using.as_str())
            || !["getSignatureStatuses", "getTransaction"]
                .iter()
                .any(|method| {
                    methods.contains(method)
                        && accepts_method(
                            &artifact.sequence[observe].using,
                            method,
                            &artifact.state.key,
                        )
                })
        {
            return Err(error(
                "invalid_finding_rule",
                &format!("{path}.facts"),
                "Selected steps cannot emit the ordered lookup facts required by this template",
            ));
        }
    }
    Ok(())
}
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<_, _> = map.iter().collect();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(key, value)| (key.clone(), canonical(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        value => value.clone(),
    }
}
fn default_arguments(id: &str) -> Value {
    match id {
        "rpc.timeout-before-dispatch@1" | "rpc.withhold-submission-response@1" => {
            json!({"hold_real_ms":20000})
        }
        "sim.confirmation-delay@1" => {
            json!({"processing_ms":2000,"confirmation_interval_ms":2000,"extra_delay_ms":30000,"hold_at":"processed"})
        }
        "rpc.blockhash-reuse@1" => json!({"expiry_window_real_ms":10000}),
        _ => json!({}),
    }
}
fn compile(value: Value, mode: Mode) -> Result<Plan, ValidationError> {
    shape(&value)?;
    let mut artifact: Artifact = serde_json::from_value(value.clone()).map_err(|_| {
        error(
            "invalid_yaml",
            "$",
            "Missing required field or invalid field type",
        )
    })?;
    semantics(&artifact, mode)?;
    for step in &mut artifact.sequence {
        if step.with.is_null() {
            step.with = json!({});
        }
        for (key, value) in default_arguments(&step.using)
            .as_object()
            .expect("default argument maps are objects")
        {
            step.with
                .as_object_mut()
                .expect("arguments validated as mapping")
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
    }
    let canonical_input = canonical(&value);
    let bytes = serde_json::to_vec(&canonical_input).expect("bounded JSON serializes");
    Ok(Plan {
        digest: crate::query::digest(&bytes),
        resolved_primitives: artifact
            .requires
            .primitives
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        resolved_templates: artifact
            .requires
            .finding_templates
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        artifact,
        canonical_input,
    })
}
pub(super) fn validate(yaml: &str, mode: Mode) -> Result<Plan, ValidationError> {
    compile(parse(yaml)?, mode)
}
fn canonical_shape(value: &Value, depth: usize) -> Result<(), ValidationError> {
    if depth > 32 {
        return Err(error(
            "limit_exceeded",
            "canonical_input",
            "Canonical artifact nesting exceeds 32 levels",
        ));
    }
    match value {
        Value::Null => Err(error(
            "invalid_yaml",
            "canonical_input",
            "Null input values are not supported",
        )),
        Value::Number(n) if !n.is_i64() && !n.is_u64() => Err(error(
            "invalid_yaml",
            "canonical_input",
            "Only bounded integer numbers are supported",
        )),
        Value::Array(values) => {
            for value in values {
                canonical_shape(value, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            for value in values.values() {
                canonical_shape(value, depth + 1)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
pub(super) fn revalidate(plan: &Plan, mode: Mode) -> Result<Plan, ValidationError> {
    canonical_shape(&plan.canonical_input, 0)?;
    if serde_json::to_vec(&plan.canonical_input)
        .map_or(true, |bytes| bytes.len() > 6 * MAX_ARTIFACT_BYTES)
    {
        return Err(error(
            "limit_exceeded",
            "canonical_input",
            "Canonical artifact exceeds its encoded byte budget",
        ));
    }
    let rebuilt = compile(plan.canonical_input.clone(), mode)?;
    if rebuilt.digest != plan.digest
        || rebuilt.resolved_primitives != plan.resolved_primitives
        || rebuilt.resolved_templates != plan.resolved_templates
        || serde_json::to_value(&rebuilt.artifact).expect("typed artifact serializes")
            != serde_json::to_value(&plan.artifact).expect("typed artifact serializes")
    {
        return Err(error(
            "incompatible_version",
            "plan",
            "Resolved plan differs from its canonical artifact or installed catalog",
        ));
    }
    Ok(rebuilt)
}
pub(super) fn catalog() -> Value {
    let entries:Vec<Value>=PRIMITIVES.iter().map(|id|{
        let(stage,modes,args,evidence)=match *id{
            "rpc.timeout-before-dispatch@1"=>("before_dispatch",vec!["simulator","surfpool"],json!({"hold_real_ms":{"default":20000,"minimum":1,"maximum":120000}}),vec!["rpc.dispatch_prevented","fault.withholding_started","rpc.boundary"]),
            "rpc.withhold-submission-response@1"=>("after_successful_acceptance",vec!["simulator","surfpool"],json!({"hold_real_ms":{"default":20000,"minimum":1,"maximum":120000}}),vec!["transaction.accepted","fault.response_loss_selected","rpc.boundary"]),
            "rpc.lookup-null@1"|"rpc.lookup-snapshot@1"=>("after_authoritative_lookup",vec!["simulator","surfpool"],json!({}),vec!["rpc.lookup_source","fault.lookup_changed","rpc.boundary"]),
            "sim.confirmation-delay@1"=>("first_simulator_acceptance",vec!["simulator"],json!({"processing_ms":{"default":2000,"minimum":1,"maximum":120000},"confirmation_interval_ms":{"default":2000,"minimum":1,"maximum":120000},"extra_delay_ms":{"default":30000,"minimum":1,"maximum":120000},"hold_at":{"default":"processed","enum":["processed","confirmed"]}}),vec!["transaction.accepted","fault.confirmation_schedule_assigned","transaction.transition"]),
            "rpc.blockhash-reuse@1"=>("after_authoritative_blockhash",vec!["simulator","surfpool"],json!({"expiry_window_real_ms":{"default":10000,"minimum":1,"maximum":120000}}),vec!["rpc.blockhash_source","rpc.blockhash_height","fault.blockhash_returned","transaction.signed_attempt"]),
            _=>("received_valid_rpc",vec!["simulator","surfpool"],json!({}),vec!["rpc.registered"]),
        };{
            let properties:Map<String,Value>=args.as_object().expect("catalog arguments are objects").iter().map(|(name,spec)|{let mut spec=spec.clone();spec["type"]=json!(if name=="hold_at"{"string"}else{"integer"});(name.clone(),spec)}).collect();
            json!({"id":id,"version":1,"matching_stage":stage,"methods":stage_methods(id),"modes":modes,"arguments":args,"argument_schema":{"type":"object","additionalProperties":false,"properties":properties},"evidence":evidence,"limits":{"keys":1024,"held_requests":16}})
        }
    }).collect();
    json!({"schema":"ronchi.case/v1","canonicalization":"sorted-json-v1","limits":{"artifact_bytes":MAX_ARTIFACT_BYTES,"steps":8,"findings":8,"filter_values":32,"keys":1024,"held_requests":16},"primitives":entries,"finding_templates":[{"id":TEMPLATE,"version":1,"facts":{"hidden":"lookup_hidden","later":"lookup_received"},"same":["transaction_signature"],"order":["hidden","later"],"within_ms":{"minimum":1,"maximum":300000},"statement":"An available lookup was hidden and a later lookup for the same signature was received.","limitations":["No inferred client outcome, business intent, economic result, or Verdict."]}]})
}
