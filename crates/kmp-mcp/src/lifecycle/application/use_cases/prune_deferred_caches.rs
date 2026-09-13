use crate::lifecycle::application::use_cases::prune_plugin_cache::PrunePluginCache;
use crate::lifecycle::domain::cache_pruning::CachePruning;
use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::plugin_cache_roots::PluginCacheRoots;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::plugin_cache::PluginCache;

/// Use case: collect, at process start, the cached releases an earlier
/// convergence superseded and deferred.
///
/// This is the other half of the deferral (#521), and the half that keeps it
/// from becoming an unbounded cache leak: an update names what it superseded
/// and leaves it, and the next start removes it. Nothing here can fail a
/// start — a cache that will not tidy up is not a reason to refuse a
/// session — and the arithmetic is unchanged, so a release newer than the
/// running one is never a candidate and a deliberate downgrade still keeps
/// the version it came from.
pub struct PruneDeferredCaches<'a> {
    cache: &'a dyn PluginCache,
}

impl<'a> PruneDeferredCaches<'a> {
    pub fn new(cache: &'a dyn PluginCache) -> Self {
        Self { cache }
    }

    /// What each host's cache gave up, reported only where something did.
    pub fn execute(
        &self,
        roots: &PluginCacheRoots,
        running: &ReleaseVersion,
    ) -> Vec<(Host, CachePruning)> {
        let prune = PrunePluginCache::new(self.cache);
        roots
            .installed(running)
            .into_iter()
            .filter_map(|(host, root)| {
                let pruning = prune.execute(&root, running);
                (!pruning.is_empty()).then_some((host, pruning))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::lifecycle::adapters::filesystem_plugin_cache::FilesystemPluginCache;

    fn codex_cache(base: &Path, releases: &[&str]) -> PathBuf {
        let versions = base.join(".codex/plugins/cache/underpass/kmp");
        for release in releases {
            std::fs::create_dir_all(versions.join(release).join("skills")).expect("version dir");
        }
        versions
    }

    fn roots(base: &Path) -> PluginCacheRoots {
        PluginCacheRoots {
            home: base.to_path_buf(),
            codex_home: base.join(".codex"),
        }
    }

    #[test]
    fn a_start_collects_what_the_update_before_it_deferred() {
        let base = tempfile::tempdir().expect("temp");
        let versions = codex_cache(base.path(), &["0.11.0", "0.12.0", "0.12.1"]);

        let collected = PruneDeferredCaches::new(&FilesystemPluginCache).execute(
            &roots(base.path()),
            &ReleaseVersion::parse("0.12.1").expect("running release"),
        );

        assert_eq!(collected.len(), 1, "{collected:?}");
        assert_eq!(collected[0].0, Host::Codex);
        assert_eq!(
            collected[0]
                .1
                .removed()
                .iter()
                .map(ReleaseVersion::as_str)
                .collect::<Vec<_>>(),
            ["0.11.0"]
        );
        assert!(!versions.join("0.11.0").exists());
        assert!(versions.join("0.12.0/skills").is_dir(), "the rollback");
        assert!(versions.join("0.12.1/skills").is_dir(), "the installed one");
    }

    #[test]
    fn a_start_with_nothing_left_over_reports_nothing() {
        let base = tempfile::tempdir().expect("temp");
        codex_cache(base.path(), &["0.12.0", "0.12.1"]);

        let collected = PruneDeferredCaches::new(&FilesystemPluginCache).execute(
            &roots(base.path()),
            &ReleaseVersion::parse("0.12.1").expect("running release"),
        );

        assert!(collected.is_empty(), "{collected:?}");
    }

    #[test]
    fn a_host_this_machine_does_not_have_is_not_an_error() {
        // Only Codex is installed here; there is no Claude cache to read and
        // a start must not care.
        let base = tempfile::tempdir().expect("temp");
        codex_cache(base.path(), &["0.11.0", "0.12.0", "0.12.1"]);

        let collected = PruneDeferredCaches::new(&FilesystemPluginCache).execute(
            &roots(base.path()),
            &ReleaseVersion::parse("0.12.1").expect("running release"),
        );

        assert!(collected.iter().all(|(host, _)| *host == Host::Codex));
    }
}
