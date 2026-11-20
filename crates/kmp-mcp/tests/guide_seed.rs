//! A fresh store gets its guide on the first guide read, from the assets
//! installed for this binary, without anyone typing `guide sync`.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn plugin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/kmp")
}

/// A server whose only knowledge of the machine is what the test gives it:
/// a scratch HOME, no launcher variable, an explicit store.
fn command(scratch: &Path, store: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"));
    command
        .env_remove("KMP_PLUGIN_ROOT")
        .env_remove("CODEX_HOME")
        .env("HOME", scratch.join("home"))
        .env("USERPROFILE", scratch.join("home"))
        .env("KMP_MCP_BACKEND", "embedded")
        .env("KMP_MCP_DATA_DIR", store)
        .env("XDG_CONFIG_HOME", scratch.join("home/.config"))
        .env("XDG_DATA_HOME", scratch.join("home/.local/share"))
        .env("KMP_VIEWER_ADDR", "off");
    command
}

fn call(mut command: Command, tool: &str, arguments: Value) -> Value {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("server");
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    writeln!(child.stdin.take().expect("stdin"), "{request}").expect("request");
    let output = child.wait_with_output().expect("server exits");
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice::<Value>(&output.stdout).expect("JSON")["result"].clone()
}

fn exported_events(scratch: &Path, store: &Path, name: &str) -> (u64, Vec<String>) {
    let output = scratch.join(name);
    let result = command(scratch, store)
        .arg("export")
        .arg(&output)
        .output()
        .expect("export");
    assert!(result.status.success(), "{result:?}");
    let text = std::fs::read_to_string(output).expect("bundle");
    let header: Value = serde_json::from_str(text.lines().next().expect("header")).expect("JSON");
    (
        header["event_count"].as_u64().expect("events"),
        header["abouts"]
            .as_array()
            .map(|abouts| {
                abouts
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    )
}

fn install_assets_under(root: &Path) {
    let guide = root.join("guide");
    std::fs::create_dir_all(&guide).expect("guide dir");
    for name in ["guide.requests.json", "memory.jsonl"] {
        std::fs::copy(plugin_root().join("guide").join(name), guide.join(name)).expect("asset");
    }
}

#[test]
fn the_launcher_variable_seeds_a_fresh_store_on_the_first_guide_read() {
    let scratch = tempfile::tempdir().expect("scratch");
    let store = scratch.path().join("store");
    let mut first = command(scratch.path(), &store);
    first.env("KMP_PLUGIN_ROOT", plugin_root());
    let opening = json!({"registration_key":"seed-agent","topic":"write"});
    let opened = call(first, "kmp_guide", opening.clone());
    assert_eq!(opened["isError"], false, "{opened}");
    assert!(opened["structuredContent"]["card"]["text"].is_string());
    let (events, abouts) = exported_events(scratch.path(), &store, "after.jsonl");
    assert_eq!(events, 2);
    assert_eq!(abouts, vec!["guide:kmp", "guide:kmp-agent"]);

    // Seeded once; the second session reads what is there and writes nothing.
    let mut second = command(scratch.path(), &store);
    second.env("KMP_PLUGIN_ROOT", plugin_root());
    let again = call(second, "kmp_guide", opening);
    assert_eq!(again["isError"], false, "{again}");
    assert_eq!(
        again["structuredContent"]["agent"],
        opened["structuredContent"]["agent"]
    );
    assert_eq!(exported_events(scratch.path(), &store, "retry.jsonl").0, 2);
}

#[test]
fn a_direct_read_of_the_human_guide_seeds_too() {
    let scratch = tempfile::tempdir().expect("scratch");
    let store = scratch.path().join("store");
    let mut session = command(scratch.path(), &store);
    session.env("KMP_PLUGIN_ROOT", plugin_root());
    let welcome = call(
        session,
        "kmp_inspect",
        json!({"about":"guide:kmp","ref":"guide:kmp:welcome"}),
    );
    assert_eq!(welcome["isError"], false, "{welcome}");
    assert_eq!(
        welcome["structuredContent"]["object"]["ref"],
        "guide:kmp:welcome"
    );
}

#[test]
fn the_host_plugin_cache_for_this_version_seeds_without_a_launcher() {
    let scratch = tempfile::tempdir().expect("scratch");
    let store = scratch.path().join("store");
    let cache = scratch
        .path()
        .join("home/.codex/plugins/cache/underpass/kmp")
        .join(env!("CARGO_PKG_VERSION"));
    install_assets_under(&cache);
    let opened = call(
        command(scratch.path(), &store),
        "kmp_guide",
        json!({"registration_key":"cache-agent"}),
    );
    assert_eq!(opened["isError"], false, "{opened}");
    assert_eq!(exported_events(scratch.path(), &store, "cache.jsonl").0, 2);
}

#[test]
fn without_matching_assets_the_repair_says_where_it_looked_and_writes_nothing() {
    let scratch = tempfile::tempdir().expect("scratch");
    let store = scratch.path().join("store");
    // Assets exist, but their bundle names another engine: never seeded.
    let stale = scratch.path().join("stale-plugin");
    install_assets_under(&stale);
    let bundle = stale.join("guide/memory.jsonl");
    let text = std::fs::read_to_string(&bundle).expect("bundle");
    let mut lines = text.lines();
    let mut header: Value =
        serde_json::from_str(lines.next().expect("header")).expect("header JSON");
    header["kernel_version"] = json!("0.0.1");
    let rest = lines.collect::<Vec<_>>().join("\n");
    std::fs::write(&bundle, format!("{header}\n{rest}\n")).expect("rewrite");

    let mut session = command(scratch.path(), &store);
    session.env("KMP_PLUGIN_ROOT", &stale);
    let missing = call(
        session,
        "kmp_guide",
        json!({"registration_key":"stale-agent","topic":"write"}),
    );
    assert_eq!(missing["isError"], true, "{missing}");
    let feedback = &missing["structuredContent"]["feedback"][0];
    assert_eq!(feedback["code"], "GUIDE_UNAVAILABLE");
    let searched = feedback["searched"].as_array().expect("searched");
    assert!(
        searched.iter().any(|line| {
            line.as_str()
                .is_some_and(|line| line.starts_with("KMP_PLUGIN_ROOT:") && line.contains("0.0.1"))
        }),
        "{searched:?}"
    );
    assert_eq!(
        feedback["repair"]["arguments"],
        json!(["guide", "sync", "--plugin-root", "<plugin-root>"])
    );
    assert_eq!(exported_events(scratch.path(), &store, "none.jsonl").0, 0);
}
