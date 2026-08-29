#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

struct PluginBootstrapHarness {
    root: tempfile::TempDir,
    repository: PathBuf,
}

impl PluginBootstrapHarness {
    fn new() -> Self {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf();
        Self {
            root: tempfile::tempdir().expect("isolated bootstrap root"),
            repository,
        }
    }

    fn execute(&self) {
        self.clean_setup();
        self.update_rejects_stale_path_engine();
    }

    fn clean_setup(&self) {
        let tools = self.root.path().join("tools");
        let install = self.root.path().join("install");
        let home = self.root.path().join("home");
        let plugin = self.root.path().join("plugin");
        fs::create_dir_all(&tools).expect("tool directory");
        fs::create_dir_all(&home).expect("home directory");
        fs::create_dir_all(plugin.join("scripts")).expect("plugin scripts");
        fs::create_dir_all(plugin.join(".codex-plugin")).expect("plugin manifest directory");
        fs::copy(
            self.repository
                .join("plugins/kmp/.codex-plugin/plugin.json"),
            plugin.join(".codex-plugin/plugin.json"),
        )
        .expect("plugin manifest");
        let bootstrap = plugin.join("scripts/kmp-install-binary.sh");
        fs::copy(
            self.repository
                .join("plugins/kmp/scripts/kmp-install-binary.sh"),
            &bootstrap,
        )
        .expect("bootstrap adapter");
        let mut bootstrap_permissions = fs::metadata(&bootstrap)
            .expect("bootstrap metadata")
            .permissions();
        bootstrap_permissions.set_mode(0o755);
        fs::set_permissions(&bootstrap, bootstrap_permissions).expect("bootstrap executable");
        self.write_executable(
            &tools.join("curl"),
            r#"#!/bin/sh
set -eu
url=
output=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) output=$2; shift 2 ;;
    https://*) url=$1; shift ;;
    *) shift ;;
  esac
done
printf '%s\n' "$url" >> "$KMP_TEST_CURL_LOG"
case "$url" in
  *.sha256) cp "$KMP_TEST_SHA" "$output" ;;
  *) cp "$KMP_TEST_ASSET" "$output" ;;
esac
"#,
        );
        self.write_executable(
            &tools.join("claude"),
            r#"#!/bin/sh
if [ "$*" = "plugin list --json" ]; then
  printf '[]\n'
  exit 0
fi
exit 1
"#,
        );

        let engine = PathBuf::from(env!("CARGO_BIN_EXE_kmp-mcp"));
        let checksum = format!("{:x}", Sha256::digest(fs::read(&engine).expect("engine")));
        let checksum_path = self.root.path().join("engine.sha256");
        fs::write(&checksum_path, format!("{checksum}  kmp-mcp\n")).expect("checksum");
        let curl_log = self.root.path().join("curl.log");
        let path = format!("{}:/usr/bin:/bin", tools.display());
        let output = Command::new(&bootstrap)
            .args(["setup", "--claude", "--dry-run"])
            .env("HOME", &home)
            .env("PATH", path)
            .env("KMP_INSTALL_DIR", &install)
            .env("KMP_TEST_ASSET", &engine)
            .env("KMP_TEST_SHA", &checksum_path)
            .env("KMP_TEST_CURL_LOG", &curl_log)
            .env("CLAUDE_CONFIG_DIR", self.root.path().join("claude"))
            .env("CODEX_HOME", self.root.path().join("codex"))
            .env("XDG_DATA_HOME", self.root.path().join("xdg-data"))
            .env("XDG_CONFIG_HOME", self.root.path().join("xdg-config"))
            .env("XDG_CACHE_HOME", self.root.path().join("xdg-cache"))
            .env("KMP_MCP_DATA_DIR", self.root.path().join("memory"))
            .output()
            .expect("bootstrap starts");
        assert!(
            output.status.success(),
            "stdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let receipt: Value = serde_json::from_slice(&output.stdout).expect("lifecycle receipt");
        assert_eq!(receipt["action"], "setup");
        assert_eq!(receipt["status"], "planned");
        assert_eq!(receipt["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(
            fs::read(install.join("kmp-mcp")).expect("installed engine"),
            fs::read(engine).expect("source engine")
        );
        assert!(
            !self.root.path().join("memory").exists(),
            "software bootstrap must not select or create memory"
        );

        let requests = fs::read_to_string(curl_log).expect("download log");
        let expected_asset = format!(
            "/releases/download/v{0}/kmp-mcp-v{0}-{1}",
            env!("CARGO_PKG_VERSION"),
            Self::target()
        );
        assert_eq!(requests.lines().count(), 2);
        assert!(requests.lines().all(|url| url.contains(&expected_asset)));
    }

    fn update_rejects_stale_path_engine(&self) {
        let case = self.root.path().join("stale-update");
        let tools = case.join("tools");
        let install = case.join("install");
        let plugin = case.join("plugin");
        fs::create_dir_all(&tools).expect("stale tool directory");
        fs::create_dir_all(plugin.join("scripts")).expect("stale plugin scripts");
        fs::create_dir_all(plugin.join(".codex-plugin")).expect("stale manifest directory");
        fs::copy(
            self.repository
                .join("plugins/kmp/.codex-plugin/plugin.json"),
            plugin.join(".codex-plugin/plugin.json"),
        )
        .expect("stale plugin manifest");
        for script in ["kmp-install-binary.sh", "kmp-update.sh"] {
            let target = plugin.join("scripts").join(script);
            fs::copy(
                self.repository.join("plugins/kmp/scripts").join(script),
                &target,
            )
            .expect("bootstrap script");
            let mut permissions = fs::metadata(&target)
                .expect("script metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(target, permissions).expect("script executable");
        }
        self.write_executable(
            &tools.join("kmp-mcp"),
            "#!/bin/sh\nprintf 'kmp-mcp 0.4.2\\n'\n",
        );
        self.write_executable(
            &tools.join("curl"),
            r#"#!/bin/sh
set -eu
url=
output=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) output=$2; shift 2 ;;
    https://*) url=$1; shift ;;
    *) shift ;;
  esac
done
printf '%s\n' "$url" >> "$KMP_TEST_CURL_LOG"
case "$url" in
  *.sha256) cp "$KMP_TEST_SHA" "$output" ;;
  *) cp "$KMP_TEST_ASSET" "$output" ;;
