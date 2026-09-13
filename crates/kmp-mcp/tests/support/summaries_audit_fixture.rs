//! Shared embedded-server fixture for summaries-audit integration tests.

use std::io::Write;
use std::process::{Command, Stdio};

use kmp_mcp::{EmbeddedKernelMcpBackend, KernelMcpServer};
use serde_json::{Value, json};

pub const ABOUT: &str = "project:relleno";
pub const OTHER: &str = "project:almacen";

/// Two Spanish memories that owe a rendering, one English memory that needs
/// none, one rendering the lint refuses, and one that stands.
pub fn seed() -> Value {
    json!({
        "about": ABOUT,
        "idempotency_key": "audit:seed:1",
        "memory": {
            "dimensions": [{"id": "work:main", "kind": "work"}],
            "entries": [
                entry(
                    "decision:valkey",
                    "decision",
                    "Se adoptó Valkey 7.2 para el almacén compartido (ADR-018).",
                    None,
                    1,
                ),
                entry(
                    "observation:english",
                    "observation",
                    "The weekly meeting moved to ten in the morning.",
                    None,
                    2,
                ),
                entry(
                    "observation:auditores",
                    "observation",
                    "Los auditores pidieron el registro completo antes del jueves.",
                    Some("the record"),
                    3,
                ),
                entry(
                    "decision:ventana",
                    "decision",
                    "La ventana de despliegue se movió al martes por la tarde (#469).",
                    Some("The rollout window moved to Tuesday afternoon (#469)."),
                    4,
                ),
            ]
        }
    })
}

pub fn entry(suffix: &str, kind: &str, text: &str, summary: Option<&str>, sequence: u32) -> Value {
    let mut entry = json!({
        "id": format!("{ABOUT}:{suffix}"),
        "kind": kind,
        "text": text,
        "coordinates": [{
            "dimension": "work",
            "scope_id": "work:main",
            "occurred_at": format!("2026-05-06T{:02}:00:00Z", 9 + sequence),
            "sequence": sequence
        }]
    });
    if let Some(summary) = summary {
        entry["metadata"] = json!({"summary_en": summary, "summary_en_by": "agent:older"});
    }
    entry
}

pub async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let raw = server
        .handle_json_line(
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments}
            })
            .to_string(),
        )
        .await
        .unwrap_or_else(|| panic!("{name} answers"));
    serde_json::from_str::<Value>(&raw).expect("JSON-RPC")["result"].clone()
}

pub async fn audit(server: &KernelMcpServer, arguments: Value) -> Value {
    let result = call(server, "kmp_summaries_audit", arguments).await;
    assert_ne!(result["isError"], true, "{result}");
    result["structuredContent"].clone()
}

pub fn state_of<'a>(body: &'a Value, suffix: &str) -> &'a Value {
    body["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["ref"] == json!(format!("{ABOUT}:{suffix}")))
        .unwrap_or_else(|| panic!("{suffix} is in the reading"))
}

pub async fn seeded() -> (tempfile::TempDir, KernelMcpServer) {
    let store = tempfile::tempdir().expect("temporary store");
    let backend = EmbeddedKernelMcpBackend::open(store.path()).expect("embedded backend");
    let server = KernelMcpServer::with_embedded_backend(backend);
    let result = call(&server, "kmp_ingest", seed()).await;
    assert_ne!(result["isError"], true, "{result}");
    (store, server)
}

pub fn run(envs: &[(&str, &str)], args: &[&str], stdin: &str) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"));
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.env_remove("KMP_KERNEL_GRPC_ENDPOINT");
    for (name, value) in envs {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("the engine starts");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
        .expect("stdin is written");
    drop(child.stdin.take());
    child.wait_with_output().expect("the engine exits")
}
