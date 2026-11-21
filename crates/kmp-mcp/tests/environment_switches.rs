//! Environment switches are strict and honest (E18), and the commands that
//! read the local store refuse to run under a remote kernel (E14).
//!
//! Every `KMP_*` switch refuses a value it does not accept, at startup,
//! naming what it accepts; on/off switches share one spelling set, in any
//! case; a blank value means unset.

#[path = "support/isolated_home.rs"]
mod isolated_home;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"switches","version":"0"}}}"#;
const TOOLS_LIST: &str = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_kmp-mcp")
}

fn store() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("environment-switches");
    std::fs::create_dir_all(&root).expect("scratch");
    tempfile::tempdir_in(root).expect("store")
}

fn command(data_dir: &Path, env: &[(&str, &str)]) -> Command {
    let mut command = isolated_home::command(binary());
    command
        .env("KMP_MCP_DATA_DIR", data_dir)
        .env("KMP_VIEWER_ADDR", "off")
        .env_remove("KMP_LEXICAL_INDEX")
        .env_remove("KMP_MCP_PASSAGES")
        .env_remove("KMP_MCP_OUTPUT_SCHEMAS")
        .env_remove("KMP_VERSION_CHECK");
    for (name, value) in env {
        command.env(name, value);
    }
    command
}

fn serve(data_dir: &Path, env: &[(&str, &str)], input: &str) -> Output {
    let mut child = command(data_dir, env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn kmp-mcp");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write requests");
    child.wait_with_output().expect("kmp-mcp exits")
}

