//! Custom Case authoring through the same processes used by CLI and MCP callers.
use rmcp::{
    ClientHandler, ServiceExt,
    model::{CallToolRequestParams, CallToolResult},
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const YAML: &str = r#"schema: ronchi.case/v1
id: custom.public-authoring
revision: "1"
requires:
  primitives: [rpc.timeout-before-dispatch@1]
modes: [simulator]
filter:
  methods: [getHealth]
  transaction_origin: known_local
state: {key: case, scope: activation, max_keys: 1, on_exhausted: error}
sequence:
  - id: hold
    using: rpc.timeout-before-dispatch@1
    until: {matches: 1}
    next: done
"#;

struct Project {
    directory: tempfile::TempDir,
    path: PathBuf,
}
impl Project {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project");
        Self { directory, path }
    }
    fn file(&self, name: &str, source: &str) -> PathBuf {
        let path = self.directory.path().join(name);
        std::fs::write(&path, source).unwrap();
        path
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ronchi"));
        clean_environment(&mut command);
        command
            .arg("--project")
            .arg(&self.path)
            .arg("--json")
            .args(args)
            .stdin(Stdio::null());
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let output = self.run(args);
        assert!(output.status.success(), "{output:?}");
        decode(output)
    }
    fn up(&self) -> Value {
        std::fs::create_dir(&self.path).unwrap();
        self.ok(&[
            "up",
            "--detach",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ])
    }
    fn mcp(&self) -> tokio::process::Child {
        std::fs::create_dir_all(&self.path).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_ronchi"));
        clean_environment(&mut command);
        command
            .arg("--project")
            .arg(&self.path)
            .args(["mcp", "stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        tokio::process::Command::from(command)
            .kill_on_drop(true)
            .spawn()
            .unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        if self.path.join("ronchi").exists() {
            let _ = self.run(&["down", "--force"]);
        }
    }
}
fn clean_environment(command: &mut Command) {
    for key in [
        "RONCHI_MODE",
        "RONCHI_READ_UPSTREAM",
        "RONCHI_SURFPOOL_DATASOURCE",
        "RONCHI_RPC_LISTEN",
        "RONCHI_CONTROL_LISTEN",
        "RONCHI_INITIAL_CASE",
        "RONCHI_SLOT_DURATION_MS",
    ] {
        command.env_remove(key);
    }
}
fn decode(output: Output) -> Value {
    assert!(output.stderr.is_empty(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}: {output:?}"))
}
fn path_text(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn offline_cli_validates_yaml_without_allocating_a_workspace() {
    let project = Project::new();
    let valid = project.file("valid.yaml", YAML);
    let reply = project.ok(&["case", "validate", path_text(&valid), "--mode", "simulator"]);
    let plan = &reply["result"];
    assert_eq!(plan["artifact"]["id"], "custom.public-authoring");
    assert_eq!(
        plan["resolved_primitives"],
        json!(["rpc.timeout-before-dispatch@1"])
    );
    assert_eq!(
        plan["artifact"]["sequence"][0]["with"]["hold_real_ms"],
        20000
    );
    assert!(plan["canonical_input"]["sequence"][0].get("with").is_none());
    assert_eq!(plan["digest"].as_str().unwrap().len(), 64);
    let invalid = project.file("invalid.yaml", &YAML.replace("matches: 1", "matches: 0"));
    let output = project.run(&["case", "validate", path_text(&invalid)]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let reply = decode(output);
    assert_eq!(reply["error"]["category"], "limit_exceeded");
    assert_eq!(reply["error"]["path"], "sequence[0].until.matches");
    assert!(reply.get("result").is_none());
    assert!(!project.path.exists());
}

fn schema_has_property(schema: &Value, property: &str) -> bool {
    match schema {
        Value::Object(map) => {
            map.get("properties")
                .and_then(Value::as_object)
                .is_some_and(|properties| properties.contains_key(property))
                || map
                    .values()
                    .any(|value| schema_has_property(value, property))
        }
        Value::Array(values) => values
            .iter()
            .any(|value| schema_has_property(value, property)),
        _ => false,
    }
}
fn assert_catalog(catalog: &Value) {
    assert_eq!(catalog["schema"], "ronchi.case/v1");
    let primitives = catalog["primitives"].as_array().unwrap();
    assert_eq!(primitives.len(), 7);
    let timeout = primitives
        .iter()
        .find(|entry| entry["id"] == "rpc.timeout-before-dispatch@1")
        .unwrap();
    assert_eq!(timeout["version"], 1);
    assert_eq!(timeout["arguments"]["hold_real_ms"]["default"], 20000);
    assert_eq!(
        timeout["argument_schema"]["properties"]["hold_real_ms"]["maximum"],
        120000
    );
    assert_eq!(
        catalog["finding_templates"][0]["id"],
        "lookup-hidden-then-queried@1"
    );
}

#[test]
fn offline_cli_discovery_describes_the_same_custom_plan_and_errors() {
    let project = Project::new();
    let catalog = project.ok(&["case", "list"]);
    assert_catalog(&catalog["custom"]);
    let schema = project.ok(&["schema", "case.validate"]);
    assert_eq!(schema["operation"], "case.validate");
    assert_catalog(&schema["custom_catalog"]);
    assert!(schema_has_property(&schema["request"], "sequence"));
    assert!(schema_has_property(&schema["response"], "digest"));
    assert!(schema_has_property(&schema["response"], "category"));
    assert!(schema_has_property(&schema["response"], "path"));
    let arm = project.ok(&["schema", "case.arm"]);
    assert!(schema_has_property(&arm["request"], "custom_yaml"));
    assert!(!project.path.exists());
}

#[test]
fn cli_activation_keeps_the_live_policy_after_an_invalid_replacement() {
    let project = Project::new();
    let up = project.up();
    assert_eq!(up["result"]["state"], "ready");
    let valid = project.file("valid.yaml", YAML);
    let plan = project.ok(&["case", "validate", path_text(&valid)]);
    let activated = project.ok(&["case", "activate", path_text(&valid)]);
    assert_eq!(activated["lab_id"], up["lab_id"]);
    let policy = &activated["result"]["active_case"];
    assert_eq!(policy["case_id"], "custom.public-authoring");
    assert_eq!(policy["custom"]["origin"], "custom");
    assert_eq!(policy["custom"]["id"], "custom.public-authoring");
    assert_eq!(policy["custom"]["digest"], plan["result"]["digest"]);
    let registration = activated["result"]["case_registration_seq"].clone();
    let invalid = project.file("invalid.yaml", &YAML.replace("matches: 1", "matches: 0"));
    let output = project.run(&["case", "activate", path_text(&invalid)]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let failed = decode(output);
    assert_eq!(failed["error"]["code"], "limit_exceeded");
    assert!(
        failed["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("sequence[0].until.matches:")
    );
    let status = project.ok(&["status"]);
    assert_eq!(status["result"]["active_case"], *policy);
    assert_eq!(status["result"]["case_registration_seq"], registration);
    assert_eq!(status["result"]["state"], "ready");
    assert_eq!(project.ok(&["down"])["result"]["state"], "stopped");
}

#[derive(Clone)]
struct Client;
impl ClientHandler for Client {}

async fn mcp_call(
    client: &rmcp::service::Peer<rmcp::RoleClient>,
    name: &'static str,
    arguments: Value,
) -> CallToolResult {
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        client.call_tool(
            CallToolRequestParams::new(name).with_arguments(arguments.as_object().unwrap().clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap()
}
async fn assert_mcp_exit(process: tokio::process::Child) {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        process.wait_with_output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[tokio::test]
async fn sdk_stdio_validates_yaml_and_publishes_its_plan_and_error_schema_offline() {
    let project = Project::new();
    let mut process = project.mcp();
    let client = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        Client.serve((
            process.stdout.take().unwrap(),
            process.stdin.take().unwrap(),
        )),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        client.peer_info().unwrap().protocol_version,
        rmcp::model::ProtocolVersion::V_2025_11_25
    );
    let tools = client.list_all_tools().await.unwrap();
    let validate = tools
        .iter()
        .find(|tool| tool.name == "ronchi_case_validate")
        .unwrap();
    assert_eq!(
        validate.annotations.as_ref().unwrap().read_only_hint,
        Some(true)
    );
    assert!(schema_has_property(
        &Value::Object((*validate.input_schema).clone()),
        "yaml"
    ));
    assert!(schema_has_property(
        &Value::Object((*validate.input_schema).clone()),
        "mode"
    ));
    let valid = mcp_call(
        &client,
        "ronchi_case_validate",
        json!({"yaml":YAML,"mode":"simulator"}),
    )
    .await;
    assert_eq!(valid.is_error, Some(false));
    let plan = valid.structured_content.unwrap()["result"].clone();
    assert_eq!(plan["artifact"]["id"], "custom.public-authoring");
    assert_eq!(
        plan["artifact"]["sequence"][0]["with"]["hold_real_ms"],
        20000
    );
    let invalid = mcp_call(
        &client,
        "ronchi_case_validate",
        json!({"yaml":YAML.replace("matches: 1", "matches: 0"),"mode":"simulator"}),
    )
    .await;
    assert_eq!(invalid.is_error, Some(true));
    let error = invalid.structured_content.unwrap();
    assert_eq!(error["error"]["category"], "limit_exceeded");
    assert_eq!(error["error"]["path"], "sequence[0].until.matches");
    let mode = mcp_call(
        &client,
        "ronchi_case_validate",
        json!({"yaml":YAML,"mode":"surfpool"}),
    )
    .await;
    assert_eq!(mode.is_error, Some(true));
    assert_eq!(
        mode.structured_content.unwrap()["error"]["category"],
        "unsupported_mode"
    );
    let catalog = mcp_call(&client, "ronchi_case_list", json!({})).await;
    assert_catalog(&catalog.structured_content.unwrap()["custom"]);
    client.cancel().await.unwrap();
    assert_mcp_exit(process).await;
    assert!(!project.path.join("ronchi").exists());
    let output = Value::Object((**validate.output_schema.as_ref().unwrap()).clone());
    assert!(schema_has_property(&output, "digest"));
    assert!(schema_has_property(&output, "category"));
    assert!(schema_has_property(&output, "path"));
}

#[tokio::test]
async fn sdk_arm_uses_custom_yaml_and_eof_preserves_the_activated_lab() {
    let project = Project::new();
    let up = project.up();
    let lab_id = up["lab_id"].as_str().unwrap();
    let mut process = project.mcp();
    let client = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        Client.serve((
            process.stdout.take().unwrap(),
            process.stdin.take().unwrap(),
        )),
    )
    .await
    .unwrap()
    .unwrap();
    let tools = client.list_all_tools().await.unwrap();
    let arm = tools
        .iter()
        .find(|tool| tool.name == "ronchi_case_arm")
        .unwrap();
    assert!(schema_has_property(
        &Value::Object((*arm.input_schema).clone()),
        "custom_yaml"
    ));
    let valid = mcp_call(
        &client,
        "ronchi_case_arm",
        json!({"lab_id":lab_id,"arguments":{"case_id":"custom","custom_yaml":YAML}}),
    )
    .await;
    assert_eq!(valid.is_error, Some(false));
    let active = valid.structured_content.unwrap()["result"]["active_case"].clone();
    assert_eq!(active["custom"]["id"], "custom.public-authoring");
    let invalid = mcp_call(&client, "ronchi_case_arm", json!({"lab_id":lab_id,"arguments":{"case_id":"custom","custom_yaml":YAML.replace("matches: 1", "matches: 0")}})).await;
    assert_eq!(invalid.is_error, Some(true));
    let error = invalid.structured_content.unwrap();
    assert_eq!(error["error"]["code"], "limit_exceeded");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("sequence[0].until.matches:")
    );
    client.cancel().await.unwrap();
    assert_mcp_exit(process).await;
    let status = project.ok(&["status"]);
    assert_eq!(status["result"]["state"], "ready");
    assert_eq!(status["result"]["active_case"], active);
    assert_eq!(status["lab_id"], lab_id);
    assert_eq!(project.ok(&["down"])["result"]["state"], "stopped");
}
