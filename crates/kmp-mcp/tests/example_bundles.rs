//! Every bundle the repository ships as an example — and its own memory —
//! replays into an empty store with this binary. A bundle a reader cannot
//! import is not an example.
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn shipped_bundles() -> Vec<PathBuf> {
    let mut bundles = vec![repository().join(".kmp/memory.jsonl")];
    let examples = std::fs::read_dir(repository().join("examples")).expect("examples directory");
    for entry in examples.flatten() {
        let bundle = entry.path().join("memory.jsonl");
        if bundle.is_file() {
            bundles.push(bundle);
        }
    }
    bundles.sort();
    bundles
}

#[test]
fn every_shipped_bundle_imports_into_an_empty_store() {
    let bundles = shipped_bundles();
    assert!(bundles.len() >= 2, "{bundles:?}");
    for bundle in bundles {
        let scratch = tempfile::tempdir().expect("scratch");
        let store = scratch.path().join("store");
        let output = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"))
            .arg("import")
            .arg(&bundle)
            .env("KMP_MCP_DATA_DIR", &store)
            .env("HOME", scratch.path().join("home"))
            .env("XDG_CONFIG_HOME", scratch.path().join("home/.config"))
            .env("XDG_DATA_HOME", scratch.path().join("home/.local/share"))
            .env("KMP_VIEWER_ADDR", "off")
            .output()
            .expect("import runs");
        assert!(
            output.status.success(),
            "{}: {}",
            bundle.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let text = std::fs::read_to_string(&bundle).expect("bundle text");
        let header: Value =
            serde_json::from_str(text.lines().next().expect("header")).expect("header JSON");
        assert!(
            header["event_count"].as_u64().unwrap_or(0) >= 1,
            "{}",
            bundle.display()
        );
        assert!(
            !header["abouts"].as_array().is_none_or(Vec::is_empty),
            "{}",
            bundle.display()
        );
    }
}
