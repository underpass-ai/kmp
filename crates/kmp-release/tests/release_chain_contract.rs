//! The two orchestrating verbs of #446: what they chain, what they refuse,
//! and what they never do twice.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use kmp_release::adapters::system_file_system::SystemFileSystem;
use kmp_release::application::use_cases::run_release_preparation::RunReleasePreparation;
use kmp_release::application::use_cases::run_release_publication::RunReleasePublication;
use kmp_release::domain::asset_wait::AssetWait;
use kmp_release::domain::branch_name::BranchName;
use kmp_release::domain::candidate_asset_set::CandidateAssetSet;
use kmp_release::domain::candidate_input_digest::CandidateInputDigest;
use kmp_release::domain::pull_request_url::PullRequestUrl;
use kmp_release::domain::release_error::ReleaseError;
use kmp_release::domain::release_version::ReleaseVersion;
use kmp_release::domain::repository_root::RepositoryRoot;
use kmp_release::domain::source_commit::SourceCommit;
use kmp_release::domain::workflow_run_id::WorkflowRunId;
use kmp_release::ports::candidate_automation::CandidateAutomation;
use kmp_release::ports::release_contracts::ReleaseContracts;
use kmp_release::ports::release_workspace::ReleaseWorkspace;

const HEAD: &str = "b6a469f0b4591b93f690a824ca239e8b68b8ba24";

fn version() -> ReleaseVersion {
    ReleaseVersion::parse("0.6.1".to_string()).expect("version")
}

/// A workspace that records what the chain asked it to do.
#[derive(Default)]
struct RecordingWorkspace {
    branch: String,
    tagged: bool,
    dirty: bool,
    log: Mutex<Vec<String>>,
}

impl RecordingWorkspace {
    fn on(branch: &str) -> Self {
        Self {
            branch: branch.to_string(),
            ..Default::default()
        }
    }

    fn already_tagged(mut self) -> Self {
        self.tagged = true;
        self
    }

    fn dirty(mut self) -> Self {
        self.dirty = true;
        self
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().expect("log").clone()
    }

    fn record(&self, entry: impl Into<String>) {
        self.log.lock().expect("log").push(entry.into());
    }
}

impl ReleaseWorkspace for RecordingWorkspace {
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
        if self.dirty {
            return Err(ReleaseError::invalid("working tree is dirty"));
        }
        Ok(())
    }
    fn current_branch(&self) -> Result<BranchName, ReleaseError> {
        BranchName::parse(self.branch.clone())
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
        Ok(self.tagged)
    }
    fn create_and_push_tag(
        &self,
        version: &ReleaseVersion,
        _run_id: &WorkflowRunId,
        _input: &CandidateInputDigest,
    ) -> Result<(), ReleaseError> {
        self.record(format!("tag {}", version.tag()));
        Ok(())
    }
    fn commit_tracked(&self, message: &str) -> Result<bool, ReleaseError> {
        self.record(format!("commit {message}"));
        Ok(true)
    }
    fn push_current_branch(&self) -> Result<(), ReleaseError> {
        self.record("push");
        Ok(())
    }
    fn advance_branch(
        &self,
        branch: &BranchName,
        commit: &SourceCommit,
    ) -> Result<(), ReleaseError> {
        self.record(format!("advance {branch} to {commit}"));
        Ok(())
    }
}

/// A release that publishes its assets one poll at a time.
struct UploadingRelease {
    published: Mutex<Vec<Vec<String>>>,
}

impl UploadingRelease {
    fn revealing(stages: Vec<Vec<String>>) -> Self {
        Self {
            published: Mutex::new(stages),
        }
    }
}

