use std::fs;
use std::path::{Path, PathBuf};

use kmp_release::adapters::system_file_system::SystemFileSystem;
use kmp_release::application::use_cases::check_release_readiness::CheckReleaseReadiness;
use kmp_release::domain::branch_name::BranchName;
use kmp_release::domain::candidate_input_digest::CandidateInputDigest;
use kmp_release::domain::release_error::ReleaseError;
use kmp_release::domain::release_readiness::ReleaseReadiness;
use kmp_release::domain::release_version::ReleaseVersion;
use kmp_release::domain::repository_root::RepositoryRoot;
use kmp_release::domain::source_commit::SourceCommit;
use kmp_release::ports::release_contracts::ReleaseContracts;
use kmp_release::ports::release_workspace::ReleaseWorkspace;

const TARGET: &str = "0.6.1";
const HEAD: &str = "1111111111111111111111111111111111111111";
const DIGEST: &str = "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";

/// A workspace whose git state and shell gates are all healthy, so the
/// interesting failures are the ones the fixture tree puts there.
struct ReadyWorkspace;

impl ReleaseWorkspace for ReadyWorkspace {
    fn refresh_lockfile(&self) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn build_engine(&self) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn show_version_diff(&self) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn require_clean(&self) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn current_branch(&self) -> Result<BranchName, ReleaseError> {
        BranchName::parse("chore/prepare-0.6.1")
    }

    fn head_commit(&self) -> Result<SourceCommit, ReleaseError> {
        SourceCommit::parse(HEAD)
    }

    fn upstream_commit(&self) -> Result<Option<SourceCommit>, ReleaseError> {
        SourceCommit::parse(HEAD).map(Some)
    }

    fn verify_registry(&self) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn verify_vendored_contract(&self) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn verify_publish_chain(&self) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn changed_files_since(&self, _commit: &SourceCommit) -> Result<Vec<PathBuf>, ReleaseError> {
        Ok(Vec::new())
    }

    fn tag_exists(&self, _version: &ReleaseVersion) -> Result<bool, ReleaseError> {
        Ok(false)
    }

    fn create_and_push_tag(
        &self,
        _version: &ReleaseVersion,
        _run_id: &kmp_release::domain::workflow_run_id::WorkflowRunId,
        _input: &CandidateInputDigest,
    ) -> Result<(), ReleaseError> {
        Ok(())
    }
}

struct StubContracts;

impl ReleaseContracts for StubContracts {
    fn sync_readmes(&self) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn prepare_changelog(&self, _version: &ReleaseVersion) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn check_changelog(&self, _version: &ReleaseVersion) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn prepare_version(&self, _version: &ReleaseVersion) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn workspace_version(&self) -> Result<ReleaseVersion, ReleaseError> {
        ReleaseVersion::parse(TARGET)
    }

    fn sync_guide(&self, _version: &ReleaseVersion, _binary: &Path) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn stamp_mcpb(&self, _archive: &Path) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn candidate_inputs(&self) -> Result<CandidateInputDigest, ReleaseError> {
        CandidateInputDigest::parse(DIGEST)
    }

    fn verify_candidate(
        &self,
        _version: &ReleaseVersion,
        _directory: &Path,
        _input: &CandidateInputDigest,
        _run_id: &kmp_release::domain::workflow_run_id::WorkflowRunId,
    ) -> Result<(), ReleaseError> {
        Ok(())
    }

    fn verify_marketplace(
        &self,
        _version: &ReleaseVersion,
        _expected_commit: &SourceCommit,
    ) -> Result<(), ReleaseError> {
        Ok(())
    }
}

struct ReadinessHarness {
    root: tempfile::TempDir,
}

