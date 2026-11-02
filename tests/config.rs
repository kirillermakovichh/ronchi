use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn run(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ronchi"));
    for key in [
        "RONCHI_MODE",
        "RONCHI_READ_UPSTREAM",
        "RONCHI_SURFPOOL_DATASOURCE",
        "RONCHI_RPC_LISTEN",
        "RONCHI_CONTROL_LISTEN",
        "RONCHI_INITIAL_CASE",
        "RONCHI_SLOT_DURATION_MS",
    ] {
        cmd.env_remove(key);
    }
    cmd.current_dir(cwd)
        .args(args)
        .envs(env.iter().copied())
        .args(
            if args.first() == Some(&"up") && !args.contains(&"--case") {
                vec!["--case", "pending"]
            } else {
                vec![]
            },
        )
        .output()
        .unwrap()
}
fn config(root: &Path, text: &str) {
    std::fs::create_dir_all(root.join("ronchi")).unwrap();
    std::fs::write(root.join("ronchi/config.toml"), text).unwrap();
}
fn error(output: Output, exit: i32, code: &str) -> serde_json::Value {
    assert_eq!(output.status.code(), Some(exit), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["contract_version"], 1);
    assert_eq!(value["error"]["code"], code);
    assert_eq!(value["error"]["effects"], "none");
    assert!(value.get("result").is_none());
    value
}
#[test]
fn discovery_and_explicit_selection() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir(root.join(".git")).unwrap();
    let nested = root.join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    assert_eq!(
        ronchi::config::discover(None, &nested).unwrap(),
        root.canonicalize().unwrap()
    );
    config(&root.join("a"), "schema_version = 1\n");
    assert_eq!(
        ronchi::config::discover(None, &nested).unwrap(),
        root.join("a").canonicalize().unwrap()
    );
    assert_eq!(
        ronchi::config::discover(Some(root), &nested).unwrap(),
        root.canonicalize().unwrap()
    );
    config(&root.join("a"), "schema_version = 1\nunknown = true\n");
    error(run(&nested, &["up", "--json"], &[]), 2, "invalid_argument");
    error(
        run(
            &nested,
            &["up", "--project", root.to_str().unwrap(), "--json"],
            &[],
        ),
        4,
        "unsupported_capability",
    );
    assert!(!root.join("ronchi").exists());
}
#[test]
fn precedence_flags_environment_file_defaults() {
    let tmp = TempDir::new().unwrap();
    config(
        tmp.path(),
        "schema_version = 1\nmode = 'surfpool'\nsurfpool_datasource = 'https://example.test'\n",
    );
    error(
        run(tmp.path(), &["up", "--json"], &[]),
        4,
        "unsupported_capability",
    );
    error(
        run(
            tmp.path(),
            &["up", "--json"],
            &[("RONCHI_MODE", "simulator")],
        ),
        2,
        "invalid_argument",
    );
    error(
        run(
            tmp.path(),
            &["up", "--json", "--mode", "surfpool"],
            &[("RONCHI_MODE", "simulator")],
        ),
        4,
        "unsupported_capability",
    );
    let empty = TempDir::new().unwrap();
    error(
        run(empty.path(), &["up", "--json"], &[]),
        4,
        "unsupported_capability",
    );
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}
#[test]
fn invalid_configuration_and_parser_errors_are_clean() {
    let tmp = TempDir::new().unwrap();
    for text in [
        "schema_version = 2",
        "mode = 'simulator'",
        "schema_version = 1\nunknown = 2",
        "schema_version = 1\nrpc_listen = '0.0.0.0:8899'",
        "schema_version = 1\nread_upstream = 'secret'",
        "schema_version = 1\nmode = 'surfpool'\nread_upstream = 'https://example.test'",
    ] {
        config(tmp.path(), text);
        error(
            run(tmp.path(), &["up", "--json"], &[]),
            2,
            "invalid_argument",
        );
    }
    for args in [
        vec!["unknown", "--json"],
        vec!["up", "--json", "--rpc-listen", "bad"],
        vec!["up", "--mode", "wrong", "--json"],
    ] {
        error(run(tmp.path(), &args, &[]), 2, "invalid_argument");
    }
    let out = run(tmp.path(), &["up"], &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());
}
#[test]
fn introspection_never_reads_configuration() {
    let tmp = TempDir::new().unwrap();
    config(tmp.path(), "invalid configuration");
    for args in [vec!["--help"], vec!["--version"], vec!["schema", "lab.up"]] {
        let out = run(tmp.path(), &args, &[]);
        assert!(out.status.success(), "{out:?}");
        assert!(out.stderr.is_empty());
        assert!(!out.stdout.is_empty());
    }
    let out = run(tmp.path(), &["schema", "lab.up"], &[]);
    let schema: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(schema["implemented"], true);
    assert_eq!(
        schema["request"]["$defs"]["Settings"]["additionalProperties"],
        false
    );
}

#[test]
fn worktree_marker_fallback_and_invalid_explicit_paths() {
    let tmp = TempDir::new().unwrap();
    let nested = tmp.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    assert_eq!(
        ronchi::config::discover(None, &nested).unwrap(),
        nested.canonicalize().unwrap()
    );
    std::fs::write(tmp.path().join(".git"), "gitdir: /unused/worktree").unwrap();
    assert_eq!(
        ronchi::config::discover(None, &nested).unwrap(),
        tmp.path().canonicalize().unwrap()
    );
    error(
        run(&nested, &["up", "--project", "missing", "--json"], &[]),
        2,
        "invalid_argument",
    );
    error(
        run(&nested, &["up", "--project", "../.git", "--json"], &[]),
        2,
        "invalid_argument",
    );
    error(
        run(&nested, &["up", "--project", "..", "--json"], &[]),
        4,
        "unsupported_capability",
    );
}

#[test]
fn listener_validation_and_secret_redaction() {
    let tmp = TempDir::new().unwrap();
    error(
        run(
            tmp.path(),
            &["up", "--rpc-listen", "127.0.0.1:8900", "--json"],
            &[],
        ),
        2,
        "invalid_argument",
    );
    error(
        run(
            tmp.path(),
            &[
                "up",
                "--rpc-listen",
                "127.0.0.1:0",
                "--control-listen",
                "127.0.0.1:0",
                "--json",
            ],
            &[],
        ),
        4,
        "unsupported_capability",
    );
    let out = run(
        tmp.path(),
        &[
            "up",
            "--read-upstream",
            "https://user:secret@example.test",
            "--mode",
            "surfpool",
            "--json",
        ],
        &[],
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("secret"));
    error(out, 2, "invalid_argument");
}

#[test]
fn help_does_not_echo_environment_credentials() {
    let tmp = TempDir::new().unwrap();
    let output = run(
        tmp.path(),
        &["up", "--help"],
        &[(
            "RONCHI_READ_UPSTREAM",
            "https://user:secret@example.test/?token=private",
        )],
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(!help.contains("secret"));
    assert!(!help.contains("token=private"));
    assert!(help.contains("RONCHI_READ_UPSTREAM"));
}

#[test]
fn slot_profile_flags_override_environment_and_reject_invalid_effective_values() {
    let tmp = TempDir::new().unwrap();
    error(
        run(
            tmp.path(),
            &["up", "--json"],
            &[("RONCHI_SLOT_DURATION_MS", "0")],
        ),
        2,
        "invalid_argument",
    );
    error(
        run(
            tmp.path(),
            &["up", "--slot-duration-ms", "400", "--json"],
            &[("RONCHI_SLOT_DURATION_MS", "0")],
        ),
        4,
        "unsupported_capability",
    );
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
}
