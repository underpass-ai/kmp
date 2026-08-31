use std::path::Path;

use crate::domain::branch_name::BranchName;
use crate::domain::pull_request_url::PullRequestUrl;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::source_commit::SourceCommit;
use crate::domain::workflow_run_id::WorkflowRunId;

pub trait CandidateAutomation {
    fn dispatch(
        &self,
        branch: &BranchName,
        commit: &SourceCommit,
    ) -> Result<WorkflowRunId, ReleaseError>;
    fn watch(&self, run_id: &WorkflowRunId) -> Result<(), ReleaseError>;
    fn download(
        &self,
        run_id: &WorkflowRunId,
        version: &ReleaseVersion,
        destination: &Path,
    ) -> Result<(), ReleaseError>;
    fn successful_runs(&self) -> Result<Vec<WorkflowRunId>, ReleaseError>;
    /// The open pull request for this branch, opening one when none is open.
    ///
    /// Idempotent on purpose: a preparation that failed after opening the pull
    /// request must find it again rather than refuse or open a second one.
    fn pull_request_for(
        &self,
        branch: &BranchName,
        title: &str,
        body: &str,
    ) -> Result<PullRequestUrl, ReleaseError>;
    /// Arm auto-merge so the full gate, not a person watching it, decides when
    /// the preparation lands.
    fn arm_auto_merge(&self, pull_request: &PullRequestUrl) -> Result<(), ReleaseError>;
    /// The public asset names of this version's release; empty while the
    /// release itself is still a draft or does not exist.
    fn published_assets(&self, version: &ReleaseVersion) -> Result<Vec<String>, ReleaseError>;
}
