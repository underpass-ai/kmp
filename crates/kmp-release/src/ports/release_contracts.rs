use std::path::Path;

use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::source_commit::SourceCommit;
use crate::domain::workflow_run_id::WorkflowRunId;

pub trait ReleaseContracts {
    fn sync_readmes(&self) -> Result<(), ReleaseError>;
    fn prepare_changelog(&self, version: &ReleaseVersion) -> Result<(), ReleaseError>;
    fn check_changelog(&self, version: &ReleaseVersion) -> Result<(), ReleaseError>;
    fn prepare_version(&self, version: &ReleaseVersion) -> Result<(), ReleaseError>;
    fn workspace_version(&self) -> Result<ReleaseVersion, ReleaseError>;
    fn sync_guide(&self, version: &ReleaseVersion, binary: &Path) -> Result<(), ReleaseError>;
    fn stamp_mcpb(&self, archive: &Path) -> Result<(), ReleaseError>;
    fn candidate_inputs(&self) -> Result<CandidateInputDigest, ReleaseError>;
    fn verify_candidate(
        &self,
        version: &ReleaseVersion,
        directory: &Path,
        input: &CandidateInputDigest,
        run_id: &WorkflowRunId,
    ) -> Result<(), ReleaseError>;
    fn verify_marketplace(
        &self,
        version: &ReleaseVersion,
        expected_commit: &SourceCommit,
    ) -> Result<(), ReleaseError>;
}
