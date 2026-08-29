use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::release_version::ReleaseVersion;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginEngineResolution {
    selected: EngineExecutable,
    warning: Option<String>,
    version: ReleaseVersion,
}

impl PluginEngineResolution {
    pub fn exact(selected: EngineExecutable, version: ReleaseVersion) -> Self {
        Self {
            selected,
            warning: None,
            version,
        }
    }

    pub fn replacing_stale_cache(
        selected: EngineExecutable,
        version: ReleaseVersion,
        stale: &ReleaseVersion,
        plugin: &ReleaseVersion,
    ) -> Self {
        Self {
            selected,
            warning: Some(format!(
                "KMP plugin: cache engine {stale} does not match plugin {plugin}; using matching PATH engine. Run kmp setup to repair the plugin-owned engine."
            )),
            version,
        }
    }

    pub fn selected(&self) -> &EngineExecutable {
        &self.selected
    }

    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    pub fn version(&self) -> &ReleaseVersion {
        &self.version
    }
}