esac
"#,
        );
        self.write_executable(
            &tools.join("claude"),
            &format!(
                r#"#!/bin/sh
if [ "$*" = "plugin list --json" ]; then
  printf '%s\n' '[{{"id":"kmp@underpass","version":"0.4.2","enabled":true,"installPath":"{}"}}]'
  exit 0
fi
exit 1
"#,
                case.join("claude-plugin").display()
            ),
        );

        let engine = PathBuf::from(env!("CARGO_BIN_EXE_kmp-mcp"));
        let checksum = format!("{:x}", Sha256::digest(fs::read(&engine).expect("engine")));
        let checksum_path = case.join("engine.sha256");
        fs::write(&checksum_path, format!("{checksum}  kmp-mcp\n")).expect("checksum");
        let curl_log = case.join("curl.log");
        let output = Command::new(plugin.join("scripts/kmp-update.sh"))
            .args(["--claude", "--dry-run"])
            .env("HOME", case.join("home"))
            .env("PATH", format!("{}:/usr/bin:/bin", tools.display()))
            .env("KMP_INSTALL_DIR", &install)
            .env("KMP_TEST_ASSET", &engine)
            .env("KMP_TEST_SHA", &checksum_path)
            .env("KMP_TEST_CURL_LOG", &curl_log)
            .env("CLAUDE_CONFIG_DIR", case.join("claude"))
            .env("CODEX_HOME", case.join("codex"))
            .env("XDG_DATA_HOME", case.join("xdg-data"))
            .env("XDG_CONFIG_HOME", case.join("xdg-config"))
            .env("XDG_CACHE_HOME", case.join("xdg-cache"))
            .env("KMP_MCP_DATA_DIR", case.join("memory"))
            .output()
            .expect("stale update starts");
        assert!(
            output.status.success(),
            "stdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let receipt: Value = serde_json::from_slice(&output.stdout).expect("update receipt");
        assert_eq!(receipt["action"], "update");
        assert_eq!(receipt["status"], "planned");
        assert_eq!(receipt["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(receipt["hosts"][0]["previous_version"], "0.4.2");
        assert_eq!(
            fs::read(install.join("kmp-mcp")).expect("replacement engine"),
            fs::read(engine).expect("current engine")
        );
        assert_eq!(
            fs::read_to_string(curl_log)
                .expect("stale update requests")
                .lines()
                .count(),
            2,
            "a stale PATH engine must force a verified current download"
        );
        assert!(
            !case.join("memory").exists(),
            "software update must not select or create memory"
        );
    }

    fn target() -> &'static str {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            platform => panic!("unsupported bootstrap test platform: {platform:?}"),
        }
    }

    fn write_executable(&self, path: &Path, body: &str) {
        fs::write(path, body).expect("write executable fixture");
        let mut permissions = fs::metadata(path).expect("fixture metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("fixture executable");
    }
}

#[test]
fn clean_source_marketplace_install_bootstraps_a_verified_rust_engine() {
    PluginBootstrapHarness::new().execute();
}
