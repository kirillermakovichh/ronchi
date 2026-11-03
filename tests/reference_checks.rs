mod reference_support;
use reference_support::{Lab, base, command};
use ronchi::{
    checks::{Config, Scope, evaluate::Selection},
    query::{Arguments, Engine, Operation, Request},
};
use serde_json::Value;
use std::net::TcpListener;
use tokio_util::sync::CancellationToken;
fn configured() -> (Lab, std::net::SocketAddr) {
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserved.local_addr().unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("ronchi")).unwrap();
    #[derive(serde::Serialize)]
    struct Configuration {
        schema_version: u32,
        checks: Config,
    }
    let definition =
        ronchi::checks::reference::definition(format!("http://{address}/checks/observe"));
    definition.validate().unwrap();
    let text = toml::to_string(&Configuration {
        schema_version: 1,
        checks: Config {
            concurrency: 4,
            sources: vec![definition],
        },
    })
    .unwrap();
    std::fs::write(directory.path().join("ronchi/config.toml"), text).unwrap();
    let up = command(
        directory.path(),
        &[
            "up",
            "--case",
            "tx-response-lost",
            "--rpc-listen",
            "127.0.0.1:0",
            "--control-listen",
            "127.0.0.1:0",
        ],
    );
    let lab = Lab {
        directory,
        id: up["lab_id"].as_str().unwrap().into(),
        rpc: up["result"]["rpc_address"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
    };
    drop(reserved);
    (lab, address)
}
fn evaluate(lab: &Lab, snapshot: u64) -> Value {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let response = rt
        .block_on(
            Engine::default().execute(
                lab.path()
                    .join("ronchi/labs")
                    .join(&lab.id)
                    .join("journal.sqlite"),
                true,
                "explicit",
                Request {
                    contract_version: 1,
                    request_id: "reference-evaluation".into(),
                    lab_id: lab.id.clone(),
                    operation: Operation::Evaluate,
                    arguments: Arguments {
                        snapshot_seq: Some(snapshot),
                        evaluation: Some(Selection {
                            check_id: "reference-intent".into(),
                            definition_version: 1,
                            definition_digest: None,
                            scope: Scope {
                                intent_ids: vec!["W1".into()],
                                ..Default::default()
                            },
                        }),
                        ..Default::default()
                    },
                },
                CancellationToken::new(),
            ),
        )
        .unwrap();
    response.result
}
#[test]
fn real_reference_profiles_produce_evidence_backed_identity_and_finality_results() {
    for profile in ["resilient", "unsafe"] {
        let (lab, address) = configured();
        let mut relayer = lab.start_at(profile, None, None, None, Some(address));
        assert_eq!(
            relayer
                .request("POST", "/intents", reference_support::input())
                .0,
            200
        );
        let terminal = relayer.wait(|v| v["state"] == "succeeded");
        let observations =
            lab.command(&["check", "observe", "reference-intent", "--intent-id", "W1"]);
        assert_eq!(
            observations["result"]["status"], "completed",
            "{observations}"
        );
        let snapshot = observations["result"]["snapshot_seq"].as_u64().unwrap();
        let result = evaluate(&lab, snapshot);
        if profile == "resilient" {
            assert_eq!(result["verdict"], "pass", "{result}");
        } else {
            assert_eq!(result["verdict"], "fail", "{result}");
            assert_eq!(result["requirements"][0]["verdict"], "fail");
        }
        assert_eq!(result["requirements"][1]["verdict"], "pass", "{result}");
        assert_eq!(result["requirements"][2]["verdict"], "pass", "{result}");
        assert_eq!(terminal["prepared_set_complete"], true);
        relayer.stop();
        assert_eq!(evaluate(&lab, snapshot), result);
        assert!(
            base(lab.path())
                .args(["down"])
                .output()
                .unwrap()
                .status
                .success()
        );
        assert_eq!(evaluate(&lab, snapshot), result);
    }
}
