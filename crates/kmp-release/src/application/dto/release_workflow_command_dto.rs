use crate::domain::release_version::ReleaseVersion;
use crate::domain::workflow_run_id::WorkflowRunId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReleaseWorkflowCommandDto {
    Preflight {
        version: ReleaseVersion,
    },
    Version {
        version: ReleaseVersion,
    },
    Candidate {
        version: ReleaseVersion,
        run_id: Option<WorkflowRunId>,
    },
    Release {
        version: ReleaseVersion,
    },
    /// Notes are written; chain every mechanical step up to an armed pull
    /// request.
    Prepare {
        version: ReleaseVersion,
        run_id: Option<WorkflowRunId>,
    },
    /// The pull request merged; tag, wait for the public assets, and advance
    /// the catalog.
    Publish {
        version: ReleaseVersion,
    },
}
