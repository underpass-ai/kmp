use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn plugin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/kmp")
}

fn command(store: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"));
    command
        .env("KMP_MCP_BACKEND", "embedded")
        .env("KMP_MCP_DATA_DIR", store)
        .env("XDG_CONFIG_HOME", store.join("config"))
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
    for case in ["absent", "human-only", "missing-card", "no-revision"] {
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
                if case == "no-revision" {
                    seed["memory"]["entries"][0]["metadata"]
                        .as_object_mut()
                        .expect("metadata")
                        .remove("guide_revision");
                }
            }
            assert_eq!(call(&store, "kmp_ingest", seed)["isError"], false);
        }
        let before = exported_events(&store, &scratch.path().join("before.jsonl"));
        let opening = json!({"registration_key":"repair-agent","topic":"write"});
        let missing = call(&store, "kmp_guide", opening.clone());
        assert_eq!(missing["isError"], true, "{case}: {missing}");
        assert_eq!(
            missing["structuredContent"]["error"]["code"],
            if case == "no-revision" {
                "conflict"
            } else {
                "not_found"
            }
        );
        assert!(
            missing["structuredContent"].get("help").is_none(),
            "no recursive guide help"
        );
        let refused = call(
            &store,
            "kmp_rewind",
            json!({"about":"project:repair","limit":"bad"}),
        );
        let help = &refused["structuredContent"]["help"];
        let actions = std::iter::once(help["guide"].clone())
            .chain(
                help["examples"]
                    .as_array()
                    .expect("examples")
                    .iter()
                    .cloned(),
            )
            .collect::<Vec<_>>();
        for action in &actions {
            let result = call(
                &store,
                action["tool"].as_str().expect("tool"),
                action["arguments"].clone(),
            );
            assert_eq!(
                result["structuredContent"]["feedback"][0]["code"],
                "GUIDE_UNAVAILABLE"
            );
            assert_eq!(
                result["structuredContent"]["feedback"][0]["ref"],
                action["arguments"]["ref"]
            );
            assert!(result["structuredContent"].get("help").is_none());
        }
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
        let opened = call(&store, "kmp_guide", opening.clone());
        let retry = call(&store, "kmp_guide", opening);
        assert_eq!(opened["isError"], false, "{opened}");
        assert_eq!(
            opened["structuredContent"]["agent"],
            retry["structuredContent"]["agent"]
        );
        assert_eq!(
            opened["structuredContent"]["context_id"],
            retry["structuredContent"]["context_id"]
        );
        for action in actions {
            let result = call(
                &store,
                action["tool"].as_str().expect("tool"),
                action["arguments"].clone(),
            );
            assert_eq!(result["isError"], false, "{result}");
            assert_eq!(
                result["structuredContent"]["object"]["ref"],
                action["arguments"]["ref"]
            );
        }
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
