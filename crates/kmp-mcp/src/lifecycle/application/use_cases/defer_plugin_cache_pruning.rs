use crate::lifecycle::domain::cache_deferral::CacheDeferral;
use crate::lifecycle::domain::cache_pruning::CachePruning;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::plugin_cache::PluginCache;

/// Use case: name the cached releases a proved convergence superseded, and
/// remove none of them.
///
/// What is superseded is exactly what it always was — the installed release
/// and one rollback survive. Only the moment of removal moved: an open
/// session still dispatches skills from the directory it started in, so an
/// update that deletes it breaks that session until the host restarts
/// (#521). The next process start collects this list instead.
pub struct DeferPluginCachePruning<'a> {
    cache: &'a dyn PluginCache,
}

impl<'a> DeferPluginCachePruning<'a> {
    pub fn new(cache: &'a dyn PluginCache) -> Self {
        Self { cache }
    }

    pub fn execute(&self, installed: &PluginRoot, release: &ReleaseVersion) -> CacheDeferral {
        let Ok(cached) = self.cache.cached_releases(installed) else {
            return CacheDeferral::default();
        };
        CacheDeferral::new(CachePruning::superseded(&cached, release))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::lifecycle::adapters::filesystem_plugin_cache::FilesystemPluginCache;

    fn cache_with(base: &Path, releases: &[&str]) -> std::path::PathBuf {
        let versions = base.join(".claude/plugins/cache/underpass/kmp");
        for release in releases {
            std::fs::create_dir_all(versions.join(release).join("skills")).expect("version dir");
        }
        versions
    }

    fn names(deferral: &CacheDeferral) -> Vec<&str> {
        deferral
            .deferred()
            .iter()
            .map(ReleaseVersion::as_str)
            .collect()
    }

    #[test]
    fn a_superseded_release_is_named_and_still_there_afterwards() {
        let base = tempfile::tempdir().expect("temp");
        let versions = cache_with(base.path(), &["0.11.0", "0.12.0", "0.12.1"]);
        let installed = PluginRoot::new(versions.join("0.12.1")).expect("installed root");

        let deferral = DeferPluginCachePruning::new(&FilesystemPluginCache).execute(
            &installed,
            &ReleaseVersion::parse("0.12.1").expect("release"),
        );

        assert_eq!(names(&deferral), ["0.11.0"]);
        assert!(
            versions.join("0.11.0/skills").is_dir(),
            "naming a release must not remove it"
        );
    }

    #[test]
    fn a_cache_that_is_not_there_defers_nothing() {
        let base = tempfile::tempdir().expect("temp");
        let installed = PluginRoot::new(
            base.path()
                .join(".claude/plugins/cache/underpass/kmp/0.12.1"),
        )
        .expect("installed root");

        let deferral = DeferPluginCachePruning::new(&FilesystemPluginCache).execute(
            &installed,
            &ReleaseVersion::parse("0.12.1").expect("release"),
        );

        assert!(deferral.is_empty());
    }
}
