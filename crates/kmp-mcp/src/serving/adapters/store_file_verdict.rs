//! What one optional store file did when the store opened.

/// Whether a file beside the store took effect, and what the operator
/// should know when it did not, or did only in part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StoreFileVerdict {
    Applied,
    /// Took effect, but not as written: the reason says what was read in
    /// its place (a file that is itself the opt-in, read with defaults).
    AppliedWithWarning(String),
    Ignored(String),
}

impl StoreFileVerdict {
    /// The reason to report, when there is one.
    pub(super) fn reason(&self) -> Option<&str> {
        match self {
            Self::Applied => None,
            Self::AppliedWithWarning(reason) | Self::Ignored(reason) => Some(reason),
        }
    }

    pub(super) fn applied(&self) -> bool {
        !matches!(self, Self::Ignored(_))
    }
}

impl From<Result<(), String>> for StoreFileVerdict {
    fn from(verdict: Result<(), String>) -> Self {
        match verdict {
            Ok(()) => Self::Applied,
            Err(reason) => Self::Ignored(reason),
        }
    }
}
