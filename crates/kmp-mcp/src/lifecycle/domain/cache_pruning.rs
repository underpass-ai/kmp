use super::release_version::ReleaseVersion;

/// What a convergence did to the cached releases beside the one it installed.
///
/// A host's plugin cache only ever grew: twenty version directories and 69M
/// on a machine that had been updated twenty times, with nothing shipped that
/// removed or even mentioned them (#451). A proved convergence is the moment
/// that can be fixed — the new release is installed, its tools answered, and
/// every host points at it — so what came before is dead weight, except the
/// one release immediately before it, which is the rollback.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CachePruning {
    removed: Vec<ReleaseVersion>,
    kept: Vec<ReleaseVersion>,
}

impl CachePruning {
    /// The cached releases a convergence to `installed` leaves superseded,
    /// newest first.
    ///
    /// Everything older than the installed release except the newest of them,
    /// which stays as the rollback. A release *newer* than the installed one
    /// is never chosen: a deliberate downgrade must not delete the version it
    /// came from.
    pub fn superseded(
        cached: &[ReleaseVersion],
        installed: &ReleaseVersion,
    ) -> Vec<ReleaseVersion> {
        let mut older = cached
            .iter()
            .filter(|release| installed.is_newer_than(release))
            .cloned()
            .collect::<Vec<_>>();
        older.sort_by(|left, right| {
            if left.is_newer_than(right) {
                std::cmp::Ordering::Less
            } else if right.is_newer_than(left) {
                std::cmp::Ordering::Greater
            } else {
                left.as_str().cmp(right.as_str())
            }
        });
        older.into_iter().skip(1).collect()
    }

    pub fn new(removed: Vec<ReleaseVersion>, kept: Vec<ReleaseVersion>) -> Self {
        Self { removed, kept }
    }

    pub fn removed(&self) -> &[ReleaseVersion] {
        &self.removed
    }

    /// Superseded releases this machine would not let go of. On Windows a
    /// running engine holds its own file; on Linux it does not, because an
    /// unlinked executable keeps running from the inode it already opened.
    pub fn kept(&self) -> &[ReleaseVersion] {
        &self.kept
    }

    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.kept.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn versions(raw: &[&str]) -> Vec<ReleaseVersion> {
        raw.iter()
            .map(|value| ReleaseVersion::parse(value).expect("version"))
            .collect()
    }

    fn names(releases: &[ReleaseVersion]) -> Vec<&str> {
        releases.iter().map(ReleaseVersion::as_str).collect()
    }

    #[test]
    fn the_installed_release_and_its_rollback_survive() {
        // The twenty directories of #451, in the order a reader found them.
        let cached = versions(&[
            "0.1.3", "0.1.18", "0.2.5", "0.2.9", "0.4.0", "0.4.2", "0.5.0", "0.5.2", "0.6.0",
            "0.6.1",
        ]);
        let installed = ReleaseVersion::parse("0.6.1").expect("installed");

        let superseded = CachePruning::superseded(&cached, &installed);

        assert!(!names(&superseded).contains(&"0.6.1"), "the one in use");
        assert!(!names(&superseded).contains(&"0.6.0"), "the rollback");
        assert_eq!(superseded.len(), 8, "{:?}", names(&superseded));
        assert_eq!(names(&superseded)[0], "0.5.2", "newest first");
    }

    #[test]
    fn a_first_install_supersedes_nothing() {
        let installed = ReleaseVersion::parse("0.6.1").expect("installed");
        assert!(CachePruning::superseded(&versions(&["0.6.1"]), &installed).is_empty());
        assert!(CachePruning::superseded(&[], &installed).is_empty());
    }

    #[test]
    fn one_release_before_the_installed_one_is_the_rollback_and_nothing_else_goes() {
        let installed = ReleaseVersion::parse("0.6.1").expect("installed");
        let superseded = CachePruning::superseded(&versions(&["0.6.0", "0.6.1"]), &installed);
        assert!(superseded.is_empty(), "{:?}", names(&superseded));
    }

    #[test]
    fn a_downgrade_never_deletes_the_version_it_came_from() {
        // Converging back to 0.5.2 must leave 0.6.0 and 0.6.1 alone: they are
        // where this machine returns to, not what it left behind.
        let installed = ReleaseVersion::parse("0.5.2").expect("installed");
        let superseded =
            CachePruning::superseded(&versions(&["0.5.0", "0.5.2", "0.6.0", "0.6.1"]), &installed);
        assert_eq!(names(&superseded), Vec::<&str>::new());
    }

    #[test]
    fn ordering_is_by_release_not_by_directory_name() {
        // Sorted as text, 0.1.18 comes before 0.1.3 and the wrong one is kept.
        let installed = ReleaseVersion::parse("0.2.0").expect("installed");
        let superseded = CachePruning::superseded(&versions(&["0.1.3", "0.1.18"]), &installed);
        assert_eq!(names(&superseded), ["0.1.3"], "0.1.18 is the rollback");
    }
}
