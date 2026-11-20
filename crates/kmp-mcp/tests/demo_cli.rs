//! `kmp-mcp demo` writes the worked example once, where the store resolves,
//! and keeps it out of the project's committed bundle.
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

fn kmp(scratch: &Path, project: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"));
    command
        .current_dir(project)
        .env_remove("KMP_MCP_DATA_DIR")
        .env_remove("KMP_MCP_BACKEND")
        .env_remove("KMP_PLUGIN_ROOT")
        .env("HOME", scratch.join("home"))
        .env("USERPROFILE", scratch.join("home"))
        .env("XDG_CONFIG_HOME", scratch.join("home/.config"))
        .env("XDG_DATA_HOME", scratch.join("home/.local/share"))
        .env("KMP_VIEWER_ADDR", "off");
    command
}

fn run(command: &mut Command) -> Output {
    let output = command.output().expect("kmp-mcp runs");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn project(scratch: &Path) -> std::path::PathBuf {
    let project = scratch.join("project");
    std::fs::create_dir_all(project.join(".git")).expect("a git root");
    std::fs::create_dir_all(scratch.join("home")).expect("home");
    project
}

#[test]
fn demo_writes_the_example_once_and_keeps_it_out_of_the_project_bundle() {
    let scratch = tempfile::tempdir().expect("scratch");
    let project = project(scratch.path());

    let dry = run(kmp(scratch.path(), &project).args(["demo", "--dry-run"]));
    let dry = String::from_utf8_lossy(&dry.stdout);
    assert!(
        dry.contains("would write 7 memories about example:kmp-demo"),
        "{dry}"
    );
    assert!(
        !project.join(".kernel/FORMAT_VERSION").exists(),
        "a dry run creates no store"
    );

    for _ in 0..2 {
        let output = run(kmp(scratch.path(), &project).args(["demo", "--no-viewer"]));
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("7 memories about example:kmp-demo"), "{text}");
        assert!(text.contains("(chosen by project)"), "{text}");
        assert!(
            text.contains("Why are retries to the payment provider capped at two?"),
            "{text}"
        );
    }

    let memories = run(kmp(scratch.path(), &project).args(["memories", "--json"]));
    let memories: Value = serde_json::from_slice(&memories.stdout).expect("memories JSON");
    let text = memories.to_string();
    assert!(text.contains("example:kmp-demo"), "{text}");

    // The project head leaves the example out, like the guides.
    let head = run(kmp(scratch.path(), &project).arg("export"));
    let receipt: Value = serde_json::from_slice(&head.stdout).expect("export receipt");
    assert_eq!(receipt["abouts"], serde_json::json!([]));
    assert_eq!(receipt["event_count"], 0);

    // An explicit export of the about still carries it, for the examples.
    let explicit = scratch.path().join("demo.jsonl");
    run(kmp(scratch.path(), &project)
        .arg("export")
        .arg(&explicit)
        .args(["--about", "example:kmp-demo"]));
    let bundle = std::fs::read_to_string(explicit).expect("bundle");
    let header: Value =
        serde_json::from_str(bundle.lines().next().expect("header")).expect("header");
    assert_eq!(header["abouts"], serde_json::json!(["example:kmp-demo"]));
    assert_eq!(header["event_count"], 1);
}

#[test]
fn demo_refuses_a_backend_that_is_not_the_local_store() {
    let scratch = tempfile::tempdir().expect("scratch");
    let project = project(scratch.path());
    let output = kmp(scratch.path(), &project)
        .env("KMP_MCP_BACKEND", "fixture")
        .args(["demo", "--no-viewer"])
        .output()
        .expect("kmp-mcp runs");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("fixture backend"), "{stderr}");
}
