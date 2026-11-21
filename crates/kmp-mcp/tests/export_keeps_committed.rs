//! `export` never drops a committed event without `--overwrite-committed`,
//! however the committed `.kmp/memory.jsonl` is named and whichever rule
//! chose the store.
//!
//! Only the spelling `project_bundle_path` produces used to be guarded: a
//! relative path, `./`, a symlinked directory or a store pinned by
//! `KMP_MCP_DATA_DIR` (as a host that manages its store does) replaced a
//! committed bundle holding other abouts with one holding only the named one.
#[path = "support/isolated_home.rs"]
mod isolated_home;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use serde_json::{Value, json};

fn kmp(project: &Path, home: &Path, args: &[&str], env: &[(&str, &Path)], stdin: &str) -> Output {
    let mut command = isolated_home::command(env!("CARGO_BIN_EXE_kmp-mcp"));
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
        .env("KMP_VERSION_CHECK", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in env {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("kmp-mcp spawns");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("stdin written");
    child.wait_with_output().expect("kmp-mcp exits")
}

fn write(project: &Path, home: &Path, about: &str, key: &str) {
    let call = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "kmp_write_memory", "arguments": {
            "about": about, "actor": "test", "labels": {"topic": ["deploy"]},
            "idempotency_key": key,
            "memories": [{"id": "m", "kind": "decision",
                "summary": format!("Decision {key}: the deploy window for {about}."),
                "evidence": format!("Notes {key}.")}]}}});
    let output = kmp(project, home, &[], &[], &format!("{call}\n"));
    let reply: Value = serde_json::from_slice(&output.stdout).expect("JSON-RPC reply");
    assert_eq!(
        reply["result"]["structuredContent"]["status"], "committed",
        "{reply}"
    );
}

/// `(event_count, abouts)` of a project's committed file.
fn committed(project: &Path) -> (u64, Vec<String>) {
    let text = std::fs::read_to_string(project.join(".kmp/memory.jsonl")).expect("bundle");
    let header = kmp_embedded::verify_bundle(&text).expect("bundle verifies");
    (header.event_count, header.abouts)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("copy dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

struct Fixture {
    root: tempfile::TempDir,
    home: PathBuf,
    /// Committed and stored: `project:a` and `project:b`.
    both: PathBuf,
    /// Committed `project:a` and `project:b` (a pull), stored only `project:a`.
    pulled: PathBuf,
    /// Committed and stored: only `project:a`.
    only_a: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("root");
        let base = std::fs::canonicalize(root.path()).expect("real root");
        let home = base.join("home");
        let both = base.join("both");
        std::fs::create_dir_all(both.join(".git")).expect("project marker");
        write(&both, &home, "project:a", "a1");
        let only_a = base.join("only-a");
        copy_tree(&both, &only_a);
        write(&both, &home, "project:b", "b1");
        let pulled = base.join("pulled");
        copy_tree(&only_a, &pulled);
        std::fs::copy(
            both.join(".kmp/memory.jsonl"),
            pulled.join(".kmp/memory.jsonl"),
        )
        .expect("pull");
        Self {
            root,
            home,
            both,
            pulled,
            only_a,
        }
    }

    /// A fresh copy of `source` for one case.
    fn case(&self, source: &Path, name: &str) -> PathBuf {
        let project = self.root.path().join("cases").join(name);
        copy_tree(source, &project);
        std::fs::canonicalize(project).expect("real case")
    }

    fn export(&self, project: &Path, args: &[&str]) -> Output {
        let mut full = vec!["export"];
        full.extend_from_slice(args);
        kmp(project, &self.home, &full, &[], "")
    }

    fn export_pinned(&self, project: &Path, args: &[&str]) -> Output {
        let mut full = vec!["export"];
        full.extend_from_slice(args);
        let store = project.join(".kernel");
        kmp(
            project,
            &self.home,
            &full,
            &[("KMP_MCP_DATA_DIR", &store)],
            "",
        )
    }
}

fn both_abouts() -> (u64, Vec<String>) {
    (2, vec!["project:a".to_string(), "project:b".to_string()])
}

fn assert_refused_naming_the_loss(output: &Output, project: &Path) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("nothing was written"), "{stderr}");
    assert!(stderr.contains("1 event (1 of `project:b`)"), "{stderr}");
    assert!(stderr.contains("--overwrite-committed"), "{stderr}");
    assert_eq!(
        committed(project),
        both_abouts(),
        "the committed file is untouched"
    );
}

#[test]
fn an_about_export_over_the_committed_file_is_refused_however_it_is_named() {
    let fixture = Fixture::new();
    for (name, source) in [("both", &fixture.both), ("pulled", &fixture.pulled)] {
        let project = fixture.case(source, &format!("named-{name}"));
        let absolute = project.join(".kmp/memory.jsonl");
        let absolute = absolute.to_str().expect("utf-8 path");
        for path in [
            None,
            Some(".kmp/memory.jsonl"),
            Some("./.kmp/memory.jsonl"),
            Some(absolute),
        ] {
            let mut args: Vec<&str> = path.into_iter().collect();
            args.extend(["--about", "project:a"]);
            let output = fixture.export(&project, &args);
            assert_refused_naming_the_loss(&output, &project);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("without --about"), "{path:?}: {stderr}");
        }
    }
}

