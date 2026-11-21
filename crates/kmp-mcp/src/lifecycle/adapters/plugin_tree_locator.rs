use std::path::{Path, PathBuf};

use crate::lifecycle::adapters::native_skill_mirror::NATIVE_SKILL_NAMES;
use crate::lifecycle::domain::plugin_cache_roots::PluginCacheRoots;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Where a KMP plugin tree for one release is on this machine, so a host
/// without a plugin manager (Hermes, Pi) can be given the `kmp-*` skills.
///
/// It used to look only four directories above the running executable,
/// which finds a development checkout and nothing else: an installed engine
/// in `~/.local/bin` never has a tree there, and `setup --hermes` reported
/// "completed" with no skill installed (S12). The candidates, in the order
/// they are trusted: the launcher's `KMP_PLUGIN_ROOT`, a development
/// checkout beside this binary, then the Claude Code and Codex caches of
/// that release. A candidate counts only when it carries skills.
#[derive(Clone, Debug, Default)]
pub struct PluginTreeLocator {
    candidates: Vec<PathBuf>,
}

impl PluginTreeLocator {
    pub fn from_environment(release: &ReleaseVersion) -> Self {
        let mut candidates = Vec::new();
        if let Some(root) = std::env::var_os("KMP_PLUGIN_ROOT").filter(|root| !root.is_empty()) {
            candidates.push(PathBuf::from(root));
        }
        if let Some(root) = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.ancestors().nth(4).map(|root| root.join("plugins/kmp")))
        {
            candidates.push(root);
        }
        if let Some(home) = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
        {
            let codex_home = std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"));
            let roots = PluginCacheRoots { home, codex_home };
            candidates.extend(
                roots
                    .installed(release)
                    .into_iter()
                    .map(|(_, root)| root.as_path().to_path_buf()),
            );
        }
        Self::with_candidates(candidates)
    }

    pub fn with_candidates(candidates: Vec<PathBuf>) -> Self {
        Self { candidates }
    }

    /// The first candidate that carries at least one of the native skills.
    pub fn locate(&self) -> Option<PathBuf> {
        self.candidates
            .iter()
            .find(|root| !Self::skills_in(root).is_empty())
            .cloned()
    }

    /// The native skills a plugin tree carries, by name.
    pub fn skills_in(root: &Path) -> Vec<&'static str> {
        NATIVE_SKILL_NAMES
            .iter()
            .copied()
            .filter(|name| root.join("skills").join(name).join("SKILL.md").is_file())
            .collect()
    }

    /// The candidates looked at, for a failure that has to say where.
    pub fn candidates(&self) -> &[PathBuf] {
        &self.candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_candidate_that_carries_skills_wins() {
        let base = tempfile::tempdir().expect("temp");
        let hollow = base.path().join("hollow");
        let tree = base.path().join("tree");
        std::fs::create_dir_all(&hollow).expect("hollow");
        std::fs::create_dir_all(tree.join("skills/kmp-memory")).expect("skill");
        std::fs::write(tree.join("skills/kmp-memory/SKILL.md"), "# memory").expect("skill");

        let locator = PluginTreeLocator::with_candidates(vec![hollow, tree.clone()]);

        assert_eq!(locator.locate(), Some(tree.clone()));
        assert_eq!(PluginTreeLocator::skills_in(&tree), ["kmp-memory"]);
    }
}
