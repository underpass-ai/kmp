use crate::domain::candidate_workspace::CandidateWorkspace;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::workflow_run_id::WorkflowRunId;
use crate::ports::candidate_automation::CandidateAutomation;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_contracts::ReleaseContracts;
use crate::ports::release_workspace::ReleaseWorkspace;

pub struct PublishReleaseWorkflow<'a, F, C, W, A> {
    file_system: &'a F,
    contracts: &'a C,
    workspace: &'a W,
    candidates: &'a A,
    root: &'a RepositoryRoot,
}

impl<'a, F, C, W, A> PublishReleaseWorkflow<'a, F, C, W, A>
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

    pub fn execute(&self, version: &ReleaseVersion) -> Result<String, ReleaseError> {
        self.require_contracts(version)?;
        self.workspace.require_clean()?;
        let branch = self.workspace.current_branch()?;
        if !branch.is_main() {
            return Err(ReleaseError::invalid(format!(
                "release must run on main, not {branch}"
            )));
        }
        if self.workspace.tag_exists(version)? {
            return Err(ReleaseError::invalid(format!(
                "tag {} already exists",
                version.tag()
            )));
        }
        self.workspace.verify_registry()?;
        let head = self.workspace.head_commit()?;
        self.contracts.verify_marketplace(version, &head)?;
        let input = self.contracts.candidate_inputs()?;
        let candidate_run = self.find_candidate(version, &input)?;
        self.workspace
            .create_and_push_tag(version, &candidate_run, &input)?;
        Ok(format!(
            "tagged {} and pushed; candidate run {candidate_run} approved. Advance the marketplace branch only after every release asset is public.",
            version.tag()
        ))
    }

    fn require_contracts(&self, version: &ReleaseVersion) -> Result<(), ReleaseError> {
        self.contracts.check_changelog(version)?;
        let actual = self.contracts.workspace_version()?;
        if actual == *version {
            Ok(())
        } else {
            Err(ReleaseError::invalid(format!(
                "workspace version {actual} does not match target {version}"
            )))
        }
    }

    fn find_candidate(
        &self,
        version: &ReleaseVersion,
        input: &crate::domain::candidate_input_digest::CandidateInputDigest,
    ) -> Result<WorkflowRunId, ReleaseError> {
        let scratch = CandidateWorkspace::for_release(self.root);
        self.file_system.remove_dir_all_if_present(scratch.root())?;
        self.file_system.create_dir_all(scratch.root())?;
        let result = self.find_candidate_in(version, input, &scratch);
        let cleanup = self.file_system.remove_dir_all_if_present(scratch.root());
        let run_id = result?;
        cleanup?;
        Ok(run_id)
    }

    fn find_candidate_in(
        &self,
        version: &ReleaseVersion,
        input: &crate::domain::candidate_input_digest::CandidateInputDigest,
        scratch: &CandidateWorkspace,
    ) -> Result<WorkflowRunId, ReleaseError> {
        for run_id in self.candidates.successful_runs()? {
            let candidate = scratch.candidate_for(&run_id);
            self.file_system.create_dir_all(&candidate)?;
            if self
                .candidates
                .download(&run_id, version, &candidate)
                .and_then(|()| {
                    self.contracts
                        .verify_candidate(version, &candidate, input, &run_id)
                })
                .is_ok()
            {
                return Ok(run_id);
            }
        }
        Err(ReleaseError::invalid(format!(
            "no successful release candidate matches {version} and inputs {input}"
        )))
    }
}
