use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

const BASELINE_VERSION: &str = "0.4.2";
const BASELINE_REPOSITORY: &str = "https://github.com/underpass-ai/kmp.git";

struct RealHostLifecycleHarness {
    root: tempfile::TempDir,
    repository: PathBuf,
    binary: PathBuf,
}

impl RealHostLifecycleHarness {
    fn new() -> Self {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf();
        let scratch = repository.join("tmp");
        fs::create_dir_all(&scratch).expect("workspace scratch directory");
        Self {
            root: tempfile::Builder::new()
                .prefix("lifecycle-real-hosts.")
                .tempdir_in(scratch)
                .expect("isolated lifecycle root"),
            repository,
            binary: PathBuf::from(env!("CARGO_BIN_EXE_kmp-mcp")),
        }
    }

    fn execute(&self) {
        self.require_host("claude", &["--version"]);
        self.require_host("codex", &["--version"]);
        self.create_directories();

        let old_product = self.root.path().join("old-product");
        let baseline_tag = format!("v{BASELINE_VERSION}");
        self.git(&[
            "clone",
            "--branch",
            &baseline_tag,
            "--single-branch",
            BASELINE_REPOSITORY,
            old_product.to_str().expect("old product path"),
        ]);

        let candidate_product = self.root.path().join("candidate-product");
        self.materialize_candidate(&candidate_product);
        let marketplace = self.root.path().join("marketplace");
        self.materialize_marketplace(&marketplace, &old_product, BASELINE_VERSION);

        self.host(
            "claude",
            &["plugin", "marketplace", "add", self.path(&marketplace)],
        );
        self.host(
            "claude",
            &[
                "plugin",
                "install",
                "kmp@underpass",
                "--scope",
                "user",
                "--yes",
            ],
        );
        self.host(
            "codex",
            &[
                "plugin",
                "marketplace",
                "add",
                self.path(&marketplace),
                "--json",
            ],
        );
        self.host("codex", &["plugin", "add", "kmp@underpass", "--json"]);

        let baseline = self.lifecycle("setup", BASELINE_VERSION);
        Self::assert_receipt(&baseline, "setup", BASELINE_VERSION, false);
        self.assert_memory_empty("after-setup");

        let candidate_version = env!("CARGO_PKG_VERSION");
        self.materialize_marketplace(&marketplace, &candidate_product, candidate_version);
        let updated = self.lifecycle("update", candidate_version);
        Self::assert_receipt(&updated, "update", candidate_version, true);
        self.assert_memory_empty("after-update");

        // Once the native managers have replaced the 0.4.2 plugin, prove that
        // the installed updater is only a thin entrance into the Rust use case.
        let repeated = self.plugin_lifecycle(candidate_version);
        Self::assert_receipt(&repeated, "update", candidate_version, false);
        self.assert_memory_empty("after-plugin-update");

        let doctor = self.host(self.path(&self.shared_binary()), &["doctor"]);
        let diagnosis = String::from_utf8_lossy(&doctor.stdout);
        for clause in [
            "claude: effective MCP registration is usable",
            "codex: effective MCP registration is usable",
            "effective engine answers all 13 tools",
            "plugin trees are byte-for-byte identical",
            "Usable.",
        ] {
            assert!(
                diagnosis.contains(clause),
                "Doctor omitted `{clause}`:\n{diagnosis}"
            );
        }
    }

    fn create_directories(&self) {
        for relative in [
            "home",
            "claude",
            "codex",
            "xdg-data",
            "xdg-config",
            "xdg-cache",
            "memory",
            "shared",
        ] {
            fs::create_dir_all(self.root.path().join(relative)).expect("isolated directory");
        }
    }

