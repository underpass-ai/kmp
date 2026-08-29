use std::path::{Path, PathBuf};

use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::workflow_run_id::WorkflowRunId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateWorkspace(PathBuf);

impl CandidateWorkspace {
    pub fn for_stamp(root: &RepositoryRoot) -> Self {
        Self(root.join(format!(
            "tmp/release-candidate-stamp.{}",
            std::process::id()
        )))
    }

    pub fn for_release(root: &RepositoryRoot) -> Self {
        Self(root.join(format!(
            "tmp/release-candidate-verify.{}",
            std::process::id()
        )))
    }

    pub fn root(&self) -> &Path {
        &self.0
    }

    pub fn candidate(&self) -> PathBuf {
        self.0.join("candidate")
    }

    pub fn candidate_for(&self, run_id: &WorkflowRunId) -> PathBuf {
        self.0.join(run_id.as_str())
    }

    pub fn mcpb(&self, version: &ReleaseVersion) -> PathBuf {
        self.candidate()
            .join(format!("assets/kmp-mcp-{}.mcpb", version.tag()))
    }
}
