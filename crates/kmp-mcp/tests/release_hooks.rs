//! Test and evaluation hooks do not ship in a release binary (E18).
//!
//! A debug build (what `cargo test` runs) and a build with the `eval-hooks`
//! feature carry them, and this test then proves each hook is observable, so
//! the release assertion below is not vacuous. A release build without the
//! feature must not react to any of them: same `info`, no fixture, no
//! injected release, no measurement modes, and not even the names in the
//! executable.
//!
//! Run it against the shipped configuration with
//! `cargo test --release -p kmp-mcp --locked --test release_hooks`, or point
//! `KMP_MCP_RELEASE_BINARY` at a binary built as `release.yml` builds it.

#[path = "support/isolated_home.rs"]
mod isolated_home;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"hooks","version":"0"}}}"#;

/// Every hook a test or the bench sets, with a value that would act.
const HOOKS: &[(&str, &str)] = &[
    ("KMP_TYPESAFE_CASSETTE", "CASSETTE"),
    ("KMP_TYPESAFE_CASSETTE_MODE", "record"),
    ("KMP_EVAL_PARTNER_FACTS", "200"),
    ("KMP_JUDGEMENT_DEADLINES", "off"),
    ("KMP_LATEST_VERSION", "9.9.9"),
];

/// Test drivers' variables, which no build of the engine reads.
const TEST_ONLY: &[(&str, &str)] = &[
    ("KMP_TEST_ATOMIC_PROJECTION", "1"),
    ("KMP_TEST_HOME_GUARD_PROBE", "1"),
    ("KMP_LEASE_TEST_MODE", "hold"),
    ("KMP_MARGIN_GATE_TEST_CHILD", "1"),
    ("KMP_BLESS_TOOL_SURFACE", "1"),
    ("KMP_BLESS_CLI_SURFACE", "1"),
];

fn binary() -> PathBuf {
    std::env::var_os("KMP_MCP_RELEASE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_kmp-mcp")))
}

/// Whether the binary under test should carry the hooks.
fn hooks_expected() -> bool {
    std::env::var_os("KMP_MCP_RELEASE_BINARY").is_none()
        && cfg!(any(debug_assertions, feature = "eval-hooks"))
}

fn scratch() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("release-hooks");
    std::fs::create_dir_all(&root).expect("scratch");
    tempfile::tempdir_in(root).expect("store")
}

/// The store a scratch directory holds, beside the home of its runs.
fn data_dir(scratch: &Path) -> PathBuf {
    scratch.join("data")
}

fn command(scratch: &Path, env: &[(&str, &str)]) -> Command {
    // A home of its own: tests running in parallel register stores in the
    // home they share, and `info` lists every store it knows.
    let home = scratch.join("home");
    let mut command = isolated_home::command(binary());
    command
        .env("HOME", &home)
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("KMP_MCP_DATA_DIR", data_dir(scratch))
        .env("KMP_VIEWER_ADDR", "off")
        .env_remove("TYPESAFE_API_KEY");
    for (name, value) in HOOKS.iter().chain(TEST_ONLY) {
        let _ = value;
        command.env_remove(name);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    command
}

fn run(store: &Path, env: &[(&str, &str)], args: &[&str], input: &str) -> Output {
    let mut child = command(store, env)
        .args(args)
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
        .expect("stdin");
    child.wait_with_output().expect("exit")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A store that opted in to TypeSafe, so a cassette would take effect.
fn opted_in_store() -> (tempfile::TempDir, String) {
    let dir = scratch();
    std::fs::create_dir_all(data_dir(dir.path())).expect("data dir");
    std::fs::write(
        data_dir(dir.path()).join("typesafe.json"),
        r#"{"endpoint":"https://api.typesafe.ai/v1","model":"jev-1.13.0","timeout_ms":5000}"#,
    )
    .expect("typesafe.json");
    let cassette = dir.path().join("jev.cassette.json");
    std::fs::write(
        &cassette,
        r#"{"schema":"kmp.typesafe.cassette.v1","model":"jev-1.13.0","entries":{}}"#,
    )
    .expect("cassette");
    (dir, cassette.display().to_string())
}

#[test]
fn info_does_not_react_to_a_hook_a_release_build_does_not_carry() {
    // A store of its own per variable: a hook that acts (a recording
    // cassette) may leave files behind that change the next report.
    let info = |env: &[(&str, &str)]| {
        let (store, cassette) = opted_in_store();
        let env = env
            .iter()
            .map(|(name, value)| {
                (
                    *name,
                    if *value == "CASSETTE" {
                        cassette.clone()
                    } else {
                        (*value).to_string()
                    },
                )
            })
            .collect::<Vec<_>>();
        let env = env
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect::<Vec<_>>();
        // A first report may settle files (the store index); compare the
        // second and the third.
        run(store.path(), &[], &["info"], "");
        let baseline = text(&run(store.path(), &[], &["info"], "").stdout);
        let with_variable = text(&run(store.path(), &env, &["info"], "").stdout);
        (baseline, with_variable)
    };
    for (name, value) in HOOKS {
        let (baseline, with_hook) = info(&[(name, value)]);
        if hooks_expected() {
            assert_ne!(baseline, with_hook, "{name} is a hook this build carries");
        } else {
            assert_eq!(baseline, with_hook, "a release binary reacted to {name}");
        }
    }
    for (name, value) in TEST_ONLY {
        let (baseline, with_variable) = info(&[(name, value)]);
        assert_eq!(baseline, with_variable, "the engine reacted to {name}");
    }
}

#[test]
fn the_fixture_backend_and_the_measurement_modes_exist_only_with_the_hooks() {
    let store = scratch();
    for (name, value) in [
        ("KMP_MCP_BACKEND", "fixture"),
        ("KMP_LEXICAL_INDEX", "shadow"),
        ("KMP_LEXICAL_INDEX", "verify"),
    ] {
        let output = run(
            store.path(),
            &[(name, value)],
            &[],
            &format!("{INITIALIZE}\n"),
        );
        let stdout = text(&output.stdout);
        if hooks_expected() {
            assert_eq!(output.status.code(), Some(0), "{name}={value}");
            assert!(stdout.contains("serverInfo"), "{name}={value}: {stdout}");
        } else {
            assert_eq!(
                output.status.code(),
                Some(2),
                "a release binary accepted {name}={value}"
            );
            assert!(stdout.trim().is_empty(), "{name}={value}: {stdout}");
            assert!(text(&output.stderr).contains(name));
        }
    }
}

#[test]
fn an_injected_latest_release_needs_the_hooks() {
    let store = scratch();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/kmp");
    let output = run(
        store.path(),
        &[
            ("KMP_VERSION_CHECK", "off"),
            ("KMP_LATEST_VERSION", "9.9.9"),
        ],
        &[
            "plugin",
            "notice",
            "--plugin-root",
            root.to_str().expect("utf-8"),
        ],
        "",
    );
    let stdout = text(&output.stdout);
    if hooks_expected() {
        assert!(stdout.contains("9.9.9"), "{stdout}");
    } else {
        assert!(
            !stdout.contains("9.9.9"),
            "a release binary injected: {stdout}"
        );
    }
}

#[test]
fn a_release_binary_carries_none_of_the_hook_names() {
    let bytes = std::fs::read(binary()).expect("read the binary");
    let carries = |name: &str| {
        bytes
            .windows(name.len())
            .any(|window| window == name.as_bytes())
    };
    for (name, _) in HOOKS {
        assert_eq!(carries(name), hooks_expected(), "{name} in the executable");
    }
    for (name, _) in TEST_ONLY {
        assert!(!carries(name), "{name} is in the executable");
    }
}
