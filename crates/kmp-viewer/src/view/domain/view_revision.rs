//! The aggregate's own counter.

use std::fmt;

/// `view_revision` is not `memory_revision`: moving a window is not
/// remembering anything. Every mutation checks an expected revision against
/// this one, so the human and the agent can share the loom without either
/// yanking it from under the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewRevision(u64);

impl ViewRevision {
    /// The revision a freshly opened view carries.
    pub fn initial() -> Self {
        Self(1)
    }

    /// The revision after one applied move.
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The number the wire and the conflict prose speak.
    pub fn value(self) -> u64 {
        self.0
    }

    /// An expectation of zero means "I expect no view to exist yet" — the
    /// one expectation a vacant slot satisfies.
    pub fn expects_absence(expected: u64) -> bool {
        expected == 0
    }
}

impl From<u64> for ViewRevision {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for ViewRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_count_move_by_move_and_zero_expects_absence() {
        let first = ViewRevision::initial();
        assert_eq!(first.value(), 1);
        assert_eq!(first.next().value(), 2);
        assert!(ViewRevision::expects_absence(0));
        assert!(!ViewRevision::expects_absence(1));
        assert_eq!(format!("{}", ViewRevision::from(42)), "42");
    }
}
