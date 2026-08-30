use std::fs;
use std::path::Path;

use kmp_release::adapters::system_file_system::SystemFileSystem;
use kmp_release::application::use_cases::prepare_release_version::PrepareReleaseVersion;
use kmp_release::domain::release_version::ReleaseVersion;
use kmp_release::domain::repository_root::RepositoryRoot;
use serde_json::Value;

struct VersionPreparationHarness {
    root: tempfile::TempDir,
}

impl VersionPreparationHarness {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("version fixture");
        for directory in [
            ".claude-plugin",
            "distribution/charts/kmp",
            "distribution/mcpb",
            "plugins/kmp/.claude-plugin",
            "plugins/kmp/.codex-plugin",
        ] {
            fs::create_dir_all(root.path().join(directory)).expect("fixture directory");
        }
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"0.4.2\"\n\n[workspace.dependencies]\nkmp-domain = { path = \"crates/kmp-domain\", version = \"0.4.2\" }\nkmp-private = { path = \"crates/kmp-private\" }\n",
        )
        .expect("Cargo fixture");
        fs::write(
            root.path().join("distribution/charts/kmp/Chart.yaml"),
            "apiVersion: v2\nversion: 0.4.2\nappVersion: \"0.4.2\"\n",
        )
        .expect("chart fixture");
        for host in [".claude-plugin", ".codex-plugin"] {
            fs::write(
                root.path().join(format!("plugins/kmp/{host}/plugin.json")),
                "{\n  \"name\": \"kmp\",\n  \"version\": \"0.4.2\"\n}\n",
            )
            .expect("plugin manifest fixture");
        }
        fs::write(
            root.path().join("server.json"),
            r#"{
  "version": "0.4.2",
  "packages": [
    {"registryType": "mcpb", "identifier": "old", "fileSha256": "live"},
    {"registryType": "cargo", "identifier": "kmp-mcp", "version": "0.4.2"}
  ]
}
"#,
        )
        .expect("server fixture");
        fs::write(
            root.path().join("distribution/mcpb/manifest.json"),
            "{\n  \"name\": \"kmp\",\n  \"version\": \"0.4.2\"\n}\n",
        )
        .expect("MCPB fixture");
        fs::write(
            root.path().join(".claude-plugin/marketplace.json"),
            r#"{
  "name": "underpass",
  "plugins": [
    {
      "name": "kmp",
      "description": "Local-first agent memory with a shared ChronoLoom view.",
      "source": {
        "source": "git-subdir",
        "url": "https://github.com/underpass-ai/kmp.git",
        "path": "plugins/kmp",
        "ref": "v0.4.2"
      }
    }
  ]
}
"#,
        )
        .expect("Claude catalog fixture");
        Self { root }
    }

    fn execute(&self) {
        let root = RepositoryRoot::from_path(self.root.path()).expect("repository root");
        let version = ReleaseVersion::parse("0.5.2").expect("target version");
        let receipt = PrepareReleaseVersion::new(&SystemFileSystem)
            .execute(&root, &version)
            .expect("version preparation");
        assert_eq!(receipt.internal_dependencies(), 1);
        assert!(receipt.mcpb_hash_was_reset());
        assert_eq!(receipt.catalog_ref(), "v0.5.2");

        let cargo = self.read("Cargo.toml");
        assert!(cargo.contains("version = \"0.5.2\""));
        assert!(
            cargo.contains("kmp-domain = { path = \"crates/kmp-domain\", version = \"0.5.2\" }")
        );
        assert!(cargo.contains("kmp-private = { path = \"crates/kmp-private\" }"));
        let chart = self.read("distribution/charts/kmp/Chart.yaml");
        assert!(chart.contains("version: 0.5.2"));
        assert!(chart.contains("appVersion: \"0.5.2\""));

        for path in [
            "plugins/kmp/.claude-plugin/plugin.json",
            "plugins/kmp/.codex-plugin/plugin.json",
            "distribution/mcpb/manifest.json",
        ] {
            let body: Value = serde_json::from_str(&self.read(path)).expect("versioned JSON");
            assert_eq!(body["version"], "0.5.2");
        }
        let server: Value = serde_json::from_str(&self.read("server.json")).expect("server JSON");
        assert_eq!(server["version"], "0.5.2");
        assert_eq!(server["packages"][0]["fileSha256"], "0".repeat(64));
        assert_eq!(server["packages"][1]["version"], "0.5.2");

        // The catalog ref pins the tag Claude Code clones and is itself a
        // candidate input, so the version change has to own it.
        let catalog = self.read(".claude-plugin/marketplace.json");
        assert!(catalog.contains("\"ref\": \"v0.5.2\""));
        let catalog: Value = serde_json::from_str(&catalog).expect("catalog JSON");
        assert_eq!(catalog["plugins"][0]["source"]["ref"], "v0.5.2");
        assert_eq!(catalog["plugins"][0]["source"]["path"], "plugins/kmp");
    }

    fn read(&self, relative: impl AsRef<Path>) -> String {
        fs::read_to_string(self.root.path().join(relative)).expect("fixture file")
    }
}

#[test]
fn one_rust_use_case_prepares_every_release_version_surface() {
    VersionPreparationHarness::new().execute();
}
