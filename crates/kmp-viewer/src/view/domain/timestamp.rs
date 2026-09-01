//! An instant as the kernel spells them.

use std::cmp::Ordering;

use kmp_domain::compare_temporal_instants;

/// A temporal instant in the kernel's own notation — RFC3339, or a persisted
/// `unix:` stamp. The view carries instants without normalizing them; two
/// instants compare through the kernel's shared rule, and an instant the
/// kernel cannot read compares as unknown rather than being guessed at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timestamp(String);

impl Timestamp {
    /// An instant exactly as given.
    pub fn new(instant: impl Into<String>) -> Self {
        Self(instant.into())
    }

    /// The instant, byte for byte.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// How this instant orders against another, or `None` when either is in
    /// no notation the kernel reads.
    pub fn compare(&self, other: &Timestamp) -> Option<Ordering> {
        compare_temporal_instants(&self.0, &other.0)
    }
}
