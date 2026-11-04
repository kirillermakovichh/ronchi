use rmcp::{ClientHandler, ServiceExt, model::CallToolRequestParams};
use serde_json::{Value, json};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Clone)]
struct Client;
impl ClientHandler for Client {}

#[tokio::test]
async fn sdk_stdio_lists_and_calls_offline_catalog() {
    let project = tempfile::tempdir().unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .args([
            "--project",
            project.path().to_str().unwrap(),
            "mcp",
            "stdio",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
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
    assert_eq!(tools.len(), 13);
    let result = client
        .call_tool(
            CallToolRequestParams::new("ronchi_case_list")
                .with_arguments(json!({}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    let value: Value = result.structured_content.unwrap();
    assert_eq!(value["result"][0]["case_id"], "tx-response-lost");
    assert!(!project.path().join("ronchi").exists());
    client.cancel().await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), process.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::process::Command::new(env!("CARGO_BIN_EXE_ronchi"))
            .arg("--project")
            .arg(&self.0)
            .args(["down", "--force", "--json"])
            .output();
    }
}

#[tokio::test]
async fn sdk_operations_match_cli_and_eof_preserves_the_lab() {
    let project = tempfile::tempdir().unwrap();
    let _cleanup = Cleanup(project.path().into());
    let mut process = Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(project.path())
        .args(["mcp", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let client = Client
        .serve((
            process.stdout.take().unwrap(),
            process.stdin.take().unwrap(),
        ))
        .await
        .unwrap();
    async fn call(
        client: &rmcp::service::Peer<rmcp::RoleClient>,
        name: &'static str,
        args: Value,
    ) -> Value {
        let result = client
            .call_tool(
                CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        result.structured_content.unwrap()
    }
    let up = call(
        &client,
        "ronchi_lab_up",
        json!({"arguments":{"rpc_listen":"127.0.0.1:0","control_listen":"127.0.0.1:0"}}),
    )
    .await;
    let id = up["lab_id"].as_str().unwrap_or_else(|| panic!("{up}"));
    let invalid = client
        .call_tool(CallToolRequestParams::new("ronchi_case_disarm"))
        .await
        .unwrap();
    assert_eq!(invalid.is_error, Some(true));
    assert_eq!(
        invalid.structured_content.unwrap()["error"]["code"],
        "invalid_argument"
    );
    let invalid = call(
        &client,
        "ronchi_investigate_timeline",
        json!({"lab_id":id,"arguments":{"limit":201}}),
    )
    .await;
    assert_eq!(invalid["error"]["code"], "invalid_argument");
    let arm = call(
        &client,
        "ronchi_case_arm",
        json!({"lab_id":id,"arguments":{"case_id":"tx-response-lost","hold_ms":20}}),
    )
    .await;
    assert_eq!(arm["result"]["active_case"]["case_id"], "tx-response-lost");
    let status = call(&client, "ronchi_lab_status", json!({"name":"default"})).await;
    assert_eq!(status["lab_id"], id);
    let summary = call(
        &client,
        "ronchi_investigate_summary",
        json!({"lab_id":id,"arguments":{}}),
    )
    .await;
    let snapshot = summary["view"]["snapshot_seq"].as_u64().unwrap();
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(project.path())
        .args([
            "--lab-id",
            id,
            "--json",
            "investigate",
            "summary",
            "--snapshot-seq",
            &snapshot.to_string(),
        ])
        .output()
        .unwrap();
    assert!(cli.status.success(), "{cli:?}");
    let cli: Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(summary["result"], cli["result"]);
    assert_eq!(summary["view"], cli["view"]);
    client.cancel().await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), process.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(project.path())
        .args(["--lab-id", id, "--json", "status"])
        .output()
        .unwrap();
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["result"]["state"], "ready");
    assert_eq!(
        status["result"]["active_case"]["case_id"],
        "tx-response-lost"
    );
}

#[tokio::test]
async fn sdk_modern_discovery_has_the_same_catalog_and_schemas() {
    use rmcp::{ClientLifecycleMode, ClientServiceExt, model::ProtocolVersion};
    let project = tempfile::tempdir().unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(project.path())
        .args(["mcp", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let client = Client
        .serve_with_lifecycle(
            (
                process.stdout.take().unwrap(),
                process.stdin.take().unwrap(),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 13);
    let oversized = client
        .call_tool(
            CallToolRequestParams::new("ronchi_case_list").with_arguments(
                json!({"padding":"x".repeat(70*1024)})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(oversized.is_error, Some(true));
    assert_eq!(
        oversized.structured_content.unwrap()["error"]["code"],
        "invalid_argument"
    );
    let timeline = tools
        .iter()
        .find(|t| t.name == "ronchi_investigate_timeline")
        .unwrap();
    assert_eq!(
        timeline.input_schema["$defs"]["Arguments"]["properties"]["limit"]["maximum"],
        200
    );
    assert!(timeline.output_schema.is_some());
    assert_eq!(
        timeline.annotations.as_ref().unwrap().read_only_hint,
        Some(true)
    );
    let arm = tools.iter().find(|t| t.name == "ronchi_case_arm").unwrap();
    assert_eq!(
        arm.annotations.as_ref().unwrap().idempotent_hint,
        Some(false)
    );
    assert!(
        client
            .call_tool(CallToolRequestParams::new("unknown"))
            .await
            .is_err()
    );
    client.cancel().await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), process.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

#[tokio::test]
async fn cancellation_releases_startup_wait_and_eof_never_allocates_a_lab() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let project = tempfile::tempdir().unwrap();
    let _cleanup = Cleanup(project.path().into());
    let ws = ronchi::workspace::Workspace::new(project.path());
    ws.initialize().unwrap();
    let lock = ws.lock("default", "operation").unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(project.path())
        .args(["mcp", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = process.stdin.take().unwrap();
    let mut output = BufReader::new(process.stdout.take().unwrap()).lines();
    input.write_all(format!("{}\n",json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"ronchi-test","version":"1"}}})).as_bytes()).await.unwrap();
    let init: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    for message in [
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ronchi_lab_up","arguments":{"arguments":{"rpc_listen":"127.0.0.1:0","control_listen":"127.0.0.1:0"}}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"ping"}),
    ] {
        input
            .write_all(format!("{message}\n").as_bytes())
            .await
            .unwrap();
    }
    let ping: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(ping["id"], 3);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    input.write_all(format!("{}\n",json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":2,"reason":"stop waiting"}})).as_bytes()).await.unwrap();
    // Keep the resource lock held. Cancellation must finish without acquiring it.
    drop(input);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), process.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(
        std::fs::read_dir(ws.root.join("names"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(
        output.next_line().await.unwrap().is_none(),
        "Cancelled requests must not emit late responses"
    );
}

#[tokio::test]
async fn eof_after_sent_mutation_keeps_accepted_effect_and_can_be_inspected() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let project = tempfile::tempdir().unwrap();
    let _cleanup = Cleanup(project.path().into());
    let up = std::process::Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(project.path())
        .args([
            "up",
            "--detach",
            "--json",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ])
        .output()
        .unwrap();
    assert!(up.status.success(), "{up:?}");
    let up: Value = serde_json::from_slice(&up.stdout).unwrap();
    let id = up["lab_id"].as_str().unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(project.path())
        .args(["--json", "mcp", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = process.stdin.take().unwrap();
    let mut output = BufReader::new(process.stdout.take().unwrap()).lines();
    input.write_all(format!("{}\n",json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).as_bytes()).await.unwrap();
    let init: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
    assert!(init.get("result").is_some());
    for message in [
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"missing"}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ronchi_case_arm","arguments":{"lab_id":id,"arguments":{"case_id":"tx-response-lost","hold_ms":19}}}}),
    ] {
        input
            .write_all(format!("{message}\n").as_bytes())
            .await
            .unwrap();
    }
    // Close the attachment before receiving the mutation result. Do not retry.
    drop(input);
    let mut replies = std::collections::BTreeMap::new();
    while let Some(line) =
        tokio::time::timeout(std::time::Duration::from_secs(6), output.next_line())
            .await
            .unwrap()
            .unwrap()
    {
        let response: Value =
            serde_json::from_str(&line).expect("stdout contains only MCP messages");
        replies.insert(response["id"].as_u64().unwrap(), response);
    }
    assert!(
        replies[&2].get("error").is_some(),
        "Unknown tool is a protocol error"
    );
    assert!(replies[&3]["result"].get("structuredContent").is_some());
    assert!(process.wait().await.unwrap().success());
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(project.path())
        .args(["status", "--json", "--lab-id", id])
        .output()
        .unwrap();
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["result"]["state"], "ready");
    assert_eq!(status["result"]["active_case"]["hold_ms"], 19);
}

#[tokio::test]
async fn sdk_cancellation_settles_collection_and_allows_status_without_recollecting() {
    use rmcp::{
        model::{ClientRequest, Request},
        service::PeerRequestOptions,
    };
    use tokio::io::AsyncReadExt;
    let project = tempfile::tempdir().unwrap();
    let _cleanup = Cleanup(project.path().into());
    let source = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = source.local_addr().unwrap();
    let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
    let source_task = tokio::spawn(async move {
        let (mut stream, _) = source.accept().await.unwrap();
        let mut bytes = [0; 4096];
        assert!(stream.read(&mut bytes).await.unwrap() > 0);
        seen_tx.send(()).unwrap();
        while stream.read(&mut bytes).await.unwrap_or(0) > 0 {}
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), source.accept())
                .await
                .is_err()
        );
    });
    let mut process = Command::new(env!("CARGO_BIN_EXE_ronchi"))
        .arg("--project")
        .arg(project.path())
        .args(["mcp", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let client = Client
        .serve((
            process.stdout.take().unwrap(),
            process.stdin.take().unwrap(),
        ))
        .await
        .unwrap();
    let up_args = json!({"arguments":{"rpc_listen":"127.0.0.1:0","control_listen":"127.0.0.1:0","checks":{"sources":[{"check_id":"state","version":1,"source":"independent-app","schema_version":1,"url":format!("http://{address}/observe"),"facts":{"enabled":{"type":"boolean"}}}]}}});
    let up = client
        .call_tool(
            CallToolRequestParams::new("ronchi_lab_up")
                .with_arguments(up_args.as_object().unwrap().clone()),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
    let id = up["lab_id"].as_str().unwrap_or_else(|| panic!("{up}"));
    let arguments = json!({"lab_id":id,"arguments":{"collection_id":"mcp-collection","check_id":"state","scope":{"intent_ids":["W1"]},"poll":true,"horizon_seconds":60}});
    let handle = client
        .send_cancellable_request(
            ClientRequest::CallToolRequest(Request::new(
                CallToolRequestParams::new("ronchi_check_observe")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )),
            PeerRequestOptions::no_options(),
        )
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), seen_rx)
        .await
        .unwrap()
        .unwrap();
    handle.cancel(Some("stop collection".into())).await.unwrap();
    let mut status_args = arguments;
    status_args["action"] = json!("status");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let reply = client
            .call_tool(
                CallToolRequestParams::new("ronchi_check_observe")
                    .with_arguments(status_args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(reply.is_error, Some(false), "{reply:?}");
        let reply = reply.structured_content.unwrap();
        if reply["result"]["status"] != "running" {
            assert_eq!(reply["result"]["status"], "cancelled");
            assert_eq!(reply["result"]["invocations"], 1);
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "{reply}");
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    source_task.await.unwrap();
    client.cancel().await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), process.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

#[test]
fn stdio_startup_failures_never_print_cli_json() {
    let project = tempfile::tempdir().unwrap();
    for args in [
        vec!["--json", "--lab-id", "L1", "mcp", "stdio"],
        vec!["--json", "mcp", "stdio", "--unknown"],
    ] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_ronchi"))
            .arg("--project")
            .arg(project.path())
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty(), "{out:?}");
        assert!(!out.stderr.is_empty());
    }
}
