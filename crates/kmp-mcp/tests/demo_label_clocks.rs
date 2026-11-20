//! The shipped demo writes its labels without a clock. On the occurred and
//! observed clocks a label selector found nothing and ChronoLoom counted
//! every label `0/7` (V-01): a label with no time on the selected clock now
//! stands where its entry stands.
#[path = "support/isolated_home.rs"]
mod isolated_home;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn kmp(scratch: &Path) -> Command {
    let home = scratch.join("home");
    let mut command = isolated_home::command(env!("CARGO_BIN_EXE_kmp-mcp"));
    command
        .current_dir(scratch)
        .env_remove("KMP_MCP_BACKEND")
        .env_remove("KMP_PLUGIN_ROOT")
        .env("KMP_MCP_DATA_DIR", scratch.join("store"))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("CODEX_HOME", home.join(".codex"))
        .env("CLAUDE_CONFIG_DIR", home.join(".claude"))
        .env("HERMES_HOME", home.join(".hermes"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("KMP_VIEWER_ADDR", "off");
    command
}

/// Every reply of one stdio session that negotiated the MCP App surface.
fn session(scratch: &Path, calls: &[Value]) -> Vec<Value> {
    let mut lines = vec![
        json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{
            "protocolVersion":"2025-06-18",
            "capabilities":{"extensions":{"io.modelcontextprotocol/ui":{"mimeTypes":["text/html;profile=mcp-app"]}}},
            "clientInfo":{"name":"demo-label-clocks","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ];
    for (index, call) in calls.iter().enumerate() {
        lines.push(json!({"jsonrpc":"2.0","id":index + 1,"method":"tools/call","params":call}));
    }
    let mut child = kmp(scratch)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("kmp-mcp serves");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        for line in &lines {
            writeln!(stdin, "{line}").expect("request written");
        }
    }
    let output = child.wait_with_output().expect("session ends");
    let replies = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    (1..=calls.len())
        .map(|id| {
            let reply = replies
                .iter()
                .find(|reply| reply["id"] == json!(id))
                .unwrap_or_else(|| panic!("no reply to call {id}: {replies:?}"));
            assert_ne!(
                reply["result"]["isError"],
                json!(true),
                "call {id} failed: {reply}"
            );
            reply["result"]["structuredContent"].clone()
        })
        .collect()
}

fn time_with(axis: &str, selector: Value) -> Value {
    json!({"name":"kmp_time","arguments":{
        "about":"example:kmp-demo","move":"rewind",
        "from":{"time":"2100-01-01T00:00:00Z"},"axis":axis,
        "limit":{"entries":20},"budget":{"max_bytes":60000},
        "dimensions":{"selectors":[selector]}}})
}

fn projection(axis: &str, selectors: Value) -> Value {
    let mut call = json!({"name":"kmp_view_read_projection","arguments":{
        "about":"example:kmp-demo","from":"1900-01-01T00:00:00Z","to":"2100-01-01T00:00:00Z",
        "axis":axis,"lod":"episode"}});
    if selectors
        .as_array()
        .is_some_and(|selectors| !selectors.is_empty())
    {
        call["arguments"]["dimensions"] = json!({"selectors": selectors});
    }
    call
}

fn label_counts(projection: &Value) -> Vec<(String, u64, u64)> {
    let mut counts = projection["labels"]
        .as_array()
        .expect("labels")
        .iter()
        .map(|label| {
            (
                format!(
                    "{}={}",
                    label["dimension"].as_str().unwrap_or_default(),
                    label["value"].as_str().unwrap_or_default()
                ),
                label["in_range"].as_u64().unwrap_or_default(),
                label["entries"].as_u64().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    counts.sort();
    counts
}

#[test]
fn undated_demo_labels_are_found_and_counted_on_every_clock() {
    let scratch = tempfile::tempdir().expect("scratch");
    std::fs::create_dir_all(scratch.path().join("home")).expect("home");
    let demo = kmp(scratch.path())
        .args(["demo", "--no-viewer"])
        .output()
        .expect("demo runs");
    assert!(
        demo.status.success(),
        "{}",
        String::from_utf8_lossy(&demo.stderr)
    );

    let component = json!({"key":"component","op":"in","values":["checkout-api"]});
    let incident = json!({"key":"incident","op":"exists"});
    let mut calls = Vec::new();
    for axis in ["occurred", "observed", "ingested"] {
        calls.push(time_with(axis, component.clone()));
        calls.push(time_with(axis, incident.clone()));
        calls.push(projection(axis, json!([])));
        calls.push(projection(axis, json!([component.clone()])));
    }
    let replies = session(scratch.path(), &calls);

    for (index, axis) in ["occurred", "observed", "ingested"].into_iter().enumerate() {
        let [labelled, incidents, all, chosen] = &replies[index * 4..index * 4 + 4] else {
            unreachable!("four replies per clock")
        };
        assert_eq!(
            labelled["entries"].as_array().map(Vec::len),
            Some(7),
            "component in checkout-api on {axis}: {labelled}"
        );
        assert_eq!(
            incidents["entries"].as_array().map(Vec::len),
            Some(3),
            "incident exists on {axis}: {incidents}"
        );
        assert_eq!(
            label_counts(all),
            vec![
                ("component=checkout-api".to_string(), 7, 7),
                ("incident=INC-2041".to_string(), 3, 3),
                ("timeline=timeline".to_string(), 7, 7),
            ],
            "label counts on {axis}"
        );
        assert_eq!(
            chosen["page"]["total"],
            json!(7),
            "clicking checkout-api on {axis} keeps the scene: {chosen}"
        );
    }
}
