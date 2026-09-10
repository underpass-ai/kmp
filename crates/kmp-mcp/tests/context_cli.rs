//! Captured-context transforms must work without touching a memory backend.
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn run(directory: &std::path::Path, args: &[&str], input: &Value) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"))
        .args(args)
        .current_dir(directory)
        .env("KMP_MCP_BACKEND", "must-not-open")
        .env("KMP_MCP_DATA_DIR", directory.join("forbidden-store"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("context command");
    let mut stdin = child.stdin.take().expect("stdin");
    if !input.is_null() {
        stdin
            .write_all(input.to_string().as_bytes())
            .expect("input");
    }
    drop(stdin);
    child.wait_with_output().expect("context result")
}

#[test]
fn captured_packets_project_expand_and_reject_invalid_budgets_without_opening_a_store() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&scratch).expect("scratch");
    let dir = tempfile::tempdir_in(&scratch).expect("isolated context files");
    let text = "R7 is authorized only offline. R8 is not authorized. ".repeat(30);
    let groups = json!([{"id":"permission","packets":[{"object":{"ref":"source:A","text":text},"evidence":[{"id":"proof:A","text":text,"source":"independent:A","supports":["source:A"]}]}],"reads":[{"tool":"kmp_inspect","arguments":{"about":"project:spans","ref":"source:A"}}]}]);
    let input = json!({"groups":groups,"max_bytes":5000});
    let projected = run(dir.path(), &["context", "project"], &input);
    assert!(
        projected.status.success(),
        "{}",
        String::from_utf8_lossy(&projected.stderr)
    );
    let projected: Value = serde_json::from_slice(&projected.stdout).expect("projected JSON");
    assert_eq!(projected["contract"], "kmp.context.passages.v2");
    assert!(
        projected["passages"]
            .as_object()
            .is_some_and(|table| !table.is_empty())
    );
    let path = dir.path().join("context.json");
    std::fs::write(&path, projected.to_string()).expect("captured context");
    let expanded = run(
        dir.path(),
        &["context", "expand", path.to_str().expect("UTF8 path")],
        &Value::Null,
    );
    assert!(expanded.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&expanded.stdout).expect("expanded JSON"),
        groups
    );
    let invalid = run(
        dir.path(),
        &["context", "project"],
        &json!({"groups":groups,"max_bytes":"5000"}),
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    let unsupported = run(
        dir.path(),
        &["context", "expand"],
        &json!({"contract":"unknown"}),
    );
    assert_eq!(unsupported.status.code(), Some(2));
    assert!(!dir.path().join("forbidden-store").exists());
}
