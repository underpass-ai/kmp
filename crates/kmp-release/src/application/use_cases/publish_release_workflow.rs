use std::path::{Path, PathBuf};

use crate::application::dto::candidate_manifest_dto::CandidateManifestDto;
use crate::application::use_cases::check_release_readiness::CheckReleaseReadiness;
use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::candidate_input_selector::CandidateInputSelector;
use crate::domain::candidate_workspace::CandidateWorkspace;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;
use crate::domain::workflow_run_id::WorkflowRunId;
use crate::ports::candidate_automation::CandidateAutomation;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_contracts::ReleaseContracts;
use crate::ports::release_file_system::ReleaseFileSystem;
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
    F: CandidateFileSystem + ReleaseFileSystem,
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
        println!(
            "{}",
            CheckReleaseReadiness::new(self.file_system, self.contracts, self.workspace, self.root)
                .execute(version)
                .into_result()?
        );
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

    fn find_candidate(
        &self,
        version: &ReleaseVersion,
        input: &CandidateInputDigest,
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
        input: &CandidateInputDigest,
        scratch: &CandidateWorkspace,
    ) -> Result<WorkflowRunId, ReleaseError> {
        let mut carried = 0;
        let mut superseded = None;
        for run_id in self.candidates.successful_runs()? {
            let candidate = scratch.candidate_for(&run_id);
            self.file_system.create_dir_all(&candidate)?;
            // A run that built another version carries no artifact for this one.
            if self
                .candidates
                .download(&run_id, version, &candidate)
                .is_err()
            {
                continue;
            }
            carried += 1;
            if self
                .contracts
                .verify_candidate(version, &candidate, input, &run_id)
                .is_ok()
            {
                return Ok(run_id);
            }
            if superseded.is_none() {
                superseded = self.superseded_by_tree(&candidate, version, input, &run_id);
            }
        }
        Err(self.no_candidate_error(version, input, carried, superseded))
    }

    /// A candidate that names this version but a different input digest was
    /// built from a tree that has since moved. That is the common case, and the
    /// operator needs the file that moved, not the digest that changed.
    fn superseded_by_tree(
        &self,
        directory: &Path,
        version: &ReleaseVersion,
        input: &CandidateInputDigest,
        run_id: &WorkflowRunId,
    ) -> Option<(WorkflowRunId, SourceCommit, String)> {
        let manifest = self.read_manifest(directory)?;
        if manifest.version != version.as_str() || manifest.input_sha256 == input.as_str() {
            return None;
        }
        let commit = SourceCommit::parse(manifest.source_sha).ok()?;
        Some((run_id.clone(), commit, manifest.input_sha256))
    }

    fn read_manifest(&self, directory: &Path) -> Option<CandidateManifestDto> {
        let body = self
            .file_system
            .read_bytes(&directory.join("candidate.json"))
            .ok()?;
        serde_json::from_slice(&body).ok()
    }

    fn no_candidate_error(
        &self,
        version: &ReleaseVersion,
        input: &CandidateInputDigest,
        carried: usize,
        superseded: Option<(WorkflowRunId, SourceCommit, String)>,
    ) -> ReleaseError {
        let Some((run_id, commit, built_from)) = superseded else {
            return ReleaseError::invalid(match carried {
                0 => format!(
                    "no successful release candidate carries {version} assets; build one with `scripts/release.sh candidate {version}`"
                ),
                carried => format!(
                    "none of the {carried} candidates carrying {version} assets matches inputs {input}; rebuild one with `scripts/release.sh candidate {version}`"
                ),
            });
        };
        let moved = self.moved_inputs(&commit);
        let detail = if moved.is_empty() {
            "a release input changed since it was built".to_string()
        } else {
            format!(
                "these release inputs changed since it was built:\n  - {}",
                moved
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n  - ")
            )
        };
        ReleaseError::invalid(format!(
            "candidate run {run_id} built {version} from {commit} with inputs {built_from}, but this tree hashes to {input}; {detail}\nrebuild it with `scripts/release.sh candidate {version}`"
        ))
    }

    fn moved_inputs(&self, commit: &SourceCommit) -> Vec<PathBuf> {
        self.workspace
            .changed_files_since(commit)
            .map(|paths| CandidateInputSelector::new().select(paths))
            .unwrap_or_default()
    }
}
