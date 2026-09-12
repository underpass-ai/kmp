//! #723: the engine handed to guide sync is the one Cargo just built.
//!
//! Cargo picks its output directory from `CARGO_TARGET_DIR` or from
//! `build.target-dir`. A release step that names `target/debug/kmp-mcp` itself
//! is guessing, and the guess is wrong for every checkout that shares a build
//! cache — the build succeeds and the next step reports a missing binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use kmp_release::adapters::system_release_workspace::SystemReleaseWorkspace;
use kmp_release::application::use_cases::prepare_release_workflow::PrepareReleaseWorkflow;
use kmp_release::domain::branch_name::BranchName;
use kmp_release::domain::candidate_input_digest::CandidateInputDigest;
use kmp_release::domain::release_error::ReleaseError;
use kmp_release::domain::release_version::ReleaseVersion;
use kmp_release::domain::repository_root::RepositoryRoot;
use kmp_release::domain::source_commit::SourceCommit;
use kmp_release::domain::workflow_run_id::WorkflowRunId;
use kmp_release::ports::release_contracts::ReleaseContracts;
use kmp_release::ports::release_workspace::ReleaseWorkspace;

const HEAD: &str = "b6a469f0b4591b93f690a824ca239e8b68b8ba24";

fn version() -> ReleaseVersion {
    ReleaseVersion::parse("0.17.1".to_string()).expect("version")
}

/// A workspace whose build lands where Cargo was configured to put it, not
/// under `<root>/target`.
struct ConfiguredTargetWorkspace {
    engine: PathBuf,
}

impl ConfiguredTargetWorkspace {
    /// Stands in for `CARGO_TARGET_DIR=<cache>`: the build writes the engine
    /// into a cache outside the checkout and reports where it went.
    fn building_into(cache: &Path) -> Self {
        let engine = cache.join("debug/kmp-mcp");
        Self { engine }
    }

    fn engine(&self) -> &Path {
        &self.engine
    }
}

