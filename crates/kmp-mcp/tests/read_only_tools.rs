//! The annotations `tools/list` publishes are true of the binary.
//!
//! A tool marked `readOnlyHint: true` leaves the store and its sidecars byte
//! for byte as they were: every file under the scratch home is hashed after
//! a session that seeded memory and closed, and again after a session that
//! made only that tool's call and closed. The only files such a call may
//! leave changed are diagnostics no tool reads back as memory: the server
//! log, the quality telemetry journal and its salt, the SQLite shared-memory
//! index of a database it opened, and the continuation handle store when, and
//! only when, the answer carried a handle.
//!
//! A writer marked `idempotentHint: true` changes nothing more the second
//! time it receives the same call, by the same measure. Inspect is read-only
//! too: a guide node the store lacks is answered with the repair, never
//! seeded, and a test holds it to that. `openWorldHint` is true only on the
//! tools a store's TypeSafe Jev opt-in actually sends text from.
#[path = "support/isolated_home.rs"]
mod isolated_home;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const ABOUT: &str = "proj:ro";

fn command(scratch: &Path) -> Command {
    let mut command = isolated_home::command(env!("CARGO_BIN_EXE_kmp-mcp"));
    command
        .env("HOME", scratch.join("home"))
        .env("XDG_DATA_HOME", scratch.join("home/.local/share"))
        .env("XDG_CONFIG_HOME", scratch.join("home/.config"))
        .env("KMP_MCP_BACKEND", "embedded")
        .env("KMP_MCP_DATA_DIR", scratch.join("store"))
        .env("KMP_VIEWER_ADDR", "off");
    command
}

/// One session: initialize, the calls in order, then stdin closes and the
/// server exits. Returns the `tools/call` replies in order, and `tools/list`
/// first when `list` is set.
fn session(scratch: &Path, calls: &[(&str, Value)]) -> Vec<Value> {
    session_with(scratch, &[], calls)
}

/// `session` with extra environment variables.
fn session_with(scratch: &Path, env: &[(&str, &str)], calls: &[(&str, Value)]) -> Vec<Value> {
    let mut command = command(scratch);
    for (name, value) in env {
        command.env(name, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("server");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut lines = vec![
        json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{
        "protocolVersion":"2025-06-18","capabilities":{},
        "clientInfo":{"name":"read-only-audit","version":"0"}}}),
    ];
    for (n, (tool, arguments)) in calls.iter().enumerate() {
        lines.push(if *tool == "tools/list" {
            json!({"jsonrpc":"2.0","id":n + 1,"method":"tools/list"})
        } else {
            json!({"jsonrpc":"2.0","id":n + 1,"method":"tools/call",
                   "params":{"name":tool,"arguments":arguments}})
        });
    }
    for line in lines {
        writeln!(stdin, "{line}").expect("request");
    }
    drop(stdin);
    let output = child.wait_with_output().expect("server exits");
    assert!(output.status.success(), "{output:?}");
    let mut replies = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let reply: Value = serde_json::from_str(line).expect("JSON reply");
        if let Some(id) = reply["id"].as_u64()
            && id > 0
        {
            replies.insert(id, reply);
        }
    }
    replies.into_values().collect()
}

