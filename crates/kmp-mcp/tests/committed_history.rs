//! A committed `.kmp/memory.jsonl` a teammate wrote reaches this checkout's
//! store: a fresh clone and a pull read and write without a human step, a
//! divergence is told on every read, and nothing ever drops the teammate's
//! memory from the bundle (B1).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

/// Runs the binary inside `project` with a home of its own: nothing here
/// reaches the developer's real store, selection or plugin caches.
fn kmp(project: &Path, home: &Path, args: &[&str], stdin: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"));
    command
        .args(args)
        .current_dir(project)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("CODEX_HOME", home.join("codex"))
        .env("KMP_VIEWER_ADDR", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("kmp-mcp spawns");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("stdin written");
    child.wait_with_output().expect("kmp-mcp exits")
}

fn session(project: &Path, home: &Path, calls: &[(&str, Value)]) -> Vec<Value> {
    let mut input = String::new();
    for (index, (tool, arguments)) in calls.iter().enumerate() {
        input.push_str(
            &json!({"jsonrpc": "2.0", "id": index + 1, "method": "tools/call",
                "params": {"name": tool, "arguments": arguments}})
            .to_string(),
        );
        input.push('\n');
    }
    let output = kmp(project, home, &[], &input);
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON-RPC line"))
        .map(|reply| reply["result"]["structuredContent"].clone())
        .collect()
}

fn write(decision: &str) -> (&'static str, Value) {
    (
        "kmp_write_memory",
        json!({"about": "project:team", "actor": "test", "labels": {"topic": ["deploy"]},
            "idempotency_key": format!("decision:{decision}"),
            "memories": [{"id": "m", "kind": "decision",
                "summary": format!("Decision {decision}: the deploy window is {decision}."),
                "evidence": format!("Notes {decision}.")}]}),
    )
}

fn project(root: &Path, name: &str) -> std::path::PathBuf {
    let project = root.join(name);
    std::fs::create_dir_all(project.join(".git")).expect("project marker");
    project
}

fn bundle_events(project: &Path) -> u64 {
    let text = std::fs::read_to_string(project.join(".kmp/memory.jsonl")).expect("bundle");
    kmp_embedded::verify_bundle(&text)
        .expect("bundle verifies")
        .event_count
}

#[test]
fn a_clone_and_a_pull_keep_the_teammate_memory_and_a_divergence_is_told() {
    let root = tempfile::tempdir().expect("root");
    let home = root.path().join("home");
    let teammate = project(root.path(), "teammate");
    let replies = session(&teammate, &home, &[write("one"), write("two")]);
    assert!(replies.iter().all(|reply| reply["status"] == "committed"));

    // A fresh clone: the bundle, no store.
    let clone = project(root.path(), "clone");
    std::fs::create_dir_all(clone.join(".kmp")).expect("bundle dir");
    std::fs::copy(
        teammate.join(".kmp/memory.jsonl"),
        clone.join(".kmp/memory.jsonl"),
    )
    .expect("clone bundle");
    let replies = session(
        &clone,
        &home,
        &[
            (
                "kmp_ask",
                json!({"about": "project:team", "question": "What is the deploy window in decision two?"}),
            ),
            write("clone"),
        ],
    );
    assert_eq!(replies[0]["answer_status"], "answered", "{:?}", replies[0]);
    assert_eq!(replies[1]["status"], "committed", "{:?}", replies[1]);
    assert_eq!(bundle_events(&clone), 3);

    // A pull: the teammate pulls this clone's write, writes again, and this
    // checkout pulls; doctor names the bundle ahead and never advises
    // `export`, and export keeps it.
    std::fs::copy(
        clone.join(".kmp/memory.jsonl"),
        teammate.join(".kmp/memory.jsonl"),
    )
    .expect("teammate pulls");
    let replies = session(&teammate, &home, &[write("three")]);
    assert!(
        replies.iter().all(|reply| reply["status"] == "committed"),
        "{replies:?}"
    );
    std::fs::copy(
        teammate.join(".kmp/memory.jsonl"),
        clone.join(".kmp/memory.jsonl"),
    )
    .expect("pull");
    let doctor = kmp(&clone, &home, &["doctor"], "");
    let report = String::from_utf8_lossy(&doctor.stdout);
    assert!(report.contains("ahead of the live store"), "{report}");
    let durability = report
        .split("Durability")
        .nth(1)
        .unwrap_or_default()
        .split('▌')
        .next()
        .unwrap_or_default();
    assert!(!durability.contains("kmp-mcp export"), "{durability}");
    let export = kmp(&clone, &home, &["export"], "");
    assert!(export.status.success(), "{export:?}");
    assert_eq!(bundle_events(&clone), 4);

    // A divergence: both sides write from the same base.
    let replies = session(&clone, &home, &[write("mine-only")]);
    assert_eq!(replies[0]["status"], "committed", "{:?}", replies[0]);
    session(&teammate, &home, &[write("theirs-only")]);
    std::fs::copy(
        teammate.join(".kmp/memory.jsonl"),
        clone.join(".kmp/memory.jsonl"),
    )
    .expect("pull a branch");
    let replies = session(
        &clone,
        &home,
        &[
            ("kmp_wake", json!({"about": "project:team"})),
            write("after"),
        ],
    );
    let warnings = replies[0]["warnings"].to_string();
    assert!(warnings.contains("different histories"), "{warnings}");
    assert!(warnings.contains("kmp-mcp doctor"), "{warnings}");
    assert_eq!(replies[1]["status"], "unconfirmed", "{:?}", replies[1]);
    let refusal = replies[1]["error"]["message"].to_string();
    assert!(refusal.contains("--overwrite-committed"), "{refusal}");
    assert!(refusal.contains("mv "), "{refusal}");
    let refused = kmp(&clone, &home, &["export"], "");
    assert_eq!(refused.status.code(), Some(2), "{refused:?}");
    let committed = std::fs::read_to_string(clone.join(".kmp/memory.jsonl")).expect("bundle");
    assert!(
        committed.contains("theirs-only"),
        "export refused to drop it"
    );
}
