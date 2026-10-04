//! The session names the store it opened (#903): in the `initialize`
//! instructions, and on every wake — including the "not found" that is
//! meaningless without it.

#[path = "support/isolated_home.rs"]
mod isolated_home;

use std::io::Write;
use std::process::Stdio;

use serde_json::{Value, json};

fn serve(working_dir: &std::path::Path, root: &std::path::Path, input: &[Value]) -> Vec<Value> {
    let mut child = isolated_home::command(env!("CARGO_BIN_EXE_kmp-mcp"))
        .current_dir(working_dir)
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("KMP_MCP_BACKEND", "embedded")
        .env("KMP_VIEWER_ADDR", "off")
        .env_remove("KMP_MCP_DATA_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    let lines: String = input.iter().map(|line| format!("{line}\n")).collect();
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(lines.as_bytes())
        .expect("requests written");
    let output = child.wait_with_output().expect("the session ends at EOF");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON-RPC response"))
        .collect()
}

fn wake(id: u64, about: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": "kmp_wake", "arguments": {"about": about}}})
}

#[test]
fn the_session_names_its_store_at_initialize_and_on_every_wake() {
    let root = tempfile::tempdir().expect("machine");
    let project = root.path().join("project");
    std::fs::create_dir_all(project.join(".git")).expect("project marker");
    let store = project.join(".kernel");

    let write = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "name": "kmp_write_memory",
        "arguments": {
            "about": "project:disclosed",
            "actor": "agent:test",
            "observed_at": "2026-10-04T12:00:00Z",
            "idempotency_key": "disclosure:one",
            "options": {"strict": false},
            "labels": {"agentic_process": ["project:disclosed:process"]},
            "memories": [{
                "id": "one",
                "ref": "project:disclosed:observation:one",
                "kind": "observation",
                "summary": "The store is named to the agent.",
                "evidence": "Regression fixture for #903."
            }]
        }
    }});
    let responses = serve(
        &project,
        root.path(),
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
            write,
            wake(3, "project:disclosed"),
            wake(4, "project:elsewhere"),
        ],
    );
    assert_eq!(responses.len(), 4, "{responses:?}");
    assert_eq!(responses[1]["result"]["isError"], false, "{}", responses[1]);

    let instructions = responses[0]["result"]["instructions"]
        .as_str()
        .expect("instructions");
    assert!(
        instructions.contains(&format!(
            "Memory store: {} (chosen by project)",
            store.display()
        )),
        "{instructions}"
    );

    let expected = json!({"path": store.display().to_string(), "rule": "project"});
    let found = &responses[2]["result"];
    assert_eq!(found["isError"], false, "{found}");
    assert_eq!(found["structuredContent"]["store"], expected);

    // The answer that misled four sessions: nothing here. It must say where
    // "here" is, in the text a host shows as well as in the structure.
    let missing = &responses[3]["result"];
    assert_eq!(missing["isError"], true, "{missing}");
    assert_eq!(missing["structuredContent"]["store"], expected);
    let text = missing["content"][0]["text"].as_str().expect("text");
    assert!(
        text.starts_with(&format!(
            "Memory store: {} (chosen by project).\n",
            store.display()
        )),
        "{text}"
    );
}