/// Every file under `root` with the SHA-256 of its bytes.
fn snapshot(root: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let relative = path.strip_prefix(root).expect("below root");
                out.insert(
                    relative.to_string_lossy().into_owned(),
                    format!("{:x}", Sha256::digest(&bytes)),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn changed(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Vec<String> {
    let mut names = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter(|name| before.get(name) != after.get(name))
        .collect()
}

/// A file a call may leave changed without changing memory.
fn diagnostic(path: &str) -> bool {
    path.contains("/logs/")
        || path.starts_with("logs/")
        || path.contains("telemetry")
        || path.ends_with("-shm")
}

fn continuation_store(path: &str) -> bool {
    path.contains("continuations.sqlite3")
}

fn carries_handle(reply: &Value) -> bool {
    let text = reply.to_string();
    text.match_indices("read_").any(|(at, _)| {
        text[at + 5..]
            .chars()
            .take(32)
            .filter(char::is_ascii_hexdigit)
            .count()
            == 32
    })
}

fn ok(reply: &Value) -> &Value {
    assert!(reply.get("error").is_none(), "{reply}");
    assert_ne!(reply["result"]["isError"], true, "{reply}");
    &reply["result"]["structuredContent"]
}

/// A store with a few linked memories, written and closed. Returns two refs.
fn seeded(scratch: &Path) -> (String, String) {
    let memories = (0..6)
        .map(|n| {
            json!({"id": format!("m{n}"), "kind": "observation",
                "summary": format!("Deploy {n} of the ledger service failed because disk {n} was full."),
                "evidence": format!("Ops log line {n}: disk full during deploy.")})
        })
        .collect::<Vec<_>>();
    let replies = session(
        scratch,
        &[(
            "kmp_write_memory",
            json!({"about": ABOUT, "actor": "audit", "labels": {"task": ["t1"]},
                   "observed_at": "2026-11-20T10:00:00Z", "memories": memories}),
        )],
    );
    let refs = &ok(&replies[0])["local_refs"];
    let first = refs["m0"].as_str().expect("ref").to_string();
    let second = refs["m1"].as_str().expect("ref").to_string();
    let link = json!({"about": ABOUT, "actor": "audit", "relations": [{"from": first,
        "to": second, "rel": "supports", "class": "evidential",
        "why": "Both deploys failed on a full disk.", "evidence": "Ops log lines 0 and 1."}]});
    let reply = &session(scratch, &[("kmp_write_memory", link)])[0];
    let content = ok(reply);
    if content["accepted"] != true {
        let handle = reply.to_string();
        let at = handle.find("read_").expect("a needs_review continuation");
        let continuation = &handle[at..at + 37];
        let resumed = &session(
            scratch,
            &[("kmp_write_memory", json!({"continuation": continuation}))],
        )[0];
        assert_eq!(ok(resumed)["accepted"], true, "{resumed}");
    }
    (first, second)
}

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("scratch");
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("home")).expect("home");
    (dir, root)
}

/// The call each read-only tool is audited with; a tool marked read-only
/// without a case here fails the test.
fn read_case(tool: &str, first: &str, second: &str) -> Vec<(&'static str, Value)> {
    match tool {
        "kmp_wake" => vec![(
            "kmp_wake",
            json!({"about": ABOUT, "budget": {"max_bytes": 900}}),
        )],
        "kmp_ask" => vec![(
            "kmp_ask",
            json!({"about": ABOUT, "question": "Why did the deploys of the ledger service fail?"}),
        )],
        "kmp_time" => vec![(
            "kmp_time",
            json!({"about": ABOUT, "move": "rewind", "from": {"time": "2030-01-01T00:00:00Z"},
                   "budget": {"max_bytes": 900}}),
        )],
        "kmp_trace" => vec![(
            "kmp_trace",
            json!({"about": ABOUT, "from": first, "to": second}),
        )],
        "kmp_relate" => vec![("kmp_relate", json!({"about": ABOUT}))],
        "kmp_inspect" => vec![("kmp_inspect", json!({"about": ABOUT, "ref": first}))],
        "kmp_summaries_audit" => vec![("kmp_summaries_audit", json!({"about": ABOUT}))],
        "kmp_guide" => vec![("kmp_guide", json!({"topic": "write"}))],
        "kmp_view_get_state" => vec![
            ("kmp_view_open", json!({"about": ABOUT})),
            ("kmp_view_get_state", json!({})),
        ],
        other => panic!("`{other}` is marked read-only and has no audit case: add one"),
    }
}

#[test]
fn a_tool_marked_read_only_leaves_the_store_and_its_sidecars_byte_identical() {
    let (_dir, root) = scratch();
    let (first, second) = seeded(&root);
    let listed = &session(&root, &[("tools/list", Value::Null)])[0];
    let reads = listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter(|tool| tool["annotations"]["readOnlyHint"] == true)
        .map(|tool| tool["name"].as_str().expect("name").to_string())
        .collect::<Vec<_>>();
    assert!(reads.len() >= 8, "{reads:?}");
    let mut report = Vec::new();
    for tool in &reads {
        let before = snapshot(&root);
        let replies = session(&root, &read_case(tool, &first, &second));
        let reply = replies.last().expect("the read's reply");
        ok(reply);
        let after = snapshot(&root);
        let changed = changed(&before, &after);
        let memory = changed
            .iter()
            .filter(|path| !diagnostic(path) && !continuation_store(path))
            .cloned()
            .collect::<Vec<_>>();
        assert!(memory.is_empty(), "{tool} changed memory files: {memory:?}");
        let handle = replies.iter().any(carries_handle);
        if changed.iter().any(|path| continuation_store(path)) {
            assert!(
                handle,
                "{tool} wrote the continuation store without returning a handle"
            );
        }
        report.push(format!("{tool}: handle={handle} changed={changed:?}"));
    }
    eprintln!("read-only audit:\n{}", report.join("\n"));
}

#[test]
fn a_writer_marked_idempotent_changes_nothing_the_second_time() {
    let (_dir, root) = scratch();
    let (first, _) = seeded(&root);
    let listed = &session(&root, &[("tools/list", Value::Null)])[0];
    let idempotent = listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter(|tool| tool["annotations"]["idempotentHint"] == true)
        .map(|tool| tool["name"].as_str().expect("name").to_string())
        .collect::<Vec<_>>();
    for tool in &idempotent {
        let call: Vec<(&str, Value)> = match tool.as_str() {
            "kmp_write_memory" => vec![(
                "kmp_write_memory",
                json!({"about": ABOUT, "actor": "audit", "labels": {"task": ["t2"]},
                       "observed_at": "2026-11-20T11:00:00Z",
                       "memories": [{"id": "again", "kind": "observation",
                           "summary": "The ledger service deployed on the second try.",
                           "evidence": "Ops log: deploy ok."}]}),
            )],
            "kmp_ingest" => vec![(
                "kmp_ingest",
                json!({"about": ABOUT, "idempotency_key": "ro-ingest",
                    "memory": {"dimensions": [{"id": "task:t3", "kind": "task"}],
                        "entries": [{"id": format!("{ABOUT}:ingested"), "kind": "observation",
                            "text": "The ledger disk was replaced.",
                            "coordinates": [{"dimension": "task", "scope_id": "task:t3"}]}]},
                    "provenance": {"source_kind": "agent", "source_agent": "audit",
                                   "observed_at": "2026-11-20T12:00:00Z"}}),
            )],
            "kmp_relabel" => vec![(
                "kmp_relabel",
                json!({"about": ABOUT, "ref": first, "actor": "audit",
                       "observed_at": "2026-11-20T13:00:00Z", "why": "incident triage",
                       "add": {"incident": ["i1"]}, "options": {"labels_new": ["incident"]}}),
            )],
            "kmp_view_apply_intent" => vec![
                ("kmp_view_open", json!({"about": ABOUT})),
                (
                    "kmp_view_apply_intent",
                    json!({"idempotency_key": "ro-view", "search": "disk"}),
                ),
                (
                    "kmp_view_apply_intent",
                    json!({"idempotency_key": "ro-view", "search": "disk"}),
                ),
            ],
            other => panic!("`{other}` is marked idempotent and has no audit case: add one"),
        };
        let replies = session(&root, &call);
        for reply in &replies {
            ok(reply);
        }
        if tool == "kmp_view_apply_intent" {
            // The view lives in the session: the replay lands on the same
            // revision and reports that nothing moved.
            let (first, again) = (ok(&replies[1]), ok(&replies[2]));
            assert_eq!(again["applied"], false, "{again}");
            assert_eq!(first["view_revision"], again["view_revision"], "{again}");
            continue;
        }
        let once = snapshot(&root);
        let replies = session(&root, &call);
        let _ = replies.last().map(|reply| reply.to_string());
        let twice = snapshot(&root);
        let memory = changed(&once, &twice)
            .into_iter()
            .filter(|path| !diagnostic(path) && !continuation_store(path))
            .collect::<Vec<_>>();
        assert!(
            memory.is_empty(),
            "{tool} wrote again on a repeat: {memory:?}"
        );
    }
}

#[test]
fn inspect_of_a_guide_node_the_store_lacks_writes_nothing() {
    let (_dir, root) = scratch();
    seeded(&root);
    let before = snapshot(&root);
    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/kmp");
    let mut child = command(&root)
        .env("KMP_PLUGIN_ROOT", plugin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("server");
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":"kmp_inspect","arguments":{"about":"guide:kmp-agent","ref":"guide:kmp-agent:card:write"}}});
    writeln!(child.stdin.take().expect("stdin"), "{request}").expect("request");
    let output = child.wait_with_output().expect("server exits");
    let reply: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    // The store has no guide and the read does not install one: it answers
    // with the repair, which names kmp_guide, and leaves every byte as it was
    // even with the plugin's assets in reach.
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let message = reply["result"]["structuredContent"]["error"]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(message.contains("kmp_guide"), "{message}");
    let memory = changed(&before, &snapshot(&root))
        .into_iter()
        .filter(|path| !diagnostic(path) && !continuation_store(path))
        .collect::<Vec<_>>();
    assert!(
        memory.is_empty(),
        "a guide read wrote the store: {memory:?}"
    );
    let listed = &session(&root, &[("tools/list", Value::Null)])[0];
    let inspect = listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "kmp_inspect")
        .expect("inspect");
    assert_eq!(inspect["annotations"]["readOnlyHint"], true);
}

/// The five tools a store's TypeSafe Jev opt-in can send text from.
const JEV_TOOLS: [&str; 5] = [
    "kmp_wake",
    "kmp_ask",
    "kmp_write_memory",
    "kmp_curate",
    "kmp_summaries_audit",
];

fn open_world(listed: &Value) -> BTreeMap<String, bool> {
    listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| {
            (
                tool["name"].as_str().expect("name").to_string(),
                tool["annotations"]["openWorldHint"] == true,
            )
        })
        .collect()
}

