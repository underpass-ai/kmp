use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::ports::release_contracts::ReleaseContracts;
use crate::ports::release_workspace::ReleaseWorkspace;

pub struct PrepareReleaseWorkflow<'a, C, W> {
    contracts: &'a C,
    workspace: &'a W,
}

impl<'a, C: ReleaseContracts, W: ReleaseWorkspace> PrepareReleaseWorkflow<'a, C, W> {
    pub fn new(contracts: &'a C, workspace: &'a W) -> Self {
        Self {
            contracts,
            workspace,
        }
    }

    pub fn execute(&self, version: &ReleaseVersion) -> Result<String, ReleaseError> {
        self.contracts.sync_readmes()?;
        self.contracts.prepare_changelog(version)?;
        self.contracts.prepare_version(version)?;
        self.workspace.refresh_lockfile()?;
        // Guide sync inspects the engine this build produced. Where Cargo put
        // it is Cargo's answer to give, not this step's to assume (#723).
        let engine = self.workspace.build_engine()?;
        self.contracts.sync_guide(version, &engine)?;
        self.workspace.show_version_diff()?;
        Ok(format!(
            "next: commit and push this version branch, then run `scripts/release.sh candidate {version}`"
        ))
    }
}
