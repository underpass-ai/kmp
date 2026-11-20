//! Guard for #910: a binary spawned by a test never touches the home of the
//! developer who runs the tests.
//!
//! The guard builds a sentinel developer home that holds what a real one
//! holds — plugin caches a serve start would prune, a saved store
//! selection, a machine index, a store exported in `KMP_MCP_DATA_DIR` — and
//! points every location variable at it, including the ones that live
//! outside `HOME` (`CODEX_HOME`, `XDG_CACHE_HOME`, `HERMES_HOME`, ...). It
//! then reruns this test binary in that environment and lets the probe
//! spawn `kmp-mcp` through `isolated_home::command` in every shape the test
//! suites use. The sentinel must be byte-identical afterwards.
//!
//! The second check keeps the guard honest about coverage: every test
//! source that names the `kmp-mcp` binary spawns it through the helper.

#[path = "support/isolated_home.rs"]
mod isolated_home;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PROBE: &str = "KMP_TEST_HOME_GUARD_PROBE";
const TOOLS_LIST: &str = concat!(
    r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"guard","version":"0"}}}"#,
    "\n",
    r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#,
    "\n"
);

#[test]
fn spawned_binaries_leave_the_developer_home_untouched() {
    if std::env::var_os(PROBE).is_some() {
        return;
    }
    let developer = tempfile::tempdir().expect("sentinel developer home");
    let home = developer.path();
    for cache in [".claude/plugins/cache", "codex/plugins/cache"] {
        for version in ["0.1.0", "0.2.0", "0.3.0"] {
            let skills = home.join(cache).join("underpass/kmp").join(version);
            std::fs::create_dir_all(skills.join("skills")).expect("plugin cache version");
        }
    }
    let selected = home.join("selected-store");
    let exported = home.join("exported-store");
    for store in [&selected, &exported] {
        std::fs::create_dir_all(store).expect("store");
        std::fs::write(store.join("NOTE"), "the developer's memory\n").expect("store note");
    }
    std::fs::create_dir_all(home.join(".config/kmp")).expect("config");
    std::fs::write(
        home.join(".config/kmp/config.toml"),
        format!("memory_store = {:?}\n", selected.display().to_string()),
    )
    .expect("saved selection");
    std::fs::create_dir_all(home.join(".local/share/kmp")).expect("data home");
    std::fs::write(
        home.join(".local/share/kmp/known-stores.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({"path": selected, "last_opened_unix_ms": 1})
        ),
    )
    .expect("machine index");
    for directory in ["cache", "state", "hermes", "claude-config", "pi"] {
        std::fs::create_dir_all(home.join(directory)).expect("location");
    }
    let before = snapshot(home);

    let output = Command::new(std::env::current_exe().expect("this test binary"))
        .args(["--exact", "probe_every_spawn_shape", "--nocapture"])
        .env(PROBE, "1")
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("CODEX_HOME", home.join("codex"))
        .env("CLAUDE_CONFIG_DIR", home.join("claude-config"))
        .env("HERMES_HOME", home.join("hermes"))
        .env("PI_CODING_AGENT_DIR", home.join("pi"))
        .env("KMP_MCP_DATA_DIR", &exported)
        .output()
        .expect("probe run");
    assert!(
        output.status.success(),
        "probe failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "the probe did not run: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let after = snapshot(home);
    let changed = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        changed.is_empty(),
        "a spawned binary touched the developer home: {changed:#?}"
    );
}

/// Runs only inside the environment the guard prepares.
#[test]
fn probe_every_spawn_shape() {
    if std::env::var_os(PROBE).is_none() {
        return;
    }
    let work = tempfile::tempdir().expect("work dir outside any project");
    let stubs = work.path().join("stubs");
    std::fs::create_dir_all(&stubs).expect("stub dir");
    let store = work.path().join("store");
    let binary = env!("CARGO_BIN_EXE_kmp-mcp");
    let serve = |envs: &[(&str, &Path)], input: &str| {
        let mut command = isolated_home::command(binary);
        command
            .current_dir(work.path())
            .env("KMP_VIEWER_ADDR", "off")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (name, value) in envs {
            command.env(name, value);
        }
        let mut child = command.spawn().expect("serve spawn");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("requests");
        child.wait().expect("serve exit");
    };
    serve(&[("KMP_MCP_BACKEND", Path::new("fixture"))], TOOLS_LIST);
    serve(&[("KMP_MCP_BACKEND", Path::new("grpc"))], "");
    serve(
        &[("KMP_KERNEL_GRPC_ENDPOINT", Path::new("http://127.0.0.1:9"))],
        "",
    );
    serve(&[("KMP_MCP_DATA_DIR", &store)], TOOLS_LIST);
    serve(&[], TOOLS_LIST);

    // Lifecycle verbs, with an empty PATH so no real host CLI can answer.
    for arguments in [
        &["doctor"][..],
        &["info"],
        &["memories"],
        &["setup", "--dry-run"],
        &["uninstall"],
        &["demo", "--dry-run"],
    ] {
        isolated_home::command(binary)
            .args(arguments)
            .current_dir(work.path())
            .env("PATH", &stubs)
            .env("KMP_VIEWER_ADDR", "off")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("lifecycle verb");
    }
}

#[test]
fn every_test_spawn_of_the_binary_goes_through_an_isolated_home() {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut unisolated = Vec::new();
    for path in sources(&tests) {
        let source = std::fs::read_to_string(&path).expect("test source");
        let spawns = source.contains("CARGO_BIN_EXE_kmp-mcp");
        let raw = source.contains(concat!("Command::new(env!(\"CARGO_BIN_", "EXE_kmp-mcp\"))"));
        if spawns && (raw || !source.contains("isolated_home::command")) {
            unisolated.push(path);
        }
    }
    assert!(
        unisolated.is_empty(),
        "these tests spawn kmp-mcp with the developer's HOME: {unisolated:#?}"
    );
}

fn sources(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(directory).expect("tests dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            found.extend(sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    found
}

/// Every path under `root` with its file contents; directories map to none.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("sentinel dir") {
            let path = entry.expect("entry").path();
            let relative = path.strip_prefix(root).expect("inside").to_path_buf();
            if path.is_dir() {
                entries.insert(relative, None);
                pending.push(path);
            } else {
                entries.insert(relative, Some(std::fs::read(&path).expect("file")));
            }
        }
    }
    entries
}
