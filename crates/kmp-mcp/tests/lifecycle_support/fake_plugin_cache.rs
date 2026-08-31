use std::sync::Mutex;

use kmp_mcp::lifecycle::PluginCache;
use kmp_mcp::lifecycle::domain::lifecycle_error::LifecycleError;
use kmp_mcp::lifecycle::domain::plugin_root::PluginRoot;
use kmp_mcp::lifecycle::domain::release_version::ReleaseVersion;

/// A host plugin cache with no disk under it: it remembers which releases it
/// holds and which ones were asked to go.
#[derive(Debug, Default)]
pub struct FakePluginCache {
    held: Mutex<Vec<ReleaseVersion>>,
    removed: Mutex<Vec<ReleaseVersion>>,
}

impl FakePluginCache {
    pub fn holding(releases: &[&str]) -> Self {
        Self {
            held: Mutex::new(
                releases
                    .iter()
                    .map(|raw| ReleaseVersion::parse(raw).expect("release"))
                    .collect(),
            ),
            removed: Mutex::new(Vec::new()),
        }
    }

    pub fn removed(&self) -> Vec<String> {
        self.removed
            .lock()
            .expect("removed")
            .iter()
            .map(ToString::to_string)
            .collect()
    }
}

impl PluginCache for FakePluginCache {
    fn cached_releases(
        &self,
        _installed: &PluginRoot,
    ) -> Result<Vec<ReleaseVersion>, LifecycleError> {
        Ok(self.held.lock().expect("held").clone())
    }

    fn remove_release(
        &self,
        _installed: &PluginRoot,
        release: &ReleaseVersion,
    ) -> Result<(), LifecycleError> {
        self.held
            .lock()
            .expect("held")
            .retain(|held| held != release);
        self.removed.lock().expect("removed").push(release.clone());
        Ok(())
    }
}
