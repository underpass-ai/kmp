//! `kmp-mcp memories` through the real binary on an isolated machine (#903).
//!
//! The inventory answers one question — what memory exists here, in which
//! store, and how each is reached — so it is driven the way a person meets
//! it: write memory from one project, export from another, name an old store
//! by hand, then stand somewhere else and ask.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

struct Machine {
    root: tempfile::TempDir,
}

impl Machine {
    fn new() -> Self {
        let machine = Self {
            root: tempfile::tempdir().expect("isolated machine"),
        };
        for relative in ["home", "config", "data", "workspace", "notes"] {
            fs::create_dir_all(machine.at(relative)).expect("isolated directory");
        }
        for project in ["repository", "other"] {
            fs::create_dir_all(machine.at(&format!("{project}/.git"))).expect("project marker");
        }
        machine
    }

    fn at(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    fn command(&self, working_dir: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"));
        command
            .current_dir(self.at(working_dir))
            .env("HOME", self.at("home"))
            .env("XDG_CONFIG_HOME", self.at("config"))
            .env("XDG_DATA_HOME", self.at("data"))
            .env("KMP_MCP_BACKEND", "embedded")
            .env("KMP_VIEWER_ADDR", "off")
            .env_remove("KMP_MCP_DATA_DIR");
        command
    }

    fn run(&self, working_dir: &str, arguments: &[&str]) -> Output {
        self.command(working_dir)
            .args(arguments)
            .output()
            .expect("the binary runs")
    }

    /// Serves MCP from `working_dir` and writes one memory per about.
    fn write_memories(&self, working_dir: &str, batch: &str, abouts: &[&str]) {
        let mut child = self
            .command(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("MCP process starts");
        let mut input = child.stdin.take().expect("piped input");
        for (id, about) in abouts.iter().enumerate() {
            let request = json!({
                "jsonrpc": "2.0",
                "id": id + 1,
                "method": "tools/call",
                "params": {
                    "name": "kmp_ingest",
                    "arguments": {
                        "about": about,
                        "idempotency_key": format!("memories-cli:{batch}:{id}"),
                        "memory": {
                            "dimensions": [{"id": format!("timeline:{id}"), "kind": "timeline"}],
                            "entries": [{
                                "id": format!("{about}:observation:{batch}:{id}"),
                                "kind": "observation",
                                "text": format!("memory {id}"),
                                "coordinates": [{
                                    "dimension": "timeline",
                                    "scope_id": format!("timeline:{id}"),
                                    "sequence": 1
                                }]
                            }]
                        }
                    }
                }
            });
            writeln!(input, "{request}").expect("request written");
        }
        drop(input);
        let output = child.wait_with_output().expect("MCP process exits");
        assert!(
            output.status.success(),
            "serve: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let responses = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            responses.matches("\"isError\":true").count(),
            0,
            "every write lands: {responses}"
        );
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("UTF-8 stdout")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("UTF-8 stderr")
}

/// Every file under a directory with its bytes, to prove a read wrote nothing.
fn snapshot(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).expect("readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.insert(path.clone(), fs::read(&path).expect("readable file"));
            }
        }
    }
    files
}

fn store<'a>(inventory: &'a Value, path: &Path) -> &'a Value {
    inventory["memories"]
        .as_array()
        .expect("memories array")
        .iter()
        .find(|memory| memory["path"] == path.display().to_string())
        .unwrap_or_else(|| panic!("{} is listed: {inventory:#}", path.display()))
}