    fn materialize_candidate(&self, destination: &Path) {
        fs::create_dir_all(destination).expect("candidate root");
        let output = self.command(Command::new("git").current_dir(&self.repository).args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "plugins/kmp",
        ]));
        for relative in String::from_utf8_lossy(&output.stdout).lines() {
            let source = self.repository.join(relative);
            if !source.is_file() {
                continue;
            }
            let target = destination.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).expect("candidate parent");
            }
            fs::copy(source, target).expect("candidate plugin file");
        }
        self.initialize_repository(destination);
        self.git_in(
            destination,
            &[
                "tag",
                "-a",
                &format!("v{}", env!("CARGO_PKG_VERSION")),
                "-m",
                &format!("Candidate v{}", env!("CARGO_PKG_VERSION")),
            ],
        );
    }

    fn materialize_marketplace(&self, marketplace: &Path, product: &Path, version: &str) {
        let plugin = marketplace.join("plugins/kmp");
        if plugin.exists() {
            fs::remove_dir_all(&plugin).expect("replace marketplace plugin");
        }
        self.copy_tree(&product.join("plugins/kmp"), &plugin);
        let agents = marketplace.join(".agents/plugins/marketplace.json");
        let claude = marketplace.join(".claude-plugin/marketplace.json");
        fs::create_dir_all(agents.parent().expect("agents parent")).expect("agents directory");
        fs::create_dir_all(claude.parent().expect("claude parent")).expect("claude directory");
        fs::write(
            agents,
            serde_json::to_vec_pretty(&json!({
                "name": "underpass",
                "interface": {"displayName": "Underpass"},
                "plugins": [{
                    "name": "kmp",
                    "source": {"source": "local", "path": "./plugins/kmp"},
                    "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
                    "category": "Developer Tools"
                }]
            }))
            .expect("agents catalog"),
        )
        .expect("write agents catalog");
        fs::write(
            claude,
            serde_json::to_vec_pretty(&json!({
                "name": "underpass",
                "owner": {"name": "Underpass AI", "url": "https://underpassai.com"},
                "plugins": [{
                    "name": "kmp",
                    "description": "KMP lifecycle contract fixture",
                    "source": {
                        "source": "git-subdir",
                        "url": format!("file://{}", product.display()),
                        "path": "plugins/kmp",
                        "ref": format!("v{version}")
                    }
                }]
            }))
            .expect("Claude catalog"),
        )
        .expect("write Claude catalog");

        if marketplace.join(".git").exists() {
            self.git_in(marketplace, &["add", "-A"]);
            self.git_in(
                marketplace,
                &["commit", "-m", &format!("marketplace {version}")],
            );
        } else {
            self.initialize_repository(marketplace);
        }
    }

    fn initialize_repository(&self, repository: &Path) {
        self.git_in(repository, &["init", "-b", "main"]);
        self.git_in(repository, &["config", "user.name", "KMP lifecycle test"]);
        self.git_in(repository, &["config", "user.email", "lifecycle@invalid"]);
        self.git_in(repository, &["add", "-A"]);
        self.git_in(repository, &["commit", "-m", "lifecycle fixture"]);
    }

    fn copy_tree(&self, source: &Path, destination: &Path) {
        fs::create_dir_all(destination).expect("tree destination");
        for entry in fs::read_dir(source).expect("tree source") {
            let entry = entry.expect("tree entry");
            let target = destination.join(entry.file_name());
            if entry.file_type().expect("entry type").is_dir() {
                if entry.file_name() != "bin" {
                    self.copy_tree(&entry.path(), &target);
                }
            } else {
                fs::copy(entry.path(), target).expect("tree file");
            }
        }
    }

    fn lifecycle(&self, action: &str, version: &str) -> Value {
        let binary = self.binary.to_str().expect("binary path");
        let shared = self.root.path().join("shared");
        let output = self.host(
            binary,
            &[
                action,
                "--version",
                version,
                "--engine-dir",
                self.path(&shared),
            ],
        );
        serde_json::from_slice(&output.stdout).expect("lifecycle receipt")
    }

    fn plugin_lifecycle(&self, version: &str) -> Value {
        let inventory = self.host("claude", &["plugin", "list", "--json"]);
        let installations: Value =
            serde_json::from_slice(&inventory.stdout).expect("Claude plugin inventory");
        let plugin_root = installations
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["id"] == "kmp@underpass" || item["name"] == "kmp@underpass")
            })
            .and_then(|item| {
                item["installPath"]
                    .as_str()
                    .or_else(|| item["install_path"].as_str())
            })
            .map(PathBuf::from)
            .expect("Claude installed plugin root");
        let shared = self.root.path().join("shared");
        let binary = self.binary.to_str().expect("candidate binary");
        let output = self.host_with(
            self.path(&plugin_root.join("scripts/kmp-update.sh")),
            &["--engine-dir", self.path(&shared)],
            &[("KMP_MCP_BIN", binary)],
        );
        let receipt: Value =
            serde_json::from_slice(&output.stdout).expect("plugin lifecycle receipt");
        assert_eq!(receipt["version"], version);
        receipt
    }

    fn assert_receipt(receipt: &Value, action: &str, version: &str, changed: bool) {
        assert_eq!(receipt["action"], action);
        assert_eq!(receipt["status"], "completed");
        assert_eq!(receipt["version"], version);
        let hosts = receipt["hosts"].as_array().expect("hosts");
        assert_eq!(hosts.len(), 2);
        assert_eq!(
            hosts
                .iter()
                .filter_map(|host| host["host"].as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["claude", "codex"])
        );
        if changed {
            assert!(
                hosts
                    .iter()
                    .all(|host| host["previous_version"] == BASELINE_VERSION)
            );
        }
        let engines = receipt["engines"].as_array().expect("engines");
        assert_eq!(engines.len(), 2);
        assert_eq!(
            engines
                .iter()
                .filter_map(|engine| engine["consumer"].as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["claude", "codex"])
        );
        assert!(engines.iter().all(|engine| engine["version"] == version));
        assert!(engines.iter().all(|engine| engine["tool_count"] == 13));
        assert!(
            receipt["plugin_tree_digest"]
                .as_str()
                .is_some_and(|digest| digest.starts_with("sha256:"))
        );
    }

    fn shared_binary(&self) -> PathBuf {
        self.root.path().join("shared/kmp-mcp")
    }

    fn assert_memory_empty(&self, label: &str) {
        let export = self.root.path().join(format!("{label}.jsonl"));
        self.host(
            self.path(&self.shared_binary()),
            &["export", self.path(&export)],
        );
        let header = fs::read_to_string(export).expect("lifecycle memory export");
        let header: Value = serde_json::from_str(header.lines().next().expect("bundle header"))
            .expect("bundle header JSON");
        assert_eq!(
            header["event_count"], 0,
            "software lifecycle wrote memory {label}"
        );
    }

    fn require_host(&self, program: &str, arguments: &[&str]) {
        let output = Command::new(program).args(arguments).output();
        assert!(
            output.is_ok_and(|output| output.status.success()),
            "real lifecycle test requires `{program}`"
        );
    }

    fn git(&self, arguments: &[&str]) -> Output {
        self.command(Command::new("git").args(arguments))
    }

    fn git_in(&self, repository: &Path, arguments: &[&str]) -> Output {
        self.command(Command::new("git").current_dir(repository).args(arguments))
    }

    fn host(&self, program: &str, arguments: &[&str]) -> Output {
        self.host_with(program, arguments, &[])
    }

    fn host_with(&self, program: &str, arguments: &[&str], env: &[(&str, &str)]) -> Output {
        let inherited_path = std::env::var("PATH").unwrap_or_default();
        let path = format!(
            "{}:{inherited_path}",
            self.root.path().join("shared").display()
        );
        let mut command = Command::new(program);
        command
            .args(arguments)
            .env("HOME", self.root.path().join("home"))
            .env("CLAUDE_CONFIG_DIR", self.root.path().join("claude"))
            .env("CODEX_HOME", self.root.path().join("codex"))
            .env("XDG_DATA_HOME", self.root.path().join("xdg-data"))
            .env("XDG_CONFIG_HOME", self.root.path().join("xdg-config"))
            .env("XDG_CACHE_HOME", self.root.path().join("xdg-cache"))
            .env("KMP_MCP_DATA_DIR", self.root.path().join("memory"))
            .env("PATH", path);
        for (name, value) in env {
            command.env(name, value);
        }
        self.command(&mut command)
    }

    fn command(&self, command: &mut Command) -> Output {
        let output = command.output().expect("start lifecycle command");
        assert!(
            output.status.success(),
            "command failed ({:?}):\nstdout={}\nstderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn path<'a>(&self, path: &'a Path) -> &'a str {
        path.to_str().expect("UTF-8 test path")
    }
}

#[test]
#[ignore = "requires real Claude Code and Codex plugin managers plus release downloads"]
fn clean_install_and_update_from_0_4_2_converge_both_real_hosts() {
    RealHostLifecycleHarness::new().execute();
}
