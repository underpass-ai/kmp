//! `kmp-mcp demo` writes the worked example once, where the store resolves,
//! and never into a project's store: a team's committed memory and its
//! snapshots must not carry it.
#[path = "support/isolated_home.rs"]
mod isolated_home;

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

fn kmp(scratch: &Path, project: &Path) -> Command {
    let mut command = isolated_home::command(env!("CARGO_BIN_EXE_kmp-mcp"));
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
fn demo_in_a_project_writes_the_user_store_and_never_the_project_store() {
    let scratch = tempfile::tempdir().expect("scratch");
    let project = project(scratch.path());
    let user_store = scratch.path().join("home/.local/share/kmp/default");

    let dry = run(kmp(scratch.path(), &project).args(["demo", "--dry-run"]));
    let dry = String::from_utf8_lossy(&dry.stdout);
    assert!(
        dry.contains("would write 7 memories about example:kmp-demo"),
        "{dry}"
    );
    assert!(dry.contains(&user_store.display().to_string()), "{dry}");
    assert!(
        dry.contains("the demo never writes a project's memory"),
        "{dry}"
    );
    assert!(
        !project.join(".kernel").exists(),
        "a dry run creates no store skeleton (N1)"
    );
    assert!(
        !user_store.exists(),
        "a dry run creates no user store either"
    );
    assert!(
        !scratch
            .path()
            .join("home/.local/share/kmp/known-stores.jsonl")
            .exists(),
        "a dry run records nothing in the machine's index (N1)"
    );

    for _ in 0..2 {
        let output = run(kmp(scratch.path(), &project).args(["demo", "--no-viewer"]));
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("7 memories about example:kmp-demo"), "{text}");
        assert!(text.contains("(chosen by user)"), "{text}");
        assert!(text.contains(&user_store.display().to_string()), "{text}");
        assert!(text.contains("Not in this project's store"), "{text}");
        assert!(
            text.contains(&format!("KMP_MCP_DATA_DIR={}", user_store.display())),
            "an agent in the project would not find it, and the output says so: {text}"
        );
        assert!(
            text.contains("Why are retries to the payment provider capped at two?"),
            "{text}"
        );
    }
    assert!(
        !project.join(".kernel").exists() && !project.join(".kmp").exists(),
        "the project holds no store and no bundle"
    );

    // The example is where the demo said, and a snapshot of the project
    // cannot carry it.
    let explicit = scratch.path().join("demo.jsonl");
    run(kmp(scratch.path(), scratch.path())
        .env("KMP_MCP_DATA_DIR", &user_store)
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
fn an_explicit_data_dir_still_decides_where_the_demo_goes() {
    let scratch = tempfile::tempdir().expect("scratch");
    let project = project(scratch.path());
    let chosen = scratch.path().join("chosen");
    let output = run(kmp(scratch.path(), &project)
        .env("KMP_MCP_DATA_DIR", &chosen)
        .args(["demo", "--no-viewer"]));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("(chosen by env)"), "{text}");
    assert!(text.contains("Ask your agent"), "{text}");
    assert!(chosen.join("FORMAT_VERSION").is_file());
    assert!(!project.join(".kernel").exists());
}

#[test]
fn demo_refuses_a_backend_that_is_not_the_local_store() {
    let scratch = tempfile::tempdir().expect("scratch");
    let project = project(scratch.path());
    let output = kmp(scratch.path(), &project)
        .env("KMP_MCP_BACKEND", "grpc")
        .env("KMP_KERNEL_GRPC_ENDPOINT", "http://127.0.0.1:9")
        .args(["demo", "--no-viewer"])
        .output()
        .expect("kmp-mcp runs");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("works on the local store"), "{stderr}");
}
