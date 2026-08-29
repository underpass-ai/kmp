use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::release_contracts::ReleaseContracts;
use crate::ports::release_workspace::ReleaseWorkspace;

pub struct PrepareReleaseWorkflow<'a, C, W> {
    contracts: &'a C,
    workspace: &'a W,
    root: &'a RepositoryRoot,
}

impl<'a, C: ReleaseContracts, W: ReleaseWorkspace> PrepareReleaseWorkflow<'a, C, W> {
    pub fn new(contracts: &'a C, workspace: &'a W, root: &'a RepositoryRoot) -> Self {
        Self {
            contracts,
            workspace,
            root,
        }
    }

    pub fn execute(&self, version: &ReleaseVersion) -> Result<String, ReleaseError> {
        self.contracts.sync_readmes()?;
        self.contracts.prepare_changelog(version)?;
        self.contracts.prepare_version(version)?;
        self.workspace.refresh_lockfile()?;
        self.workspace.build_engine()?;
        self.contracts
            .sync_guide(version, &self.root.join("target/debug/kmp-mcp"))?;
        self.workspace.show_version_diff()?;
        Ok(format!(
            "next: commit and push this version branch, then run `scripts/release.sh candidate {version}`"
        ))
    }
}
