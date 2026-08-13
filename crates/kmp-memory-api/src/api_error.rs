use std::fmt;

use serde::{Deserialize, Serialize};

/// How this contract fails.
///
/// Three shapes, because a consumer acts differently on each: waiting is a
/// remedy for `Unavailable`, asking about something else is the remedy for
/// `NotFound`, and `Refused` means the kernel looked at the request and said
/// no — retrying it unchanged earns the same answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiError {
    Unavailable { reason: String },
    NotFound { what: String },
    Refused { reason: String },
}

impl ApiError {
    /// Whether trying again, unchanged, could plausibly succeed.
    ///
    /// Published on the error rather than left to the consumer, because a
    /// consumer keeping its own table of which errors are worth retrying goes
    /// stale the first time this enum grows.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Unavailable { .. })
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { reason } => write!(f, "the memory kernel is unavailable: {reason}"),
            Self::NotFound { what } => write!(f, "not found: {what}"),
            Self::Refused { reason } => write!(f, "the memory kernel refused: {reason}"),
        }
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unavailability_invites_a_retry() {
        assert!(
            ApiError::Unavailable {
                reason: "opening".to_string()
            }
            .is_transient()
        );
        assert!(
            !ApiError::NotFound {
                what: "about project:x".to_string()
            }
            .is_transient()
        );
        assert!(
            !ApiError::Refused {
                reason: "empty about".to_string()
            }
            .is_transient(),
            "retrying a refusal unchanged asks the same question and earns the same answer"
        );
    }
}
