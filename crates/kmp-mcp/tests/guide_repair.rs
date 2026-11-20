#[path = "support/isolated_home.rs"]
mod isolated_home;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn plugin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/kmp")
}

/// The explicit repair is what this file proves, so the server must find no
/// installed assets on its own: no launcher variable, and a HOME that holds
/// no host plugin cache — the developer's real one must not seed a test store.
fn command(store: &Path) -> Command {
    let mut command = isolated_home::command(env!("CARGO_BIN_EXE_kmp-mcp"));
    command
        .env_remove("KMP_PLUGIN_ROOT")
        .env_remove("CODEX_HOME")
        .env("HOME", store.join("home"))
        .env("USERPROFILE", store.join("home"))
        .env("KMP_MCP_BACKEND", "embedded")
        .env("KMP_MCP_DATA_DIR", store)
        .env("XDG_CONFIG_HOME", store.join("config"))
        .env("XDG_DATA_HOME", store.join("home/.local/share"))
        .env("KMP_VIEWER_ADDR", "off");
    command
}

fn call(store: &Path, tool: &str, arguments: Value) -> Value {
    let mut child = command(store)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("server");
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    writeln!(child.stdin.take().expect("stdin"), "{request}").expect("request");
    let output = child
        .wait_with_output()
        .expect("server exits before repair");
    assert!(output.status.success(), "{:?}", output);
    serde_json::from_slice::<Value>(&output.stdout).expect("JSON")["result"].clone()
}

fn repair(store: &Path, rejected: &Value) {
    let feedback = &rejected["structuredContent"]["feedback"][0];
    assert_eq!(feedback["code"], "GUIDE_UNAVAILABLE");
    let action = &feedback["repair"];
    assert_eq!(action["command"], "kmp-mcp");
    let mut sync = command(store);
    for argument in action["arguments"].as_array().expect("repair arguments") {
        let argument = argument.as_str().expect("string argument");
        if argument == "<plugin-root>" {
            sync.arg(plugin_root());
        } else {
            sync.arg(argument);
        }
    }
    let output = sync.output().expect("explicit repair");
    assert!(output.status.success(), "{:?}", output);
}

fn exported_events(store: &Path, output: &Path) -> u64 {
    let result = command(store)
        .arg("export")
        .arg(output)
        .output()
        .expect("export");
    assert!(result.status.success(), "{:?}", result);
    let text = std::fs::read_to_string(output).expect("bundle");
    let header: Value = serde_json::from_str(text.lines().next().expect("header")).expect("JSON");
    header["event_count"].as_u64().expect("events")
}

#[test]
fn absent_or_partial_guide_repairs_in_the_same_store_and_retries_idempotently() {
    let requests: Vec<Value> = serde_json::from_str(include_str!(
        "../../../plugins/kmp/guide/guide.requests.json"
    ))
    .expect("assets");
    for case in ["absent", "human-only", "missing-card"] {
        let scratch = tempfile::tempdir().expect("scratch");
        let store = scratch.path().join("store");
        if case != "absent" {
            let about = if case == "human-only" {
                "guide:kmp"
            } else {
                "guide:kmp-agent"
            };
            let mut seed = requests
                .iter()
                .find(|r| r["about"] == about)
                .expect("guide")
                .clone();
            if case != "human-only" {
                seed["idempotency_key"] = json!(format!("guide-repair-{case}"));
                seed["memory"]["entries"]
                    .as_array_mut()
                    .expect("entries")
                    .retain(|e| e["id"] == "guide:kmp-agent:overview");
                seed["memory"]["evidence"]
                    .as_array_mut()
                    .expect("evidence")
                    .retain(|e| e["supports"] == json!(["guide:kmp-agent:overview"]));
                seed["memory"]["relations"] = json!([]);
            }
            assert_eq!(call(&store, "kmp_ingest", seed)["isError"], false);
        }
        let before = exported_events(&store, &scratch.path().join("before.jsonl"));
        let opening = json!({"about":"guide:kmp-agent","ref":"guide:kmp-agent:card:write"});
        let missing = call(&store, "kmp_inspect", opening.clone());
        assert_eq!(missing["isError"], true, "{case}: {missing}");
        assert_eq!(missing["structuredContent"]["error"]["code"], "not_found");
        assert!(
            !missing["content"][0]["text"]
                .as_str()
                .expect("text")
                .contains("Help:"),
            "no recursive guide help"
        );
        // The help pointer resolves on any store: the cards ship inside the
        // engine, so the refused call is taught before any repair.
        let refused = call(
            &store,
            "kmp_time",
            json!({"move":"rewind","about":"project:repair","limit":"bad"}),
        );
        let text = refused["content"][0]["text"].as_str().expect("text");
        assert!(
            text.ends_with(r#"Help: kmp_guide {"topic":"time"}"#),
            "{text}"
        );
        assert!(refused["structuredContent"].get("help").is_none());
        let card = call(&store, "kmp_guide", json!({"topic":"time"}));
        assert_eq!(card["isError"], false, "{card}");
        assert!(card["structuredContent"]["parameters"]["kmp_time"].is_array());
        assert_eq!(
            exported_events(&store, &scratch.path().join("read.jsonl")),
            before,
            "failed reads never seed guide memories"
        );
        repair(&store, &missing);
        let after = exported_events(&store, &scratch.path().join("after.jsonl"));
        repair(&store, &missing);
        assert_eq!(
            exported_events(&store, &scratch.path().join("retry.jsonl")),
            after
        );
        let opened = call(&store, "kmp_inspect", opening.clone());
        let retry = call(&store, "kmp_inspect", opening);
        assert_eq!(opened["isError"], false, "{opened}");
        assert_eq!(
            opened["structuredContent"]["object"],
            retry["structuredContent"]["object"]
        );
    }
}

#[test]
fn human_guide_has_the_same_repair_but_unrelated_missing_memory_does_not() {
    let scratch = tempfile::tempdir().expect("scratch");
    let store = scratch.path().join("store");
    let human = call(
        &store,
        "kmp_inspect",
        json!({"about":"guide:kmp","ref":"guide:kmp:welcome"}),
    );
    repair(&store, &human);
    let human = call(
        &store,
        "kmp_inspect",
        json!({"about":"guide:kmp","ref":"guide:kmp:welcome"}),
    );
    assert_eq!(human["isError"], false);
    let missing = call(
        &store,
        "kmp_inspect",
        json!({"about":"project:repair","ref":"project:repair:missing"}),
    );
    assert_eq!(missing["structuredContent"]["error"]["code"], "not_found");
    assert!(missing["structuredContent"].get("feedback").is_none());
}