#[test]
fn a_store_pinned_by_the_environment_is_held_to_the_same_rule() {
    let fixture = Fixture::new();
    // The downstream report: a host pins the store and names the file.
    let project = fixture.case(&fixture.both, "pinned-about");
    let head = project.join(".kmp/memory.jsonl");
    let head = head.to_str().expect("utf-8 path");
    for path in [head, ".kmp/memory.jsonl"] {
        let output = fixture.export_pinned(&project, &[path, "--about", "project:a"]);
        assert_refused_naming_the_loss(&output, &project);
    }

    // A whole export from a pinned store that holds every committed event.
    let output = fixture.export_pinned(&project, &[head]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(committed(&project), both_abouts());

    // A pinned store that lacks the pulled event: no catch-up, so refused.
    let project = fixture.case(&fixture.pulled, "pinned-pulled");
    let head = project.join(".kmp/memory.jsonl");
    let output = fixture.export_pinned(&project, &[head.to_str().expect("utf-8 path")]);
    assert_refused_naming_the_loss(&output, &project);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("kmp-mcp import --from"), "{stderr}");
}

#[test]
fn a_whole_export_of_a_pulled_file_catches_up_however_it_is_named() {
    let fixture = Fixture::new();
    for (name, path) in [("none", None), ("relative", Some(".kmp/memory.jsonl"))] {
        let project = fixture.case(&fixture.pulled, &format!("pulled-{name}"));
        let output = fixture.export(&project, &path.into_iter().collect::<Vec<_>>());
        assert!(output.status.success(), "{name}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("caught this store up"), "{name}: {stderr}");
        assert_eq!(committed(&project), both_abouts(), "{name}");
    }
}

#[test]
fn what_drops_nothing_is_still_written() {
    let fixture = Fixture::new();
    // Another file takes exactly the subset.
    let project = fixture.case(&fixture.both, "elsewhere");
    let output = fixture.export(&project, &["subset.jsonl", "--about", "project:a"]);
    assert!(output.status.success(), "{output:?}");
    let subset = std::fs::read_to_string(project.join("subset.jsonl")).expect("subset");
    assert_eq!(
        kmp_embedded::verify_bundle(&subset)
            .expect("verifies")
            .event_count,
        1
    );
    assert_eq!(committed(&project), both_abouts());

    // A host's everyday save: the committed file holds only that about.
    let project = fixture.case(&fixture.only_a, "only-a");
    let output = fixture.export(&project, &["--about", "project:a"]);
    assert!(output.status.success(), "{output:?}");
    let head = project.join(".kmp/memory.jsonl");
    let output = fixture.export_pinned(
        &project,
        &[head.to_str().expect("utf-8 path"), "--about", "project:a"],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(committed(&project), (1, vec!["project:a".to_string()]));

    // A first save: no committed file yet, named relatively.
    std::fs::remove_dir_all(project.join(".kmp")).expect("no bundle yet");
    let output = fixture.export(&project, &["./.kmp/memory.jsonl"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(committed(&project), (1, vec!["project:a".to_string()]));

    // --overwrite-committed replaces it, as asked.
    let project = fixture.case(&fixture.pulled, "overwrite");
    let output = fixture.export(
        &project,
        &[
            ".kmp/memory.jsonl",
            "--about",
            "project:a",
            "--overwrite-committed",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(committed(&project), (1, vec!["project:a".to_string()]));
}

#[cfg(unix)]
#[test]
fn a_symlink_to_the_committed_file_or_its_directory_is_the_committed_file() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let project = fixture.case(&fixture.both, "links");
    symlink(
        project.join(".kmp/memory.jsonl"),
        project.join("link.jsonl"),
    )
    .expect("file link");
    symlink(project.join(".kmp"), project.join("alias")).expect("directory link");
    let through = fixture.root.path().join("through");
    symlink(&project, &through).expect("project link");
    let via_parent = through.join(".kmp/memory.jsonl");
    for path in [
        "link.jsonl",
        "alias/memory.jsonl",
        via_parent.to_str().expect("utf-8"),
    ] {
        let output = fixture.export(&project, &[path, "--about", "project:a"]);
        assert_refused_naming_the_loss(&output, &project);
    }

    // Allowed through the file link, the bytes land in the committed file
    // and the link stays a link.
    let output = fixture.export(&project, &["link.jsonl"]);
    assert!(output.status.success(), "{output:?}");
    let link = std::fs::symlink_metadata(project.join("link.jsonl")).expect("link");
    assert!(link.file_type().is_symlink());
    let reported: Value = serde_json::from_slice(&output.stdout).expect("export JSON");
    assert_eq!(reported["exported_to"], "link.jsonl");
    assert_eq!(committed(&project), both_abouts());
}