impl CandidateAutomation for UploadingRelease {
    fn dispatch(
        &self,
        _branch: &BranchName,
        _commit: &SourceCommit,
    ) -> Result<WorkflowRunId, ReleaseError> {
        unreachable!("publication never dispatches a build")
    }
    fn watch(&self, _run_id: &WorkflowRunId) -> Result<(), ReleaseError> {
        unreachable!("publication never watches a build")
    }
    fn download(
        &self,
        _run_id: &WorkflowRunId,
        _version: &ReleaseVersion,
        _destination: &Path,
    ) -> Result<(), ReleaseError> {
        unreachable!("publication downloads nothing once the tag exists")
    }
    fn successful_runs(&self) -> Result<Vec<WorkflowRunId>, ReleaseError> {
        Ok(Vec::new())
    }
    fn pull_request_for(
        &self,
        _branch: &BranchName,
        _title: &str,
        _body: &str,
    ) -> Result<PullRequestUrl, ReleaseError> {
        unreachable!("publication opens no pull request")
    }
    fn arm_auto_merge(&self, _pull_request: &PullRequestUrl) -> Result<(), ReleaseError> {
        unreachable!("publication arms nothing")
    }
    fn published_assets(&self, _version: &ReleaseVersion) -> Result<Vec<String>, ReleaseError> {
        let mut stages = self.published.lock().expect("stages");
        if stages.len() > 1 {
            Ok(stages.remove(0))
        } else {
            Ok(stages.first().cloned().unwrap_or_default())
        }
    }
}

struct UnusedContracts;

impl ReleaseContracts for UnusedContracts {
    fn sync_readmes(&self) -> Result<(), ReleaseError> {
        unreachable!()
    }
    fn prepare_changelog(&self, _version: &ReleaseVersion) -> Result<(), ReleaseError> {
        unreachable!()
    }
    fn check_changelog(&self, _version: &ReleaseVersion) -> Result<(), ReleaseError> {
        unreachable!()
    }
    fn prepare_version(&self, _version: &ReleaseVersion) -> Result<(), ReleaseError> {
        unreachable!()
    }
    fn workspace_version(&self) -> Result<ReleaseVersion, ReleaseError> {
        unreachable!()
    }
    fn sync_guide(&self, _version: &ReleaseVersion, _binary: &Path) -> Result<(), ReleaseError> {
        unreachable!()
    }
    fn stamp_mcpb(&self, _archive: &Path) -> Result<(), ReleaseError> {
        unreachable!()
    }
    fn candidate_inputs(&self) -> Result<CandidateInputDigest, ReleaseError> {
        unreachable!()
    }
    fn verify_candidate(
        &self,
        _version: &ReleaseVersion,
        _directory: &Path,
        _input: &CandidateInputDigest,
        _run_id: &WorkflowRunId,
    ) -> Result<(), ReleaseError> {
        unreachable!()
    }
    fn verify_marketplace(
        &self,
        _version: &ReleaseVersion,
        _expected_commit: &SourceCommit,
    ) -> Result<(), ReleaseError> {
        unreachable!()
    }
}

fn brisk() -> AssetWait {
    AssetWait::new(Duration::from_secs(2), Duration::from_millis(1))
}

#[test]
fn publication_advances_the_catalog_only_after_every_asset_is_public() {
    let all = CandidateAssetSet::for_version(&version()).all().to_vec();
    let workspace = RecordingWorkspace::on("main").already_tagged();
    // Two polls: a release still uploading, then the complete one.
    let releases = UploadingRelease::revealing(vec![all[..4].to_vec(), all.clone()]);
    let root = RepositoryRoot::discover().expect("root");

    let message = RunReleasePublication::new(
        &SystemFileSystem,
        &UnusedContracts,
        &workspace,
        &releases,
        &root,
    )
    .waiting(brisk())
    .execute(&version())
    .expect("publication completes");

    assert_eq!(
        workspace.log(),
        [format!("advance marketplace to {HEAD}")],
        "the branch moves once, and only after the assets are public"
    );
    assert!(
        message.contains("marketplace branch now serves"),
        "{message}"
    );
}

