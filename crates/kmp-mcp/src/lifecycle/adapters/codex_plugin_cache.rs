use std::path::{Path, PathBuf};

use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Resolves Codex's installed plugin cache independently of the marketplace
/// snapshot path exposed by `plugin list`.
#[derive(Clone, Debug)]
pub struct CodexPluginCache {
    root: PathBuf,
}

impl CodexPluginCache {
    pub fn from_environment() -> Self {
        let home = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".codex"))
            })
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(".codex")
            });
        Self::new(home)
    }

    pub fn new(codex_home: impl AsRef<Path>) -> Self {
        Self {
            root: codex_home.as_ref().join("plugins").join("cache"),
        }
    }

    pub fn plugin_root(
        &self,
        marketplace: &str,
        plugin: &str,
        version: &ReleaseVersion,
    ) -> Result<PluginRoot, LifecycleError> {
        PluginRoot::new(
            self.root
                .join(marketplace)
                .join(plugin)
                .join(version.as_str()),
        )
    }
}
