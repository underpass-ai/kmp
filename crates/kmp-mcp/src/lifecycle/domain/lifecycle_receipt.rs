use super::bridge_installation::BridgeInstallation;
use super::cache_deferral::CacheDeferral;
use super::host::Host;
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
    deferred_caches: Vec<(Host, CacheDeferral)>,
    lexical_bridge: Option<BridgeInstallation>,
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
            deferred_caches: Vec::new(),
            lexical_bridge: None,
        }
    }

    pub fn completed(
        action: LifecycleAction,
        version: ReleaseVersion,
        hosts: Vec<HostConvergence>,
        engine_proofs: Vec<HostEngineProof>,
        plugin_tree: Option<TreeDigest>,
        deferred_caches: Vec<(Host, CacheDeferral)>,
        lexical_bridge: BridgeInstallation,
    ) -> Self {
        Self {
            action,
            version,
            dry_run: false,
            hosts,
            engine_proofs,
            plugin_tree,
            deferred_caches,
            lexical_bridge: Some(lexical_bridge),
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

    /// What each host's plugin cache was left holding: the superseded
    /// releases this convergence did not remove, so a reader learns that
    /// sixty megabytes are still there and when they go, instead of being
    /// told they are already gone (#521).
    pub fn deferred_caches(&self) -> &[(Host, CacheDeferral)] {
        &self.deferred_caches
    }

    /// What this run did about the table that lets `ask` cross languages.
    /// A plan has not done anything about it yet and reports none.
    pub fn lexical_bridge(&self) -> Option<&BridgeInstallation> {
        self.lexical_bridge.as_ref()
    }
}
