use std::path::{Path, PathBuf};

/// The tracked files whose bytes bind a release candidate to the tree it was
/// built from. A candidate is only promotable while every one of them is
/// byte-identical to the tree being tagged, so the selector is also what names
/// the file that moved when a candidate stops matching.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CandidateInputSelector;

impl CandidateInputSelector {
    const EXACT: [&'static str; 11] = [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "LICENSE",
        "NOTICE",
        "THIRD_PARTY_NOTICES.md",
        ".github/workflows/release.yml",
        ".agents/plugins/marketplace.json",
        ".claude-plugin/marketplace.json",
        "scripts/ci/install-protoc.sh",
        "scripts/ci/install-rust-toolchain.sh",
    ];

    const PREFIXES: [&'static str; 6] = [
        "crates/",
        "api/",
        ".github/actions/install-rust/",
        "distribution/mcpb/",
        "plugins/kmp/",
        "scripts/plugin/",
    ];

    pub fn new() -> Self {
        Self
    }

    pub fn includes(&self, relative: &Path) -> bool {
        let relative = relative.to_string_lossy().replace('\\', "/");
        Self::EXACT.contains(&relative.as_str())
            || Self::PREFIXES
                .iter()
                .any(|prefix| relative.starts_with(prefix))
    }

    /// The selected paths in the stable order the input digest hashes them in.
    pub fn select(&self, tracked: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
        let mut selected = tracked
            .into_iter()
            .filter(|path| self.includes(path))
            .collect::<Vec<_>>();
        selected.sort();
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_claude_catalog_is_a_candidate_input() {
        assert!(
            CandidateInputSelector::new().includes(Path::new(".claude-plugin/marketplace.json"))
        );
    }

    #[test]
    fn documentation_is_not_a_candidate_input() {
        let selector = CandidateInputSelector::new();

        assert!(!selector.includes(Path::new("docs/development/releasing.md")));
        assert!(!selector.includes(Path::new("CHANGELOG.md")));
    }

    #[test]
    fn selection_is_sorted_so_the_digest_is_stable() {
        let selected = CandidateInputSelector::new().select([
            PathBuf::from("plugins/kmp/README.md"),
            PathBuf::from("README.md"),
            PathBuf::from("Cargo.toml"),
        ]);

        assert_eq!(
            selected,
            vec![
                PathBuf::from("Cargo.toml"),
                PathBuf::from("plugins/kmp/README.md"),
            ]
        );
    }
}