impl ReleaseWorkspace for ConfiguredTargetWorkspace {
    fn refresh_lockfile(&self) -> Result<(), ReleaseError> {
        Ok(())
    }
    fn build_engine(&self) -> Result<PathBuf, ReleaseError> {
        fs::create_dir_all(self.engine.parent().expect("engine parent")).expect("cache directory");
        fs::write(&self.engine, b"built engine").expect("engine");
        Ok(self.engine.clone())
    }
    fn show_version_diff(&self) -> Result<(), ReleaseError> {
        Ok(())
    }
    fn require_clean(&self) -> Result<(), ReleaseError> {
        Ok(())
    }
    fn current_branch(&self) -> Result<BranchName, ReleaseError> {
        BranchName::parse("release/0.17.1".to_string())
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
    fn commit_tracked(&self, _message: &str) -> Result<bool, ReleaseError> {
        Ok(true)
    }
    fn push_current_branch(&self) -> Result<(), ReleaseError> {
        Ok(())
    }
    fn advance_branch(
        &self,
        _branch: &BranchName,
        _commit: &SourceCommit,
    ) -> Result<(), ReleaseError> {
        Ok(())
    }
    fn create_and_push_tag(
        &self,
        _version: &ReleaseVersion,
        _run_id: &WorkflowRunId,
        _input: &CandidateInputDigest,
    ) -> Result<(), ReleaseError> {
        Ok(())
    }
}

/// Contracts that keep the engine path guide sync was given.
#[derive(Default)]
struct RecordingContracts {
    synced_with: Mutex<Option<PathBuf>>,
}

impl RecordingContracts {
    fn synced_with(&self) -> Option<PathBuf> {
        self.synced_with.lock().expect("sync record").clone()
    }
}

impl ReleaseContracts for RecordingContracts {
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
        Ok(version())
    }
    fn sync_guide(&self, _version: &ReleaseVersion, binary: &Path) -> Result<(), ReleaseError> {
        *self.synced_with.lock().expect("sync record") = Some(binary.to_path_buf());
        // The real guide engine refuses a path that is not a file, which is
        // exactly how #723 surfaced.
        if !binary.is_file() {
            return Err(ReleaseError::invalid(format!(
                "guide engine binary does not exist: {}",
                binary.display()
            )));
        }
        Ok(())
    }
    fn stamp_mcpb(&self, _archive: &Path) -> Result<(), ReleaseError> {
        Ok(())
    }
    fn candidate_inputs(&self) -> Result<CandidateInputDigest, ReleaseError> {
        CandidateInputDigest::parse("0".repeat(64))
    }
    fn verify_candidate(
        &self,
        _version: &ReleaseVersion,
        _directory: &Path,
        _input: &CandidateInputDigest,
        _run_id: &WorkflowRunId,
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

#[test]
fn guide_sync_receives_the_engine_the_build_produced() {
    let cache = tempfile::tempdir().expect("shared cargo cache");
    let workspace = ConfiguredTargetWorkspace::building_into(cache.path());
    let contracts = RecordingContracts::default();

    let outcome = PrepareReleaseWorkflow::new(&contracts, &workspace).execute(&version());

    assert!(
        outcome.is_ok(),
        "prepare must not fail after a successful build: {:?}",
        outcome.err()
    );
    assert_eq!(
        contracts.synced_with().as_deref(),
        Some(workspace.engine()),
        "guide sync has to inspect the engine the build produced, not a guessed target path"
    );
}

/// A standalone checkout that builds an engine of its own, so the real
/// workspace adapter can be driven end to end without touching this one.
fn engine_fixture(root: &Path, target_directory: Option<&str>) {
    fs::create_dir_all(root.join("src")).expect("fixture source");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\n[package]\nname = \"kmp-mcp\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("fixture manifest");
    // `--locked` is part of the real build command; the fixture has no
    // dependencies, so its lockfile is this short.
    fs::write(
        root.join("Cargo.lock"),
        "version = 3\n\n[[package]]\nname = \"kmp-mcp\"\nversion = \"0.0.0\"\n",
    )
    .expect("fixture lockfile");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("fixture entry point");
    if let Some(directory) = target_directory {
        fs::create_dir_all(root.join(".cargo")).expect("fixture cargo directory");
        fs::write(
            root.join(".cargo/config.toml"),
            format!("[build]\ntarget-dir = \"{directory}\"\n"),
        )
        .expect("fixture cargo config");
    }
}

#[test]
fn the_real_build_answers_with_an_engine_that_exists() {
    let root = tempfile::tempdir().expect("checkout");
    engine_fixture(root.path(), None);
    let workspace =
        SystemReleaseWorkspace::new(RepositoryRoot::from_path(root.path()).expect("root"));

    let engine = workspace.build_engine().expect("build");

    assert!(
        engine.is_file(),
        "the build has to answer with the engine it produced, not a path to look for: {}",
        engine.display()
    );
    assert_eq!(
        engine.file_name().and_then(|name| name.to_str()),
        Some("kmp-mcp")
    );
}

#[test]
fn a_configured_target_directory_is_honored() {
    let root = tempfile::tempdir().expect("checkout");
    engine_fixture(root.path(), Some("build-out"));
    let workspace =
        SystemReleaseWorkspace::new(RepositoryRoot::from_path(root.path()).expect("root"));

    let engine = workspace.build_engine().expect("build");

    assert!(
        engine.is_file(),
        "a configured target directory still has to yield a real engine: {}",
        engine.display()
    );
    assert!(
        !root.path().join("target/debug/kmp-mcp").exists(),
        "the fixture must not build into the default directory, or it proves nothing"
    );
}

#[test]
fn two_checkouts_sharing_one_cache_each_resolve_their_engine() {
    let cache = tempfile::tempdir().expect("shared cargo cache");
    let shared = cache.path().to_string_lossy().to_string();
    let first = tempfile::tempdir().expect("first checkout");
    let second = tempfile::tempdir().expect("second checkout");
    engine_fixture(first.path(), Some(&shared));
    engine_fixture(second.path(), Some(&shared));

    let from_first =
        SystemReleaseWorkspace::new(RepositoryRoot::from_path(first.path()).expect("first root"))
            .build_engine()
            .expect("first build");
    // The second build finds the cache warm, which is also what a resumed
    // release run looks like.
    let from_second =
        SystemReleaseWorkspace::new(RepositoryRoot::from_path(second.path()).expect("second root"))
            .build_engine()
            .expect("second build");

    assert!(from_first.is_file() && from_second.is_file());
    assert_eq!(
        from_first, from_second,
        "one shared cache holds one engine; both checkouts must be told about that one"
    );
    assert!(
        from_first.starts_with(cache.path()),
        "the engine lives in the configured cache: {}",
        from_first.display()
    );
}

#[test]
fn a_build_that_fails_is_reported_instead_of_a_missing_binary() {
    let root = tempfile::tempdir().expect("checkout");
    engine_fixture(root.path(), None);
    fs::write(root.path().join("src/main.rs"), "fn main() { not_rust }\n").expect("broken source");
    let workspace =
        SystemReleaseWorkspace::new(RepositoryRoot::from_path(root.path()).expect("root"));

    let outcome = workspace.build_engine();

    let error = outcome.expect_err("a failed build cannot answer with an engine");
    assert!(
        format!("{error:?}").contains("cargo build"),
        "the operator has to read that the build failed: {error:?}"
    );
}
