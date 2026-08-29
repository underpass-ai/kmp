use crate::domain::release_version::ReleaseVersion;
use crate::domain::workflow_run_id::WorkflowRunId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReleaseWorkflowCommandDto {
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
}