fn verb(data_dir: &Path, env: &[(&str, &str)], args: &[&str]) -> Output {
    command(data_dir, env)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run kmp-mcp")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A misspelt value of each switch, the words its refusal must name, and
/// what else the environment needs for the switch to be read.
type Misspelt = (
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const MISSPELT: &[Misspelt] = &[
    ("KMP_MCP_BACKEND", "embeded", "embedded, grpc", &[]),
    (
        "KMP_KERNEL_GRPC_TLS_MODE",
        "mutal",
        "mutual",
        &[("KMP_KERNEL_GRPC_ENDPOINT", "http://127.0.0.1:9")],
    ),
    ("KMP_MCP_PASSAGES", "sharde", "inline, shared", &[]),
    ("KMP_MCP_OUTPUT_SCHEMAS", "banana", "on, off", &[]),
    ("KMP_LEXICAL_INDEX", "offf", "on, off", &[]),
    ("KMP_VERSION_CHECK", "nope", "on, off", &[]),
    ("KMP_VIEWER_ADDR", "banana", "loopback", &[]),
    ("KMP_MCP_DATA_DIR", "relative/store", "absolute path", &[]),
];

#[test]
fn each_strict_switch_refuses_a_misspelt_value_at_startup_naming_what_it_accepts() {
    for &(name, value, accepted, extra) in MISSPELT {
        let dir = store();
        let mut env = vec![(name, value)];
        env.extend_from_slice(extra);
        let output = serve(dir.path(), &env, &format!("{INITIALIZE}\n"));
        let stderr = text(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{name}={value}: {stderr}");
        assert!(
            text(&output.stdout).trim().is_empty(),
            "{name}={value} answered initialize"
        );
        assert!(stderr.contains(name), "{name}={value}: {stderr}");
        assert!(stderr.contains(accepted), "{name}={value}: {stderr}");
        // Only a remote backend someone asked for gets the backend hint.
        if name != "KMP_MCP_BACKEND" {
            assert!(
                !stderr.contains("unset KMP_MCP_BACKEND"),
                "{name}={value}: {stderr}"
            );
        }

        let doctor = verb(dir.path(), &env, &["doctor"]);
        let report = text(&doctor.stdout);
        assert_ne!(doctor.status.code(), Some(0), "{name}={value}: {report}");
        assert!(report.contains(name), "{name}={value}: {report}");

        let summaries = verb(dir.path(), &env, &["summaries", "pending"]);
        assert_eq!(
            summaries.status.code(),
            Some(2),
            "{name}={value}: {}",
            text(&summaries.stderr)
        );
    }
}

#[test]
fn on_off_switches_read_one_spelling_set_in_any_case_and_blank_is_unset() {
    let dir = store();
    let input = format!("{INITIALIZE}\n{TOOLS_LIST}\n");
    for env in [
        vec![("KMP_LEXICAL_INDEX", "OFF")],
        vec![("KMP_LEXICAL_INDEX", "0")],
        vec![("KMP_LEXICAL_INDEX", "Yes")],
        vec![("KMP_MCP_PASSAGES", "")],
        vec![("KMP_MCP_PASSAGES", "Shared")],
        vec![("KMP_MCP_OUTPUT_SCHEMAS", "")],
        vec![("KMP_MCP_BACKEND", " Embedded ")],
        vec![("KMP_VERSION_CHECK", "False")],
    ] {
        let output = serve(dir.path(), &env, &input);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{env:?}: {}",
            text(&output.stderr)
        );
        assert_eq!(text(&output.stdout).lines().count(), 2, "{env:?}");
    }
    let output = serve(dir.path(), &[("KMP_MCP_OUTPUT_SCHEMAS", "TRUE")], &input);
    assert!(
        text(&output.stdout).contains("outputSchema"),
        "TRUE turns output schemas on"
    );
}

#[test]
fn the_retired_backend_aliases_are_refused() {
    for alias in ["live", "fixtures"] {
        let dir = store();
        let output = serve(
            dir.path(),
            &[
                ("KMP_MCP_BACKEND", alias),
                ("KMP_KERNEL_GRPC_ENDPOINT", "http://127.0.0.1:9"),
            ],
            &format!("{INITIALIZE}\n"),
        );
        assert_eq!(
            output.status.code(),
            Some(2),
            "{alias}: {}",
            text(&output.stderr)
        );
    }
}

#[test]
fn doctor_and_info_list_the_switches_in_effect() {
    let dir = store();
    let env = [("KMP_LEXICAL_INDEX", "off"), ("KMP_MCP_ENGINE", "sqlite")];
    let info = text(&verb(dir.path(), &env, &["info"]).stdout);
    assert!(info.contains("Environment"), "{info}");
    assert!(info.contains("KMP_LEXICAL_INDEX=off"), "{info}");
    assert!(info.contains("KMP_VIEWER_ADDR=off"), "{info}");
    let doctor = text(&verb(dir.path(), &env, &["doctor"]).stdout);
    assert!(doctor.contains("KMP_LEXICAL_INDEX=off"), "{doctor}");
    // A removed switch is named as unread, never silently honoured.
    assert!(
        doctor.contains("KMP_MCP_ENGINE is set, and this build reads no such switch"),
        "{doctor}"
    );
}

/// `grpc` with no endpoint names the variable to set, with no stray space
/// before the comma.
#[test]
fn doctor_names_the_missing_grpc_endpoint_cleanly() {
    let dir = store();
    let doctor = text(&verb(dir.path(), &[("KMP_MCP_BACKEND", "grpc")], &["doctor"]).stdout);
    assert!(
        doctor.contains("grpc, with no kernel to talk to"),
        "{doctor}"
    );
    assert!(
        doctor.contains("KMP_KERNEL_GRPC_ENDPOINT, or unset"),
        "{doctor}"
    );
    assert!(!doctor.contains("ENDPOINT ,"), "{doctor}");
}

fn plugin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/kmp")
}

#[test]
fn plugin_notice_refuses_a_misspelt_version_check() {
    let dir = store();
    let root = plugin_root();
    let output = verb(
        dir.path(),
        &[("KMP_VERSION_CHECK", "of")],
        &[
            "plugin",
            "notice",
            "--plugin-root",
            root.to_str().expect("utf-8"),
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    assert!(text(&output.stderr).contains("KMP_VERSION_CHECK"));
}

#[test]
fn local_store_commands_refuse_to_run_under_a_remote_kernel() {
    let dir = store();
    let remote = [("KMP_KERNEL_GRPC_ENDPOINT", "http://127.0.0.1:9")];
    for args in [
        vec!["summaries", "pending"],
        vec!["memories"],
        vec!["export", "/nonexistent/bundle.jsonl"],
        vec!["document", "proj:x"],
        vec!["snapshot", "list"],
        vec!["demo", "--dry-run"],
        vec!["import", "/nonexistent/bundle.jsonl"],
        vec!["viewer", "127.0.0.1:0"],
    ] {
        let output = verb(dir.path(), &remote, &args);
        let stderr = text(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {stderr}");
        assert!(
            stderr.contains("works on the local store"),
            "{args:?}: {stderr}"
        );
        assert!(stderr.contains("does not apply"), "{args:?}: {stderr}");
    }
    // Nothing was created on the way: the refusal comes before any store opens.
    assert!(
        std::fs::read_dir(dir.path())
            .expect("store dir")
            .next()
            .is_none(),
        "a refused command created files"
    );
    let info = text(&verb(dir.path(), &remote, &["info"]).stdout);
    assert!(info.contains("not on this machine"), "{info}");
}
