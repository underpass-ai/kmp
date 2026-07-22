use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::Value;

const TLS_ENV_VARS: &[&str] = &[
    "REHYDRATION_MCP_BACKEND",
    "REHYDRATION_KERNEL_GRPC_ENDPOINT",
    "REHYDRATION_KERNEL_GRPC_TLS_MODE",
    "REHYDRATION_KERNEL_GRPC_TLS_CA_PATH",
    "REHYDRATION_KERNEL_GRPC_TLS_CERT_PATH",
    "REHYDRATION_KERNEL_GRPC_TLS_KEY_PATH",
    "REHYDRATION_KERNEL_GRPC_TLS_DOMAIN_NAME",
];

#[test]
fn stdio_binary_fails_fast_without_backend_configuration() {
    let output = run_binary(&[], "");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("REHYDRATION_KERNEL_GRPC_ENDPOINT is required"));
}

#[test]
fn stdio_binary_serves_explicit_fixture_jsonrpc_until_stdin_eof() {
    let output = run_binary(
        &[("REHYDRATION_MCP_BACKEND", "fixture")],
        "\n\
         {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n\
         {\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\",\"params\":{}}\n\
         {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n",
    );

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("using explicit fixture backend"));

    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let responses = stdout
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("stdout line should be JSON"))
        .collect::<Vec<_>>();

    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[0]["result"]["metadata"]["backend"], "fixture");
    assert_eq!(responses[1]["id"], 2);
    assert_eq!(
        responses[1]["result"]["tools"][0]["name"],
        Value::String("kernel_ingest".to_string())
    );
}

#[test]
fn stdio_binary_reports_live_grpc_backend_without_tls() {
    let output = run_binary(
        &[("REHYDRATION_KERNEL_GRPC_ENDPOINT", "http://127.0.0.1:1")],
        "",
    );

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("using live gRPC backend"));
    assert!(stderr.contains("REHYDRATION_KERNEL_GRPC_TLS_MODE=disabled"));
    assert!(!stderr.contains("TLS envs:"));
}

#[test]
fn stdio_binary_reports_live_grpc_backend_with_tls_envs() {
    let output = run_binary(
        &[(
            "REHYDRATION_KERNEL_GRPC_ENDPOINT",
            "https://rehydration-kernel.example.test",
        )],
        "",
    );

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("using live gRPC backend"));
    assert!(stderr.contains("REHYDRATION_KERNEL_GRPC_TLS_MODE=server"));
    assert!(stderr.contains("TLS envs:"));
}

fn run_binary(envs: &[(&str, &str)], stdin: &str) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rehydration-mcp"));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for name in TLS_ENV_VARS {
        command.env_remove(name);
    }
    for (name, value) in envs {
        command.env(name, value);
    }

    let mut child = command.spawn().expect("stdio MCP binary should spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(stdin.as_bytes())
        .expect("stdin should be written");
    drop(child.stdin.take());

    child
        .wait_with_output()
        .expect("stdio MCP binary should exit after stdin EOF")
}

#[test]
fn embedded_backend_serves_initialize_and_journals_logs_in_data_dir() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let output = run_binary(
        &[
            ("REHYDRATION_MCP_BACKEND", "embedded"),
            (
                "REHYDRATION_MCP_DATA_DIR",
                data_dir.path().to_str().expect("utf8 path"),
            ),
            ("RUST_LOG", "rehydration_mcp=info"),
        ],
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(
        stdout.contains("\"backend\":\"embedded\""),
        "initialize must report the embedded backend: {stdout}"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(
        stderr.contains("embedded backend (kernel in-process)"),
        "banner must announce embedded mode: {stderr}"
    );

    let log_entries = std::fs::read_dir(data_dir.path().join("logs"))
        .expect("logs dir exists in the data dir")
        .count();
    assert!(
        log_entries > 0,
        "embedded mode must journal logs into <data-dir>/logs/"
    );
}
