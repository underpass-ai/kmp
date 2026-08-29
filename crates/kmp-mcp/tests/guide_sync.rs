use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn plugin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/kmp")
}

fn run_guide(store: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kmp-mcp"))
        .args(["guide", "sync", "--plugin-root"])
        .arg(plugin_root())
        .args(extra)
        .env("KMP_MCP_DATA_DIR", store)
        .env("KMP_VIEWER_ADDR", "off")
        .output()
        .expect("guide command runs")
}

#[test]
fn guide_sync_is_an_explicit_idempotent_two_about_write() {
    let scratch = tempfile::tempdir().expect("scratch directory");
    let store = scratch.path().join("store");
    for _ in 0..2 {
        let output = run_guide(&store, &[]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("converged 2 immutable guide memories")
        );
    }

    let bundle = scratch.path().join("memory.jsonl");
    let output = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"))
        .arg("export")
        .arg(&bundle)
        .env("KMP_MCP_DATA_DIR", &store)
        .env("KMP_VIEWER_ADDR", "off")
        .output()
        .expect("guide store exports");
    assert!(output.status.success());
    let text = std::fs::read_to_string(bundle).expect("exported bundle");
    let header: Value = serde_json::from_str(text.lines().next().expect("bundle header"))
        .expect("valid bundle header");
    assert_eq!(header["event_count"], 2);
    assert_eq!(
        header["abouts"],
        serde_json::json!(["guide:kmp", "guide:kmp-agent"])
    );
}

#[test]
fn guide_dry_run_validates_assets_without_creating_a_store() {
    let scratch = tempfile::tempdir().expect("scratch directory");
    let store = scratch.path().join("store");
    let output = run_guide(&store, &["--dry-run"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("would converge 2"));
    assert!(!store.exists(), "dry-run must not select or create a store");
}