#[test]
fn without_a_jev_opt_in_no_tool_is_open_world() {
    let (_dir, root) = scratch();
    seeded(&root);
    let hints = open_world(&session(&root, &[("tools/list", Value::Null)])[0]);
    assert!(hints.len() >= 16, "{hints:?}");
    assert!(hints.values().all(|open| !open), "{hints:?}");
}

#[test]
fn a_store_opted_into_jev_marks_the_tools_that_send_text_open_world() {
    let (_dir, root) = scratch();
    seeded(&root);
    let store = root.join("store");
    std::fs::write(
        store.join("typesafe.json"),
        r#"{"endpoint":"https://api.typesafe.ai/v1/judge","model":"jev-1.13.0","timeout_ms":5000}"#,
    )
    .expect("typesafe.json");
    for file in ["wake-focus.json", "rerank.json"] {
        std::fs::write(store.join(file), r#"{"pool_size": 8}"#).expect(file);
    }
    std::fs::write(store.join("write-relations.json"), "{}").expect("write-relations.json");
    let listed = &session_with(
        &root,
        &[("TYPESAFE_API_KEY", "test-key-not-used")],
        &[("tools/list", Value::Null)],
    )[0];
    for (tool, open) in open_world(listed) {
        assert_eq!(open, JEV_TOOLS.contains(&tool.as_str()), "{tool}");
    }
    // typesafe.json alone: only the tools that read through Jev without a
    // file of their own.
    for file in ["wake-focus.json", "rerank.json", "write-relations.json"] {
        std::fs::remove_file(store.join(file)).expect(file);
    }
    let listed = &session_with(
        &root,
        &[("TYPESAFE_API_KEY", "test-key-not-used")],
        &[("tools/list", Value::Null)],
    )[0];
    for (tool, open) in open_world(listed) {
        assert_eq!(
            open,
            matches!(tool.as_str(), "kmp_curate" | "kmp_summaries_audit"),
            "{tool}"
        );
    }
}
