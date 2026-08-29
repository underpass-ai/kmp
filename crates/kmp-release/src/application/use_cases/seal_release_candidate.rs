use crate::domain::candidate_workspace::CandidateWorkspace;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::workflow_run_id::WorkflowRunId;
use crate::ports::candidate_automation::CandidateAutomation;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_contracts::ReleaseContracts;
use crate::ports::release_workspace::ReleaseWorkspace;

pub struct SealReleaseCandidate<'a, F, C, W, A> {
    file_system: &'a F,
    contracts: &'a C,
    workspace: &'a W,
    candidates: &'a A,
    root: &'a RepositoryRoot,
}

impl<'a, F, C, W, A> SealReleaseCandidate<'a, F, C, W, A>
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

    pub fn execute(
        &self,
        version: &ReleaseVersion,
        supplied_run: Option<WorkflowRunId>,
    ) -> Result<String, ReleaseError> {
        self.require_contracts(version)?;
        self.workspace.require_clean()?;
        let branch = self.workspace.current_branch()?;
        let head = self.workspace.head_commit()?;
        let run_id = match supplied_run {
            Some(run_id) => run_id,
            None => {
                if self.workspace.upstream_commit()?.as_ref() != Some(&head) {
                    return Err(ReleaseError::invalid(format!(
                        "push {branch} before building its candidate"
                    )));
                }
                self.candidates.dispatch(&branch, &head)?
            }
        };
        println!("candidate run: {run_id}");
        self.candidates.watch(&run_id)?;

        let scratch = CandidateWorkspace::for_stamp(self.root);
        self.file_system.remove_dir_all_if_present(scratch.root())?;
        let result = self.verify_and_stamp(version, &run_id, &scratch);
        let cleanup = self.file_system.remove_dir_all_if_present(scratch.root());
        result?;
        cleanup?;
        Ok(format!(
            "candidate {run_id} verified and server.json stamped; review, commit and push server.json"
        ))
    }

    fn require_contracts(&self, version: &ReleaseVersion) -> Result<(), ReleaseError> {
        let actual = self.contracts.workspace_version()?;
        if actual != *version {
            return Err(ReleaseError::invalid(format!(
                "workspace version {actual} does not match target {version}"
            )));
        }
        self.contracts.check_changelog(version)?;
        Ok(())
    }

    fn verify_and_stamp(
        &self,
        version: &ReleaseVersion,
        run_id: &WorkflowRunId,
        scratch: &CandidateWorkspace,
    ) -> Result<(), ReleaseError> {
        let candidate = scratch.candidate();
        self.file_system.create_dir_all(&candidate)?;
        self.candidates.download(run_id, version, &candidate)?;
        self.contracts.stamp_mcpb(&scratch.mcpb(version))?;
        let input = self.contracts.candidate_inputs()?;
        self.contracts
            .verify_candidate(version, &candidate, &input, run_id)?;
        self.workspace.verify_registry()
    }
}
