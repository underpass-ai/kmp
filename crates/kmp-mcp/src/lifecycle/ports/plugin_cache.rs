use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Outbound port for the version directories a host's plugin cache keeps.
///
/// Every method is scoped by the *installed* root, so nothing here can name a
/// directory outside the cache that root already lives in. Which releases may
/// go is policy and stays in the domain.
pub trait PluginCache: Send + Sync {
    /// Every release this cache holds beside `installed`, including it.
    fn cached_releases(
        &self,
        installed: &PluginRoot,
    ) -> Result<Vec<ReleaseVersion>, LifecycleError>;

    /// Remove one cached release from the same cache `installed` lives in.
    fn remove_release(
        &self,
        installed: &PluginRoot,
        release: &ReleaseVersion,
    ) -> Result<(), LifecycleError>;
}
