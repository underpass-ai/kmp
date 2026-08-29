use std::process::Command;

#[test]
fn lifecycle_http_adapter_never_drops_a_nested_runtime_on_the_async_cli() {
    let scratch = tempfile::tempdir().expect("scratch home");
    let output = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"))
        .args(["setup", "--dry-run", "--version", "0.5.1"])
        .env("HOME", scratch.path())
        .env("PATH", "")
        .output()
        .expect("lifecycle CLI starts");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "nested runtime panic: {stderr}"
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("machine-readable failure receipt");
    assert_eq!(receipt["action"], "setup");
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["failed_component"], "host_inventory");
}
