use super::host_convergence::HostConvergence;
use super::host_engine_proof::HostEngineProof;
use super::lifecycle_action::LifecycleAction;
use super::release_version::ReleaseVersion;
use super::tree_digest::TreeDigest;

/// Domain outcome emitted only after the requested convergence is proved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleReceipt {
    action: LifecycleAction,
    version: ReleaseVersion,
    dry_run: bool,
    hosts: Vec<HostConvergence>,
    engine_proofs: Vec<HostEngineProof>,
    plugin_tree: Option<TreeDigest>,
}

impl LifecycleReceipt {
    pub fn planned(
        action: LifecycleAction,
        version: ReleaseVersion,
        hosts: Vec<HostConvergence>,
    ) -> Self {
        Self {
            action,
            version,
            dry_run: true,
            hosts,
            engine_proofs: Vec::new(),
            plugin_tree: None,
        }
    }

    pub fn completed(
        action: LifecycleAction,
        version: ReleaseVersion,
        hosts: Vec<HostConvergence>,
        engine_proofs: Vec<HostEngineProof>,
        plugin_tree: Option<TreeDigest>,
    ) -> Self {
        Self {
            action,
            version,
            dry_run: false,
            hosts,
            engine_proofs,
            plugin_tree,
        }
    }

    pub fn action(&self) -> LifecycleAction {
        self.action
    }

    pub fn version(&self) -> &ReleaseVersion {
        &self.version
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    pub fn hosts(&self) -> &[HostConvergence] {
        &self.hosts
    }

    pub fn engine_proofs(&self) -> &[HostEngineProof] {
        &self.engine_proofs
    }

    pub fn plugin_tree(&self) -> Option<&TreeDigest> {
        self.plugin_tree.as_ref()
    }
}
