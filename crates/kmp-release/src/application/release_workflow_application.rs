use crate::application::dto::release_workflow_command_dto::ReleaseWorkflowCommandDto;
use crate::application::use_cases::prepare_release_workflow::PrepareReleaseWorkflow;
use crate::application::use_cases::publish_release_workflow::PublishReleaseWorkflow;
use crate::application::use_cases::seal_release_candidate::SealReleaseCandidate;
use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_automation::CandidateAutomation;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_contracts::ReleaseContracts;
use crate::ports::release_workspace::ReleaseWorkspace;

pub struct ReleaseWorkflowApplication<'a, F, C, W, A> {
    file_system: &'a F,
    contracts: &'a C,
    workspace: &'a W,
    candidates: &'a A,
    root: &'a RepositoryRoot,
}

impl<'a, F, C, W, A> ReleaseWorkflowApplication<'a, F, C, W, A>
where
    F: CandidateFileSystem,
    C: ReleaseContracts,
    W: ReleaseWorkspace,
    A: CandidateAutomation,
{
    pub fn new(
        file_system: &'a F,
        contracts: &'a C,
        workspace: &'a W,
        candidates: &'a A,
        root: &'a RepositoryRoot,
    ) -> Self {
        Self {
            file_system,
            contracts,
            workspace,
            candidates,
            root,
        }
    }

    pub fn execute(&self, command: ReleaseWorkflowCommandDto) -> Result<String, ReleaseError> {
        match command {
            ReleaseWorkflowCommandDto::Version { version } => {
                PrepareReleaseWorkflow::new(self.contracts, self.workspace, self.root)
                    .execute(&version)
            }
            ReleaseWorkflowCommandDto::Candidate { version, run_id } => SealReleaseCandidate::new(
                self.file_system,
                self.contracts,
                self.workspace,
                self.candidates,
                self.root,
            )
            .execute(&version, run_id),
            ReleaseWorkflowCommandDto::Release { version } => PublishReleaseWorkflow::new(
                self.file_system,
                self.contracts,
                self.workspace,
                self.candidates,
                self.root,
            )
            .execute(&version),
        }
    }
}
