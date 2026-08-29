use std::path::Path;

use crate::domain::branch_name::BranchName;
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
}