#[test]
fn a_release_that_never_finishes_uploading_leaves_the_catalog_where_it_is() {
    // Publishing the catalog first would make Claude Code clone a tag whose
    // assets nobody can download.
    let all = CandidateAssetSet::for_version(&version()).all().to_vec();
    let workspace = RecordingWorkspace::on("main").already_tagged();
    let releases = UploadingRelease::revealing(vec![all[..21].to_vec()]);
    let root = RepositoryRoot::discover().expect("root");

    let error = RunReleasePublication::new(
        &SystemFileSystem,
        &UnusedContracts,
        &workspace,
        &releases,
        &root,
    )
    .waiting(brisk())
    .execute(&version())
    .expect_err("an incomplete release is not publishable");

    assert!(error.to_string().contains("21 of 22 assets"), "{error}");
    assert!(
        workspace.log().is_empty(),
        "nothing moved: {:?}",
        workspace.log()
    );
}

#[test]
fn publication_does_not_tag_a_version_that_is_already_tagged() {
    // A publication that failed at the marketplace advance has to be able to
    // finish without `release` refusing on the tag it created itself.
    let all = CandidateAssetSet::for_version(&version()).all().to_vec();
    let workspace = RecordingWorkspace::on("main").already_tagged();
    let releases = UploadingRelease::revealing(vec![all]);
    let root = RepositoryRoot::discover().expect("root");

    RunReleasePublication::new(
        &SystemFileSystem,
        &UnusedContracts,
        &workspace,
        &releases,
        &root,
    )
    .waiting(brisk())
    .execute(&version())
    .expect("publication completes");

    assert!(
        !workspace
            .log()
            .iter()
            .any(|entry| entry.starts_with("tag ")),
        "{:?}",
        workspace.log()
    );
}

#[test]
fn preparation_refuses_to_run_on_main() {
    // The gate has to stand between a version change and main.
    let workspace = RecordingWorkspace::on("main");
    let releases = UploadingRelease::revealing(vec![Vec::new()]);
    let root = RepositoryRoot::discover().expect("root");

    let error = RunReleasePreparation::new(
        &SystemFileSystem,
        &UnusedContracts,
        &workspace,
        &releases,
        &root,
    )
    .execute(&version(), None)
    .expect_err("prepare must not run on main");

    assert!(error.to_string().contains("not main"), "{error}");
    assert!(workspace.log().is_empty());
}

#[test]
fn preparation_refuses_a_dirty_tree_before_it_writes_anything() {
    // The chain commits what its own steps wrote. An edit already sitting in
    // the tree would ride into `chore: prepare vX.Y.Z` under a message that
    // does not describe it.
    let workspace = RecordingWorkspace::on("chore/prepare-0.6.1").dirty();
    let releases = UploadingRelease::revealing(vec![Vec::new()]);
    let root = RepositoryRoot::discover().expect("root");

    let error = RunReleasePreparation::new(
        &SystemFileSystem,
        &UnusedContracts,
        &workspace,
        &releases,
        &root,
    )
    .execute(&version(), None)
    .expect_err("prepare must not start on a dirty tree");

    assert!(error.to_string().contains("dirty"), "{error}");
    assert!(workspace.log().is_empty(), "nothing was written");
}

#[test]
fn publication_refuses_anywhere_but_main() {
    let workspace = RecordingWorkspace::on("chore/prepare-0.6.1").already_tagged();
    let releases = UploadingRelease::revealing(vec![Vec::new()]);
    let root = RepositoryRoot::discover().expect("root");

    let error = RunReleasePublication::new(
        &SystemFileSystem,
        &UnusedContracts,
        &workspace,
        &releases,
        &root,
    )
    .waiting(brisk())
    .execute(&version())
    .expect_err("publish must run on main");

    assert!(
        error.to_string().contains("not chore/prepare-0.6.1"),
        "{error}"
    );
    assert!(workspace.log().is_empty());
}