impl ReadinessHarness {
    /// A tree already prepared for 0.6.1, as `scripts/release.sh version`
    /// leaves it.
    fn prepared() -> Self {
        let root = tempfile::tempdir().expect("readiness fixture");
        for directory in [
            ".agents/plugins",
            ".claude-plugin",
            "distribution/charts/kmp",
            "distribution/mcpb",
            "plugins/kmp/.claude-plugin",
            "plugins/kmp/.codex-plugin",
            "plugins/kmp/guide",
        ] {
            fs::create_dir_all(root.path().join(directory)).expect("fixture directory");
        }
        let harness = Self { root };
        harness.write(
            "CHANGELOG.md",
            "# Changelog\n\n## [Unreleased]\n\n## [0.6.1] - 2026-08-30\n\n### Fixed\n\n- the catalog ref moves with the version.\n\n[0.6.1]: https://github.com/underpass-ai/kmp/compare/v0.6.0...v0.6.1\n",
        );
        harness.write(
            "Cargo.toml",
            "[workspace.package]\nversion = \"0.6.1\"\n\n[workspace.dependencies]\nkmp-domain = { path = \"crates/kmp-domain\", version = \"0.6.1\" }\n",
        );
        harness.write(
            "distribution/charts/kmp/Chart.yaml",
            "apiVersion: v2\nversion: 0.6.1\nappVersion: \"0.6.1\"\n",
        );
        for host in [".claude-plugin", ".codex-plugin"] {
            harness.write(
                &format!("plugins/kmp/{host}/plugin.json"),
                "{\n  \"name\": \"kmp\",\n  \"version\": \"0.6.1\"\n}\n",
            );
        }
        harness.write(
            "distribution/mcpb/manifest.json",
            "{\n  \"name\": \"kmp\",\n  \"version\": \"0.6.1\"\n}\n",
        );
        harness.write(
            "server.json",
            r#"{
  "version": "0.6.1",
  "packages": [
    {
      "registryType": "mcpb",
      "identifier": "https://github.com/underpass-ai/kmp/releases/download/v0.6.1/kmp-mcp-v0.6.1.mcpb",
      "fileSha256": "0000000000000000000000000000000000000000000000000000000000000000"
    },
    {"registryType": "cargo", "identifier": "kmp-mcp", "version": "0.6.1"}
  ]
}
"#,
        );
        harness.write(
            "plugins/kmp/guide/memory.jsonl",
            "{\"bundle_format\":2,\"event_count\":2,\"kernel_version\":\"0.6.1\",\"abouts\":[\"guide:kmp\"]}\n",
        );
        harness.write(
            ".claude-plugin/marketplace.json",
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
        "ref": "v0.6.1"
      }
    }
  ]
}
"#,
        );
        harness.write(
            ".agents/plugins/marketplace.json",
            r#"{
  "name": "underpass",
  "plugins": [
    {
      "name": "kmp",
      "source": {"source": "local", "path": "./plugins/kmp"}
    }
  ]
}
"#,
        );
        harness
    }

    fn write(&self, relative: &str, content: &str) {
        fs::write(self.root.path().join(relative), content).expect("fixture file");
    }

    fn readiness(&self) -> ReleaseReadiness {
        let root = RepositoryRoot::from_path(self.root.path()).expect("repository root");
        let version = ReleaseVersion::parse(TARGET).expect("target version");
        CheckReleaseReadiness::new(&SystemFileSystem, &StubContracts, &ReadyWorkspace, &root)
            .execute(&version)
    }
}

#[test]
fn a_prepared_tree_is_ready_before_anything_is_built() {
    let readiness = ReadinessHarness::prepared().readiness();

    assert!(readiness.is_ready(), "{readiness}");
    assert!(
        readiness
            .checks()
            .iter()
            .any(|check| check.name() == "candidate inputs" && check.detail().contains(DIGEST))
    );
}

#[test]
fn every_static_failure_is_reported_in_one_run() {
    let harness = ReadinessHarness::prepared();
    // Exactly what releasing 0.6.0 hit: the catalog still pins the previous tag,
    // and it was found one release attempt at a time.
    harness.write(
        ".claude-plugin/marketplace.json",
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
        "ref": "v0.6.0"
      }
    }
  ]
}
"#,
    );
    harness.write("CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n");

    let readiness = harness.readiness();

    assert!(!readiness.is_ready());
    let failed = readiness
        .failures()
        .iter()
        .map(|check| check.name().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        failed,
        vec!["changelog", "version sources", "marketplace catalogs"]
    );
    let report = readiness.to_string();
    assert!(
        report.contains(".claude-plugin/marketplace.json ref is v0.6.0, not v0.6.1"),
        "{report}"
    );
    assert!(
        report.contains("missing release section ## [0.6.1]"),
        "{report}"
    );
}

#[test]
fn a_half_bumped_tree_names_every_source_left_behind() {
    let harness = ReadinessHarness::prepared();
    harness.write(
        "distribution/charts/kmp/Chart.yaml",
        "apiVersion: v2\nversion: 0.6.0\nappVersion: \"0.6.1\"\n",
    );
    harness.write(
        "plugins/kmp/guide/memory.jsonl",
        "{\"bundle_format\":2,\"event_count\":2,\"kernel_version\":\"0.6.0\",\"abouts\":[\"guide:kmp\"]}\n",
    );

    let readiness = harness.readiness();

    let report = readiness.to_string();
    assert!(
        report.contains("Chart.yaml version is 0.6.0, not 0.6.1"),
        "{report}"
    );
    assert!(
        report.contains("guide envelope kernel_version is 0.6.0, not 0.6.1"),
        "{report}"
    );
    // appVersion was bumped, so the report must not accuse it.
    assert!(!report.contains("Chart.yaml appVersion is"), "{report}");
    assert_eq!(readiness.failures().len(), 1);
}
