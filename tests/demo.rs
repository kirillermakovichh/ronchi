mod reference_support;
use reference_support::{base, command};
use serde_json::Value;
#[test]
fn resilient_demo_collects_public_proof_and_retains_only_its_lab() {
    let project = tempfile::tempdir().unwrap();
    let demo = command(project.path(), &["demo", "tx-response-lost"]);
    let result = &demo["result"];
    assert_eq!(result["check"]["verdict"], "pass", "{demo}");
    assert_eq!(result["check"]["definition_version"], 2);
    assert_eq!(result["reference_stopped"], true);
    assert_eq!(result["lab_name"], "demo");
    assert_eq!(result["lab_state"], "ready");
    assert!(
        ronchi::process::identity(result["reference_pid"].as_u64().unwrap() as u32)
            .unwrap()
            .is_none()
    );
    let id = result["lab_id"].as_str().unwrap();
    let snapshot = result["snapshot_seq"].as_u64().unwrap().to_string();
    let intent = result["intent_id"].as_str().unwrap();
    let replay = command(
        project.path(),
        &[
            "--lab-id",
            id,
            "check",
            "evaluate",
            "reference-intent",
            "--definition-version",
            "2",
            "--snapshot-seq",
            &snapshot,
            "--intent-id",
            intent,
        ],
    );
    assert_eq!(replay["result"], result["check"]);
    assert!(!project.path().join("ronchi/config.toml").exists());
    command(project.path(), &["--lab-id", id, "down"]);
    let stopped = command(
        project.path(),
        &[
            "--lab-id",
            id,
            "investigate",
            "summary",
            "--snapshot-seq",
            &snapshot,
        ],
    );
    assert!(!stopped["result"]["findings"].as_array().unwrap().is_empty());
    let output = base(project.path())
        .args([
            "demo",
            "tx-response-lost",
            "--profile",
            "unsafe",
            "--stop-after-demo",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let unsafe_demo: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        unsafe_demo["result"]["check"]["verdict"], "fail",
        "{unsafe_demo}"
    );
    assert_eq!(unsafe_demo["result"]["lab_name"], "demo-2");
    assert_eq!(unsafe_demo["result"]["lab_state"], "stopped");
    assert_ne!(
        unsafe_demo["result"]["reference_name"],
        result["reference_name"]
    );
}
