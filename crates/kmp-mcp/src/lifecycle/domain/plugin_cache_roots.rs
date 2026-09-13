use std::path::PathBuf;

use super::host::Host;
use super::plugin_root::PluginRoot;
use super::release_version::ReleaseVersion;

/// Where each native host keeps the KMP versions it has installed.
///
/// Taken as values rather than read from the environment, so a process start
/// can be exercised against a temporary tree. The layouts are the hosts'
/// own — Claude Code under `~/.claude/plugins/cache`, Codex under its
/// `CODEX_HOME` — and both are keyed by marketplace, plugin and version.
/// Naming them directly is what lets a start tidy a cache up without asking
/// a host CLI anything.
pub struct PluginCacheRoots {
    pub home: PathBuf,
    pub codex_home: PathBuf,
}

impl PluginCacheRoots {
    /// The version directory each host's cache would hold for `release`.
    ///
    /// A path is named, never required to exist: a machine with one host
    /// installed simply has one cache, and looking into the other finds
    /// nothing to collect. A root that is not absolute is not a cache this
    /// process may touch, and is left out.
    pub fn installed(&self, release: &ReleaseVersion) -> Vec<(Host, PluginRoot)> {
        [
            (Host::Claude, self.home.join(".claude/plugins/cache")),
            (Host::Codex, self.codex_home.join("plugins/cache")),
        ]
        .into_iter()
        .filter_map(|(host, cache)| {
            PluginRoot::new(cache.join("underpass").join("kmp").join(release.as_str()))
                .ok()
                .map(|root| (host, root))
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_host_cache_is_named_by_its_own_layout() {
        let roots = PluginCacheRoots {
            home: PathBuf::from("/home/reader"),
            codex_home: PathBuf::from("/home/reader/.codex"),
        };

        let installed = roots.installed(&ReleaseVersion::parse("0.12.1").expect("release"));

        assert_eq!(
            installed
                .iter()
                .map(|(host, root)| (host.to_string(), root.as_path().display().to_string()))
                .collect::<Vec<_>>(),
            vec![
                (
                    "claude".to_string(),
                    "/home/reader/.claude/plugins/cache/underpass/kmp/0.12.1".to_string()
                ),
                (
                    "codex".to_string(),
                    "/home/reader/.codex/plugins/cache/underpass/kmp/0.12.1".to_string()
                ),
            ]
        );
    }

    #[test]
    fn a_home_this_process_could_not_resolve_names_no_cache_at_all() {
        // Without HOME the composition root falls back to a relative path,
        // and pruning relative to a working directory is not housekeeping.
        let roots = PluginCacheRoots {
            home: PathBuf::from("."),
            codex_home: PathBuf::from("./.codex"),
        };

        assert!(
            roots
                .installed(&ReleaseVersion::parse("0.12.1").expect("release"))
                .is_empty()
        );
    }
}
