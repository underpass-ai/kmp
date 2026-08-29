use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::json;

struct PluginNoticeHarness {
    root: tempfile::TempDir,
    binary: PathBuf,
}

impl PluginNoticeHarness {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().expect("notice root"),
            binary: PathBuf::from(env!("CARGO_BIN_EXE_kmp-mcp")),
        }
    }

    fn execute(&self) {
        let current = env!("CARGO_PKG_VERSION");
        let plugin = self.plugin(current);
        let quiet = self.notice(&plugin, current);
        assert!(quiet.status.success());
        assert!(quiet.stdout.is_empty());

        let available = self.notice(&plugin, "0.99.0");
        assert!(available.status.success());
        assert!(String::from_utf8_lossy(&available.stdout).contains("0.99.0 is available"));

        let stale = self.plugin("0.4.2");
        let mismatch = self.notice(&stale, "0.99.0");
        assert!(mismatch.status.success());
        let mismatch = String::from_utf8_lossy(&mismatch.stdout);
        assert!(mismatch.contains(&format!("engine {current}, plugin 0.4.2")));
        assert!(mismatch.contains("/kmp:setup"));
    }

    fn plugin(&self, version: &str) -> PathBuf {
        let root = self.root.path().join(format!("plugin-{version}"));
        fs::create_dir_all(root.join(".codex-plugin")).expect("manifest directory");
        fs::write(
            root.join(".codex-plugin/plugin.json"),
            serde_json::to_vec(&json!({"name": "kmp", "version": version})).expect("manifest JSON"),
        )
        .expect("manifest");
        root
    }

    fn notice(&self, plugin: &Path, latest: &str) -> Output {
        Command::new(&self.binary)
            .args([
                "plugin",
                "notice",
                "--plugin-root",
                plugin.to_str().expect("plugin path"),
            ])
            .env("KMP_LATEST_VERSION", latest)
            .env("XDG_CACHE_HOME", self.root.path().join("cache"))
            .output()
            .expect("notice command")
    }
}

#[test]
fn plugin_notice_is_a_non_mutating_rust_use_case() {
    PluginNoticeHarness::new().execute();
}
