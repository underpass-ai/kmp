use std::thread::sleep;
use std::time::Instant;

use crate::application::use_cases::publish_release_workflow::PublishReleaseWorkflow;
use crate::domain::asset_wait::AssetWait;
use crate::domain::branch_name::BranchName;
use crate::domain::candidate_asset_set::CandidateAssetSet;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_automation::CandidateAutomation;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_contracts::ReleaseContracts;
use crate::ports::release_file_system::ReleaseFileSystem;
use crate::ports::release_workspace::ReleaseWorkspace;

/// Use case: the point of no return, in one word.
///
/// Tag the reviewed candidate, wait until the GitHub release and every one of
/// its checksummed assets are public, and only then advance the protected
/// `marketplace` branch to that exact commit. The ordering is the whole point:
/// publishing the catalog first would make Claude Code clone a tag that does
/// not exist and make the updater ask for assets nobody can download.
///
/// Invoking this stays a human decision. What was mechanical — watching the
/// assets appear, then typing the branch advance — is not (#446).
pub struct RunReleasePublication<'a, F, C, W, A> {
    file_system: &'a F,
    contracts: &'a C,
    workspace: &'a W,
    candidates: &'a A,
    root: &'a RepositoryRoot,
    wait: AssetWait,
}

impl<'a, F, C, W, A> RunReleasePublication<'a, F, C, W, A>
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
            wait: AssetWait::default(),
        }
    }

    /// Wait on a different clock. The default is the one a release runs on.
    pub fn waiting(mut self, wait: AssetWait) -> Self {
        self.wait = wait;
        self
    }

    pub fn execute(&self, version: &ReleaseVersion) -> Result<String, ReleaseError> {
        // Both checks also live inside `release`, and both have to be here
        // too: the already-tagged path below skips it, and what this verb
        // publishes is whatever main's HEAD is.
        let branch = self.workspace.current_branch()?;
        if !branch.is_main() {
            return Err(ReleaseError::invalid(format!(
                "publish runs on main, not {branch}: the commit it makes public is main's HEAD"
            )));
        }
        self.workspace.require_clean()?;

        // `release` refuses a tag that already exists, and a publication that
        // failed at the marketplace advance must still be able to finish.
        if self.workspace.tag_exists(version)? {
            println!("tag {} already exists; not tagging again", version.tag());
        } else {
            println!(
                "{}",
                PublishReleaseWorkflow::new(
                    self.file_system,
                    self.contracts,
                    self.workspace,
                    self.candidates,
                    self.root,
                )
                .execute(version)?
            );
        }

        self.await_public_assets(version)?;

        let head = self.workspace.head_commit()?;
        let marketplace = BranchName::parse("marketplace")?;
        self.workspace.advance_branch(&marketplace, &head)?;
        Ok(format!(
            "{} is public with every asset, and the marketplace branch now serves {head}.",
            version.tag()
        ))
    }

    /// Poll until the release carries exactly the twenty checksummed assets a
    /// candidate carries. Anything less is a release still uploading, and the
    /// catalog must not point at it.
    fn await_public_assets(&self, version: &ReleaseVersion) -> Result<(), ReleaseError> {
        let expected = CandidateAssetSet::for_version(version);
        let deadline = Instant::now() + self.wait.timeout();
        let mut said = false;
        loop {
            let mut published = self.candidates.published_assets(version)?;
            published.sort();
            if expected.matches(&published) {
                println!("release assets: all {} are public", published.len());
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(ReleaseError::invalid(format!(
                    "{} still carries {} of {} assets after {} minutes; the marketplace branch \
                     stays where it is until the release is complete",
                    version.tag(),
                    published.len(),
                    expected.all().len(),
                    self.wait.timeout_minutes()
                )));
            }
            if !said {
                println!(
                    "waiting for {} to publish all {} assets…",
                    version.tag(),
                    expected.all().len()
                );
                said = true;
            }
            sleep(self.wait.poll());
        }
    }
}
