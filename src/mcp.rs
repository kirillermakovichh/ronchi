//! Local stdio adapter. The SDK owns framing; Lab owners outlive this attachment.
use crate::{cases, checks, config, lab, query, workspace::Workspace};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
    },
    service::RequestContext,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Target {
    lab_id: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Status {
    lab_id: Option<String>,
    name: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Up {
    #[serde(default = "default_name")]
    name: String,
    #[serde(default)]
    arguments: config::Settings,
}
fn default_name() -> String {
    "default".into()
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Down {
    lab_id: String,
    #[serde(default)]
    force: bool,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Arm {
    lab_id: String,
    arguments: cases::Arm,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Query {
    lab_id: String,
    #[serde(default)]
    arguments: query::Arguments,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Observe {
    lab_id: String,
    #[serde(default)]
    action: ObserveAction,
    arguments: checks::Arguments,
}
#[derive(Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ObserveAction {
    #[default]
    Collect,
    Status,
    Cancel,
}

#[derive(Clone)]
struct Adapter {
    project: PathBuf,
    permits: Arc<tokio::sync::Semaphore>,
}
fn parse<T: DeserializeOwned>(value: Value) -> Result<T, query::Error> {
    serde_json::from_value(value)
        .map_err(|_| query::Error::invalid("Arguments do not match the tool schema"))
}
fn value(value: impl Serialize) -> Value {
    serde_json::to_value(value).expect("typed operation result serializes")
}
fn lab_error(error: lab::LabError) -> query::Error {
    let mut result = query::Error::new(error.code, &error.message);
    result.phase = error.phase.into();
    result.effects = error.effects.into();
    result.recovery =
        "Inspect ronchi_lab_status with the immutable Lab ID before retrying a mutation".into();
    result
}
fn check_error(error: checks::Error) -> query::Error {
    serde_json::from_value(value(error)).unwrap_or_else(|_| query::Error::storage())
}
fn identity(id: &str) -> Result<(), query::Error> {
    if !crate::workspace::identifier(id) || id.len() > 128 {
        return Err(query::Error::invalid("Expected an immutable Lab ID"));
    }
    Ok(())
}
fn lab_result(result: lab::LabResult, request_id: &str) -> Value {
    json!({"contract_version":1,"request_id":request_id,"lab_id":result.manifest.lab_id,"result":result})
}
fn lab_reply(result: Result<lab::LabResult, lab::LabError>, request_id: &str) -> Value {
    match result {
        Ok(result) => lab_result(result, request_id),
        Err(error) => {
            let lab_id = error.lab_id.clone();
            json!({"contract_version":1,"request_id":request_id,"lab_id":lab_id,"error":lab_error(error)})
        }
    }
}
fn schema<T: JsonSchema>() -> serde_json::Map<String, Value> {
    let mut result = value(schemars::schema_for!(T));
    // Shared runtime normalization enforces these ceilings in every interface.
    if let Some(limit) = result.pointer_mut("/$defs/Arguments/properties/limit") {
        limit["minimum"] = json!(1);
        limit["maximum"] = json!(200);
    }
    result.as_object().expect("object schema").clone()
}
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum CatalogOutput {
    Success(crate::CatalogReply),
    Failure(query::Failure),
}
fn output_schema(name: &str) -> Arc<serde_json::Map<String, Value>> {
    let mut schema = if name.starts_with("ronchi_investigate_") || name == "ronchi_check_evaluate" {
        schema::<query::Envelope>()
    } else if name == "ronchi_check_observe" {
        schema::<checks::Envelope>()
    } else if name == "ronchi_case_list" {
        schema::<CatalogOutput>()
    } else {
        schema::<crate::LabEnvelope>()
    };
    schema.insert("type".into(), json!("object"));
    Arc::new(schema)
}
fn tool<T: JsonSchema>(name: &'static str, description: &'static str, read_only: bool) -> Tool {
    Tool::new(name, description, schema::<T>())
        .with_raw_output_schema(output_schema(name))
        .with_annotations(
            ToolAnnotations::new()
                .read_only(read_only)
                .destructive(name == "ronchi_lab_down")
                .idempotent(read_only)
                .open_world(name == "ronchi_check_observe"),
        )
}
fn tools() -> &'static [Tool] {
    static TOOLS: std::sync::OnceLock<Vec<Tool>> = std::sync::OnceLock::new();
    TOOLS.get_or_init(|| vec![
        tool::<Up>(
            "ronchi_lab_up",
            "Start a persistent project-local Lab and await bounded readiness. The Lab survives adapter EOF. Inspect status after uncertain completion.",
            false,
        ),
        tool::<Status>(
            "ronchi_lab_status",
            "Resolve a project-local name or inspect an immutable Lab ID, including retained evidence. Returns resolved identity.",
            true,
        ),
        tool::<Down>(
            "ronchi_lab_down",
            "Stop the explicitly identified Lab and retain its Journal. Inspect status after uncertain completion.",
            false,
        ),
        tool::<Empty>(
            "ronchi_case_list",
            "List maintained Cases and mode limits offline without a Lab.",
            true,
        ),
        tool::<Arm>(
            "ronchi_case_arm",
            "Arm or replace the continuing Case on an explicit live Lab. Repetition creates a new revision; inspect status after uncertain completion.",
            false,
        ),
        tool::<Target>(
            "ronchi_case_disarm",
            "Disarm future registrations on an explicit live Lab; admitted effects keep their policy.",
            false,
        ),
        tool::<Observe>(
            "ronchi_check_observe",
            "Collect bounded configured observations. Supply a stable collection_id. Use action:status or action:cancel with the same identity to inspect uncertain completion without restarting. Protocol cancellation stops remaining polls.",
            false,
        ),
        tool::<Query>(
            "ronchi_check_evaluate",
            "Evaluate retained facts at explicit snapshot_seq with arguments.evaluation. Does not contact the source. Requires explicit Lab ID.",
            true,
        ),
        tool::<Query>(
            "ronchi_investigate_summary",
            "Inspect a bounded Journal summary, at most five Findings, and Evidence Gaps for an explicit Lab ID. Request limit 1..200; response at most 256 KiB.",
            true,
        ),
        tool::<Query>(
            "ronchi_investigate_timeline",
            "Read one pinned Timeline page. Filters, projection and cursor share CLI semantics. Limit 1..200, request 64 KiB, response 256 KiB.",
            true,
        ),
        tool::<Query>(
            "ronchi_investigate_evidence",
            "Resolve bounded evidence references. Payload is opt-in with include_payload; next payload offset and pagination preserve completeness.",
            true,
        ),
        tool::<Query>(
            "ronchi_investigate_findings",
            "Read bounded deterministic Findings with pinned snapshot, scope and continuation. Findings do not invent Client Outcomes.",
            true,
        ),
    ])
}
impl Adapter {
    async fn execute(
        &self,
        tool: &str,
        input: Value,
        request_id: &str,
        cancel: CancellationToken,
    ) -> Result<Value, query::Error> {
        match tool {
            "ronchi_case_list" => {
                let _: Empty = parse(input)?;
                Ok(json!({"contract_version":1,"request_id":request_id,"result":cases::catalog()}))
            }
            "ronchi_lab_up" => {
                let input: Up = parse(input)?;
                if !crate::workspace::identifier(&input.name) {
                    return Err(query::Error::invalid("Invalid project-local name"));
                }
                let (configuration, settings) =
                    config::resolve_requested(self.project.clone(), input.arguments)
                        .map_err(|m| query::Error::invalid(&m))?;
                Ok(lab_reply(
                    lab::up_cancellable(configuration, settings, &input.name, cancel).await,
                    request_id,
                ))
            }
            "ronchi_lab_status" => {
                let input: Status = parse(input)?;
                if input.lab_id.is_some() && input.name.is_some() {
                    return Err(query::Error::invalid("Select a name or an exact Lab ID"));
                }
                if let Some(id) = &input.lab_id {
                    identity(id)?;
                }
                let name = input.name.as_deref().unwrap_or("default");
                if !crate::workspace::identifier(name) {
                    return Err(query::Error::invalid("Invalid project-local name"));
                }
                Ok(lab_reply(
                    lab::status(self.project.clone(), name, input.lab_id.as_deref()).await,
                    request_id,
                ))
            }
            "ronchi_lab_down" => {
                let input: Down = parse(input)?;
                identity(&input.lab_id)?;
                Ok(lab_reply(
                    mutation(
                        lab::down(
                            self.project.clone(),
                            "default",
                            Some(&input.lab_id),
                            input.force,
                        ),
                        &cancel,
                        &input.lab_id,
                    )
                    .await,
                    request_id,
                ))
            }
            "ronchi_case_arm" | "ronchi_case_disarm" => {
                let (id, settings) = if tool == "ronchi_case_arm" {
                    let input: Arm = parse(input)?;
                    (input.lab_id, Some(input.arguments))
                } else {
                    let input: Target = parse(input)?;
                    (input.lab_id, None)
                };
                identity(&id)?;
                Ok(lab_reply(
                    mutation(
                        lab::case_change(self.project.clone(), "default", Some(&id), settings),
                        &cancel,
                        &id,
                    )
                    .await,
                    request_id,
                ))
            }
            "ronchi_check_observe" => {
                let mut input: Observe = parse(input)?;
                identity(&input.lab_id)?;
                input.arguments.normalize().map_err(check_error)?;
                let selected = lab::status(self.project.clone(), "default", Some(&input.lab_id))
                    .await
                    .map_err(lab_error)?;
                if selected.manifest.state != lab::Lifecycle::Ready {
                    return Err(query::Error::new(
                        "state_conflict",
                        "Check collection requires a ready Lab",
                    ));
                }
                let ws = Workspace::new(&self.project);
                let request = checks::Request {
                    contract_version: 1,
                    operation: "check.observe".into(),
                    request_id: request_id.into(),
                    lab_id: input.lab_id,
                    arguments: input.arguments,
                };
                if !matches!(input.action, ObserveAction::Collect) {
                    return crate::control::check_request(
                        &ws,
                        &selected.manifest,
                        None,
                        &request.arguments.collection_id,
                        matches!(input.action, ObserveAction::Cancel),
                    )
                    .await
                    .map(value)
                    .map_err(check_error);
                }
                // Complete start admission before observing cancellation. Never retry it blindly.
                let mut reply = crate::control::check_request(
                    &ws,
                    &selected.manifest,
                    Some(&request),
                    &request.arguments.collection_id,
                    false,
                )
                .await
                .map_err(check_error)?;
                while reply.result.status == "running" {
                    tokio::select! {
                        _=cancel.cancelled()=> {
                            let outcome = cancel_collection(&ws, &selected.manifest, &request.arguments.collection_id).await;
                            let mut error = query::Error::new("cancelled","Collection cancellation requested; inspect collection_id before retrying");
                            error.effects = if outcome.is_ok() { "applied" } else { "unknown" }.into();
                            return Err(error);
                        }
                        _=tokio::time::sleep(Duration::from_millis(100))=> {}
                    }
                    reply = crate::control::check_request(
                        &ws,
                        &selected.manifest,
                        None,
                        &request.arguments.collection_id,
                        false,
                    )
                    .await
                    .map_err(check_error)?;
                }
                Ok(value(reply))
            }
            _ => {
                let input: Query = parse(input)?;
                identity(&input.lab_id)?;
                let op = if tool == "ronchi_check_evaluate" {
                    "check.evaluate".into()
                } else {
                    format!(
                        "investigate.{}",
                        tool.trim_start_matches("ronchi_investigate_")
                    )
                };
                let mut request = query::Request {
                    contract_version: 1,
                    request_id: request_id.into(),
                    lab_id: input.lab_id.clone(),
                    operation: query::Operation::parse(&op)?,
                    arguments: input.arguments,
                };
                request.normalize()?;
                let selected = lab::status(self.project.clone(), "default", Some(&input.lab_id))
                    .await
                    .map_err(lab_error)?;
                let ws = Workspace::new(&self.project);
                let _evidence_lease =
                    crate::maintenance::read_lock(&ws, &input.lab_id).map_err(lab_error)?;
                let result = if matches!(
                    selected.manifest.state,
                    lab::Lifecycle::Ready | lab::Lifecycle::Stopping
                ) {
                    crate::control::query_request(&ws, &selected.manifest, &request, cancel).await?
                } else {
                    query::Engine::default()
                        .execute(
                            ws.lab(&input.lab_id)
                                .map_err(lab_error)?
                                .join("journal.sqlite"),
                            false,
                            "explicit",
                            request,
                            cancel,
                        )
                        .await?
                };
                Ok(value(result))
            }
        }
    }
}
async fn mutation<F>(
    future: F,
    cancel: &CancellationToken,
    id: &str,
) -> Result<lab::LabResult, lab::LabError>
where
    F: std::future::Future<Output = Result<lab::LabResult, lab::LabError>>,
{
    tokio::select! {
        result=future=>result,
        _=cancel.cancelled()=>Err(lab::LabError { exit:130,code:"cancelled",message:"Mutation wait cancelled; inspect Lab status before retrying".into(),phase:"control",effects:"unknown",lab_id:Some(id.into()) })
    }
}
async fn cancel_collection(
    ws: &Workspace,
    manifest: &lab::Manifest,
    collection: &str,
) -> Result<(), checks::Error> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut reply = crate::control::check_request(ws, manifest, None, collection, true).await?;
        while reply.result.status == "running" {
            tokio::time::sleep(Duration::from_millis(25)).await;
            reply = crate::control::check_request(ws, manifest, None, collection, false).await?;
        }
        Ok(())
    })
    .await
    .map_err(|_| checks::Error::new("cancelled", "unknown"))?
}
impl ServerHandler for Adapter {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("Tools share Ronchi CLI operations. Pass immutable lab_id except bootstrap up/status and offline case.list. EOF only detaches. Inspect status after uncertain mutations; never retry blindly.")
    }
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if request.is_some_and(|r| r.cursor.is_some()) {
            return Err(ErrorData::invalid_params(
                "No catalog continuation cursor",
                None,
            ));
        }
        Ok(ListToolsResult {
            tools: tools().to_vec(),
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if !tools().iter().any(|t| t.name == request.name) {
            return Err(ErrorData::invalid_params("Unknown Ronchi tool", None));
        }
        let request_id = format!("mcp-{}", context.id);
        let input = Value::Object(request.arguments.unwrap_or_default());
        let lab_id = input
            .get("lab_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let collection_id = input
            .pointer("/arguments/collection_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let result = if query::encoded_len(&input, query::MAX_REQUEST_BYTES).is_err() {
            Err(query::Error::invalid("Encoded arguments exceed 64 KiB"))
        } else if let Ok(_permit) = self.permits.try_acquire() {
            if context.ct.is_cancelled() {
                Err(query::Error::new(
                    "cancelled",
                    "Request cancelled before execution",
                ))
            } else {
                self.execute(&request.name, input, &request_id, context.ct)
                    .await
            }
        } else {
            Err(query::Error::new(
                "capacity_exceeded",
                "Four MCP operations are already active",
            ))
        };
        let (value, error) = match result {
            Ok(value) if query::encoded_len(&value, query::MAX_RESPONSE_BYTES).is_ok() => {
                let is_error = value.get("error").is_some();
                (value, is_error)
            }
            Ok(value) => {
                let resolved_id = value
                    .get("lab_id")
                    .cloned()
                    .or_else(|| lab_id.clone().map(Value::String));
                let mut error =
                    query::Error::new("result_too_large", "Operation result exceeds 256 KiB");
                if !tools()
                    .iter()
                    .find(|t| t.name == request.name)
                    .and_then(|t| t.annotations.as_ref())
                    .and_then(|a| a.read_only_hint)
                    .unwrap_or(false)
                {
                    error.effects = "applied".into();
                    error.recovery = "Inspect the resolved Lab ID and collection_id before another mutation; do not retry blindly".into();
                }
                let collection = value.pointer("/result/collection_id").cloned();
                (
                    json!({"contract_version":1,"request_id":request_id,"lab_id":resolved_id,"collection_id":collection,"error":error}),
                    true,
                )
            }
            Err(error) => (
                json!({"contract_version":1,"request_id":request_id,"lab_id":lab_id,"collection_id":collection_id,"error":error}),
                true,
            ),
        };
        let mut result = if error {
            CallToolResult::error(vec![ContentBlock::text(
                "Ronchi operation failed; inspect structured error and recovery.",
            )])
        } else {
            CallToolResult::success(vec![ContentBlock::text(
                "Ronchi operation completed; inspect structured result.",
            )])
        };
        result.structured_content = Some(value);
        Ok(result.into())
    }
}

pub(crate) async fn serve(project: PathBuf) -> Result<(), lab::LabError> {
    let adapter = Adapter {
        project,
        permits: Arc::new(tokio::sync::Semaphore::new(4)),
    };
    let transport =
        rmcp::transport::async_rw::AsyncRwTransport::new(tokio::io::stdin(), tokio::io::stdout())
            .with_max_line_length(query::MAX_REQUEST_BYTES * 2);
    let permits = adapter.permits.clone();
    let server = adapter
        .serve(transport)
        .await
        .map_err(|_| lab::LabError::internal("MCP transport initialization failed"))?;
    server
        .waiting()
        .await
        .map_err(|_| lab::LabError::internal("MCP transport stopped with an error"))?;
    // SDK EOF cancels request tokens after its response drain. Keep this runtime
    // alive until startup cleanup and already accepted bounded operations settle.
    let _drained = tokio::time::timeout(Duration::from_secs(40), permits.acquire_many(4))
        .await
        .map_err(|_| {
            lab::LabError::internal("MCP operation cleanup deadline exceeded; inspect Lab status")
        })?
        .map_err(|_| lab::LabError::internal("MCP operation admission closed"))?;
    Ok(())
}