#[test]
fn every_store_on_the_machine_is_listed_with_its_reach_and_its_abouts() {
    let machine = Machine::new();
    let repository = machine.at("repository/.kernel");
    let other = machine.at("other/.kernel");
    let legacy = machine.at("legacy/.kernel");

    // A project written through serve, and another only ever exported from:
    // both must be remembered, not only the one a session served.
    machine.write_memories(
        "repository",
        "first",
        &["project:repository", "project:repository:ops"],
    );
    machine.write_memories("repository", "second", &["project:repository"]);
    let exported = machine.run(
        "other",
        &[
            "export",
            &machine.at("notes/other.jsonl").display().to_string(),
        ],
    );
    assert!(exported.status.success(), "export: {}", stderr(&exported));

    // An old store nothing here has opened: named by hand, never opened.
    fs::create_dir_all(legacy.join("store")).expect("legacy store");
    fs::write(legacy.join("FORMAT_VERSION"), "2").expect("legacy stamp");
    fs::write(legacy.join("store/kernel.sqlite3"), b"format-2 bytes").expect("legacy file");
    let registered = machine.run(
        "workspace",
        &["memories", "register", &legacy.display().to_string()],
    );
    assert!(
        registered.status.success(),
        "register: {}",
        stderr(&registered)
    );
    assert!(stdout(&registered).contains("registered"));

    let before = snapshot(machine.root.path());
    let listed = machine.run("workspace", &["memories"]);
    assert!(listed.status.success(), "memories: {}", stderr(&listed));
    let text = stdout(&listed);
    println!("{text}");
    assert!(text.contains(&repository.display().to_string()), "{text}");
    assert!(text.contains(&other.display().to_string()), "{text}");
    assert!(text.contains("project:repository:ops"), "{text}");
    assert!(text.contains("unsupported format-2 artifact"), "{text}");
    assert!(
        text.contains("contents not readable by this engine"),
        "{text}"
    );

    let json = machine.run("repository", &["memories", "--json"]);
    assert!(json.status.success(), "memories --json: {}", stderr(&json));
    let inventory: Value = serde_json::from_slice(&json.stdout).expect("stdout is only JSON");

    let served = store(&inventory, &repository);
    assert_eq!(served["reach"], "project");
    assert_eq!(served["opened_here"], true, "{served:#}");
    assert_eq!(served["readable"], true);
    let counts: BTreeMap<String, u64> = served["abouts"]
        .as_array()
        .expect("abouts")
        .iter()
        .map(|about| {
            (
                about["about"].as_str().expect("about").to_string(),
                about["events"].as_u64().expect("events"),
            )
        })
        .collect();
    assert_eq!(counts.get("project:repository"), Some(&2), "{served:#}");
    assert_eq!(counts.get("project:repository:ops"), Some(&1), "{served:#}");
    assert!(served["last_write"].is_string(), "{served:#}");

    let exported_from = store(&inventory, &other);
    assert_eq!(exported_from["reach"], "project");
    assert_eq!(exported_from["opened_here"], false);

    let old = store(&inventory, &legacy);
    assert_eq!(old["readable"], false);
    assert_eq!(old["storage"], "unsupported format-2 artifact");

    // From anywhere, the env override names the rule that chose it.
    let by_env = machine
        .command("workspace")
        .env("KMP_MCP_DATA_DIR", &other)
        .args(["memories", "--json"])
        .output()
        .expect("memories runs");
    let by_env: Value = serde_json::from_slice(&by_env.stdout).expect("JSON");
    assert_eq!(store(&by_env, &other)["reach"], "env");
    assert_eq!(store(&by_env, &other)["opened_here"], true);

    // Looking wrote nothing anywhere on the machine: no lease, no index
    // line, no WAL beside a store at rest, no migrated legacy byte.
    let info = machine.run("workspace", &["info"]);
    assert!(info.status.success(), "info: {}", stderr(&info));
    assert!(
        stdout(&info).contains("project:repository 2"),
        "{}",
        stdout(&info)
    );
    assert_eq!(snapshot(machine.root.path()), before);
}

#[test]
fn register_refuses_what_is_not_an_absolute_store_and_creates_nothing() {
    let machine = Machine::new();
    let plain = machine.at("notes");
    let before = snapshot(machine.root.path());

    for (argument, expected) in [
        ("repository/.kernel", "is relative"),
        ("~/repository/.kernel", "starts with `~`"),
        (plain.to_str().expect("utf8"), "is not a KMP store"),
    ] {
        let refused = machine.run("workspace", &["memories", "register", argument]);
        assert_eq!(refused.status.code(), Some(2), "{argument}");
        assert!(stderr(&refused).contains(expected), "{}", stderr(&refused));
        assert!(stdout(&refused).is_empty());
    }
    assert_eq!(snapshot(machine.root.path()), before);
    assert!(!machine.at("data/kmp/known-stores.jsonl").exists());
}
