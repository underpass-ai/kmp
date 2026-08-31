use std::path::PathBuf;

use crate::domain::branch_name::BranchName;
use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::source_commit::SourceCommit;
use crate::domain::workflow_run_id::WorkflowRunId;

pub trait ReleaseWorkspace {
    fn refresh_lockfile(&self) -> Result<(), ReleaseError>;
    fn build_engine(&self) -> Result<(), ReleaseError>;
    fn show_version_diff(&self) -> Result<(), ReleaseError>;
    fn require_clean(&self) -> Result<(), ReleaseError>;
    fn current_branch(&self) -> Result<BranchName, ReleaseError>;
    fn head_commit(&self) -> Result<SourceCommit, ReleaseError>;
    fn upstream_commit(&self) -> Result<Option<SourceCommit>, ReleaseError>;
    fn verify_registry(&self) -> Result<(), ReleaseError>;
    fn verify_vendored_contract(&self) -> Result<(), ReleaseError>;
    fn verify_publish_chain(&self) -> Result<(), ReleaseError>;
    /// Tracked paths that differ between `commit` and the working tree, used to
    /// name what moved under a candidate that no longer matches.
    fn changed_files_since(&self, commit: &SourceCommit) -> Result<Vec<PathBuf>, ReleaseError>;
    fn tag_exists(&self, version: &ReleaseVersion) -> Result<bool, ReleaseError>;
    /// Stage every tracked change and commit them.
    ///
    /// `false` when there was nothing to commit, which is what a rerun of a
    /// step that already landed looks like — the chain has to tell those two
    /// apart to be resumable (#446).
    fn commit_tracked(&self, message: &str) -> Result<bool, ReleaseError>;
    /// Push the current branch, giving it an upstream when it has none.
    fn push_current_branch(&self) -> Result<(), ReleaseError>;
    /// Move a branch to exactly this commit, without checking it out.
    fn advance_branch(
        &self,
        branch: &BranchName,
        commit: &SourceCommit,
    ) -> Result<(), ReleaseError>;
    fn create_and_push_tag(
        &self,
        version: &ReleaseVersion,
        run_id: &WorkflowRunId,
        input: &CandidateInputDigest,
    ) -> Result<(), ReleaseError>;
}
