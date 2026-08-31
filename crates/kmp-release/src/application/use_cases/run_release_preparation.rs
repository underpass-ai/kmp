use crate::application::use_cases::collect_version_sources::CollectVersionSources;
use crate::application::use_cases::prepare_release_workflow::PrepareReleaseWorkflow;
use crate::application::use_cases::seal_release_candidate::SealReleaseCandidate;
use crate::domain::pull_request_url::PullRequestUrl;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::workflow_run_id::WorkflowRunId;
use crate::ports::candidate_automation::CandidateAutomation;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_contracts::ReleaseContracts;
use crate::ports::release_file_system::ReleaseFileSystem;
use crate::ports::release_workspace::ReleaseWorkspace;

/// Use case: everything between reviewed release notes and a pull request the
/// gate can merge.
///
/// Releasing 0.6.1 by the runbook took one editorial step — writing the
/// `[Unreleased]` notes — and then a chain of mechanical ones typed by hand:
/// version, commit, push, candidate, commit the seal, push, open the pull
/// request, arm auto-merge. None of the typed steps adds judgment, and the
/// seal "review" is a one-line sha diff the candidate verification already
/// proved (#446).
///
/// Three decision points stay deliberate and none of them is here: writing the
/// notes comes before, merging is the gate's word, and `publish` is a separate
/// sentence the operator has to say.
pub struct RunReleasePreparation<'a, F, C, W, A> {
    file_system: &'a F,
    contracts: &'a C,
    workspace: &'a W,
    candidates: &'a A,
    root: &'a RepositoryRoot,
}

impl<'a, F, C, W, A> RunReleasePreparation<'a, F, C, W, A>
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

    pub fn execute(
        &self,
        version: &ReleaseVersion,
        supplied_run: Option<WorkflowRunId>,
    ) -> Result<String, ReleaseError> {
        let branch = self.workspace.current_branch()?;
        if branch.is_main() {
            return Err(ReleaseError::invalid(
                "prepare runs on a version branch, not main: the gate has to stand between this \
                 change and main",
            ));
        }
        // Before anything is written. This chain commits what its own steps
        // wrote, so an edit already sitting in the tree would ride into the
        // release commit under a message that does not describe it.
        self.workspace.require_clean()?;

        if self.version_already_bumped(version)? {
            println!("version: every source already reads {version}; not bumping again");
        } else {
            println!(
                "{}",
                PrepareReleaseWorkflow::new(self.contracts, self.workspace, self.root)
                    .execute(version)?
            );
        }
        self.commit_and_push(&format!("chore: prepare {}", version.tag()))?;

        println!(
            "{}",
            SealReleaseCandidate::new(
                self.file_system,
                self.contracts,
                self.workspace,
                self.candidates,
                self.root,
            )
            .execute(version, supplied_run)?
        );
        self.commit_and_push(&format!("chore(release): seal {version} MCPB"))?;

        let pull_request = self.armed_pull_request(version, &branch)?;
        Ok(format!(
            "{pull_request} is open with auto-merge armed. Merge when the gate speaks, then run \
             `scripts/release.sh publish {version}` on main."
        ))
    }

    /// A rerun after a failed step must not bump a version that is already
    /// bumped: the second bump would refuse on an empty `[Unreleased]` and
    /// leave the operator reading an error about notes they already wrote.
    fn version_already_bumped(&self, version: &ReleaseVersion) -> Result<bool, ReleaseError> {
        Ok(CollectVersionSources::new(self.file_system)
            .execute(self.root, version)?
            .iter()
            .all(|source| source.agrees()))
    }

    fn commit_and_push(&self, message: &str) -> Result<(), ReleaseError> {
        if self.workspace.commit_tracked(message)? {
            println!("committed: {message}");
        } else {
            println!("nothing to commit for `{message}`; it already landed");
        }
        self.workspace.push_current_branch()
    }

    fn armed_pull_request(
        &self,
        version: &ReleaseVersion,
        branch: &crate::domain::branch_name::BranchName,
    ) -> Result<PullRequestUrl, ReleaseError> {
        let pull_request = self.candidates.pull_request_for(
            branch,
            &format!("chore: prepare {}", version.tag()),
            &format!(
                "Prepared by `scripts/release.sh prepare {version}`: version sources, guides and \
                 public READMEs bumped, and `server.json` sealed with the digest of the verified \
                 release candidate.\n\nMerging is the full gate's decision; this pull request \
                 carries no judgment of its own."
            ),
        )?;
        self.candidates.arm_auto_merge(&pull_request)?;
        Ok(pull_request)
    }
}
