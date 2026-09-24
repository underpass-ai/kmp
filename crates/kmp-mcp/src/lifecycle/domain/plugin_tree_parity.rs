use super::host::Host;
use super::lifecycle_error::LifecycleError;
use super::tree_digest::TreeDigest;

/// Byte-for-byte agreement between the marketplace plugin trees that hosts
/// installed. Only hosts that install such a tree take part: a Hermes home is
/// the host's own configuration directory and never equals a plugin tree
/// (#849).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginTreeParity {
    /// No host that installs a plugin tree was observed.
    Unobserved,
    /// Every observed plugin tree has this digest.
    Identical(TreeDigest),
    /// At least two observed plugin trees disagree.
    Divergent(Vec<(Host, TreeDigest)>),
}

impl PluginTreeParity {
    /// Judge the observed digests, ignoring any host that installs no tree.
    pub fn of(observed: impl IntoIterator<Item = (Host, TreeDigest)>) -> Self {
        let trees = observed
            .into_iter()
            .filter(|(host, _)| host.installs_plugin_tree())
            .collect::<Vec<_>>();
        match trees.first() {
            None => Self::Unobserved,
            Some((_, expected)) if trees.iter().all(|(_, digest)| digest == expected) => {
                Self::Identical(expected.clone())
            }
            Some(_) => Self::Divergent(trees),
        }
    }

    /// The agreed digest, if any tree was observed; an error that names every
    /// disagreeing host otherwise.
    pub fn require_identical(self) -> Result<Option<TreeDigest>, LifecycleError> {
        match self {
            Self::Unobserved => Ok(None),
            Self::Identical(digest) => Ok(Some(digest)),
            Self::Divergent(_) => Err(LifecycleError::TreeMismatch(format!(
                "{} installed different KMP plugin trees: {}",
                self.hosts(),
                self.digests()
            ))),
        }
    }

    /// The disagreeing hosts, as `claude and codex`. Empty unless divergent.
    pub fn hosts(&self) -> String {
        let Self::Divergent(trees) = self else {
            return String::new();
        };
        let names = trees
            .iter()
            .map(|(host, _)| host.to_string())
            .collect::<Vec<_>>();
        match names.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            _ => names.concat(),
        }
    }

    /// Each disagreeing host with its digest. Empty unless divergent.
    pub fn digests(&self) -> String {
        let Self::Divergent(trees) = self else {
            return String::new();
        };
        trees
            .iter()
            .map(|(host, digest)| format!("{host} {digest}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(fill: &str) -> TreeDigest {
        TreeDigest::sha256(fill.repeat(64))
    }

    #[test]
    fn a_hermes_home_never_breaks_parity_between_identical_trees() {
        let parity = PluginTreeParity::of([
            (Host::Claude, digest("a")),
            (Host::Codex, digest("a")),
            (Host::Hermes, digest("c")),
        ]);
        assert_eq!(parity, PluginTreeParity::Identical(digest("a")));
    }

    #[test]
    fn a_machine_with_only_hermes_observes_no_plugin_tree() {
        let parity = PluginTreeParity::of([(Host::Hermes, digest("c"))]);
        assert_eq!(parity, PluginTreeParity::Unobserved);
        assert_eq!(parity.require_identical().expect("no tree"), None);
    }

    #[test]
    fn a_divergence_names_exactly_the_hosts_that_disagree() {
        let error = PluginTreeParity::of([
            (Host::Claude, digest("a")),
            (Host::Codex, digest("b")),
            (Host::Hermes, digest("c")),
        ])
        .require_identical()
        .expect_err("different trees");
        let detail = error.to_string();
        assert!(matches!(error, LifecycleError::TreeMismatch(_)));
        assert_eq!(
            detail,
            format!(
                "claude and codex installed different KMP plugin trees: claude {}, codex {}",
                digest("a"),
                digest("b")
            )
        );
    }

    #[test]
    fn a_single_plugin_tree_is_its_own_agreement() {
        let parity = PluginTreeParity::of([(Host::Codex, digest("a"))]);
        assert_eq!(
            parity.require_identical().expect("one tree"),
            Some(digest("a"))
        );
    }
}
