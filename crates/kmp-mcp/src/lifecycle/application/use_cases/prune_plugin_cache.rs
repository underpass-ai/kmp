use crate::lifecycle::domain::cache_pruning::CachePruning;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::plugin_cache::PluginCache;

/// Use case: remove the cached releases a proved convergence superseded.
///
/// Housekeeping never decides whether the convergence succeeded, so nothing
/// here can fail the update. A release this machine will not let go of is
/// reported as kept rather than raised: on Windows a running engine holds its
/// own file, and the honest answer is to say which one stayed.
pub struct PrunePluginCache<'a> {
    cache: &'a dyn PluginCache,
}

impl<'a> PrunePluginCache<'a> {
    pub fn new(cache: &'a dyn PluginCache) -> Self {
        Self { cache }
    }

    pub fn execute(&self, installed: &PluginRoot, release: &ReleaseVersion) -> CachePruning {
        let Ok(cached) = self.cache.cached_releases(installed) else {
            return CachePruning::default();
        };
        let mut removed = Vec::new();
        let mut kept = Vec::new();
        for superseded in CachePruning::superseded(&cached, release) {
            match self.cache.remove_release(installed, &superseded) {
                Ok(()) => removed.push(superseded),
                Err(_) => kept.push(superseded),
            }
        }
        CachePruning::new(removed, kept)
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
            let root = versions.join(release);
            std::fs::create_dir_all(root.join("bin")).expect("version dir");
            std::fs::write(root.join("bin/kmp-mcp"), b"an engine").expect("engine");
        }
        versions
    }

    fn names(base: &Path) -> Vec<String> {
        let mut found = std::fs::read_dir(base)
            .expect("versions dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        found.sort();
        found
    }

    #[test]
    fn a_proved_convergence_leaves_the_installed_release_and_one_rollback() {
        // The cache of #451: twenty version directories, 69M, and nothing
        // shipped that removed or even mentioned them.
        let base = tempfile::tempdir().expect("temp");
        let versions = cache_with(
            base.path(),
            &[
                "0.1.3", "0.1.18", "0.2.5", "0.2.9", "0.4.0", "0.4.2", "0.5.0", "0.5.2", "0.6.0",
                "0.6.1",
            ],
        );
        let installed = PluginRoot::new(versions.join("0.6.1")).expect("installed root");
        let release = ReleaseVersion::parse("0.6.1").expect("release");

        let pruning = PrunePluginCache::new(&FilesystemPluginCache).execute(&installed, &release);

        assert_eq!(names(&versions), ["0.6.0", "0.6.1"]);
        assert_eq!(pruning.removed().len(), 8, "{pruning:?}");
        assert!(pruning.kept().is_empty(), "{pruning:?}");
        assert!(installed.as_path().join("bin/kmp-mcp").is_file());
    }

    #[test]
    fn a_cache_holding_only_the_installed_release_is_left_exactly_as_it_is() {
        let base = tempfile::tempdir().expect("temp");
        let versions = cache_with(base.path(), &["0.6.1"]);
        let installed = PluginRoot::new(versions.join("0.6.1")).expect("installed root");
        let release = ReleaseVersion::parse("0.6.1").expect("release");

        let pruning = PrunePluginCache::new(&FilesystemPluginCache).execute(&installed, &release);

        assert!(pruning.is_empty());
        assert_eq!(names(&versions), ["0.6.1"]);
    }

    #[test]
    fn a_directory_that_is_not_a_release_is_somebody_elses_business() {
        let base = tempfile::tempdir().expect("temp");
        let versions = cache_with(base.path(), &["0.4.0", "0.6.0", "0.6.1"]);
        std::fs::create_dir_all(versions.join("scratch")).expect("foreign dir");
        std::fs::write(versions.join("marketplace.json"), b"{}").expect("foreign file");
        let installed = PluginRoot::new(versions.join("0.6.1")).expect("installed root");
        let release = ReleaseVersion::parse("0.6.1").expect("release");

        PrunePluginCache::new(&FilesystemPluginCache).execute(&installed, &release);

        assert_eq!(
            names(&versions),
            ["0.6.0", "0.6.1", "marketplace.json", "scratch"]
        );
    }

    #[test]
    fn a_cache_that_is_not_there_is_an_answer_not_a_failure() {
        let base = tempfile::tempdir().expect("temp");
        let installed = PluginRoot::new(
            base.path()
                .join(".claude/plugins/cache/underpass/kmp/0.6.1"),
        )
        .expect("installed root");
        let release = ReleaseVersion::parse("0.6.1").expect("release");

        let pruning = PrunePluginCache::new(&FilesystemPluginCache).execute(&installed, &release);

        assert!(pruning.is_empty(), "housekeeping never fails an update");
    }
}
