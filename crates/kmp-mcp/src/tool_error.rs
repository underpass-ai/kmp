//! What went wrong, said in a word the caller can act on.
//!
//! The `code` an agent received used to be reconstructed by substring-matching
//! the English message: anything containing `must` or `invalid` became
//! `invalid_argument`, so "the store must be migrated before it can be opened"
//! — a backend condition nothing about the arguments can fix — arrived as the
//! agent's fault. The skill tells agents to read the code and not retry
//! blindly; following that advice on a misclassified code is an infinite loop.
//!
//! Rewording a message could silently change its code, which made the code
//! untestable as a contract, and a message in any other language degraded to
//! `backend_error`.
//!
//! So the code is produced where the failure is known and travels with it.
//! Nothing here reads the message.

use std::fmt;

/// The closed set. An agent can branch on these, so adding one is a contract
/// change and removing one breaks callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorCode {
    /// The arguments cannot produce a call. Fixable by the caller, and only
    /// by the caller — this is the one code that means "try something else".
    InvalidArgument,
    /// The memory asked for is not in this store.
    NotFound,
    /// The operation collided with current state. The message distinguishes a
    /// retryable optimistic-concurrency miss from a reused idempotency key
    /// whose content does not match the accepted write.
    Conflict,
    /// The kernel could not be reached. Retrying the same call may work.
    Unavailable,
    /// No such tool on this surface.
    UnknownTool,
    /// Everything else, including any failure whose producer did not say
    /// which of the above it was. A default, not a guess about wording.
    BackendError,
}

impl ToolErrorCode {
    pub const ALL: &'static [Self] = &[
        Self::InvalidArgument,
        Self::NotFound,
        Self::Conflict,
        Self::Unavailable,
        Self::UnknownTool,
        Self::BackendError,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidArgument => "invalid_argument",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Unavailable => "unavailable",
            Self::UnknownTool => "unknown_tool",
            Self::BackendError => "backend_error",
        }
    }

    /// What a caller should do about it, in one line, for the tool surface.
    pub fn guidance(self) -> &'static str {
        match self {
            Self::InvalidArgument => {
                "the arguments cannot produce a call; fix them — retrying unchanged cannot work"
            }
            Self::NotFound => "the memory asked for is not in this store",
            Self::Conflict => {
                "the operation conflicted with current state; follow the message — a retryable \
                 write conflict is safely replayed with the same idempotency key, while a key \
                 already accepted with different content must not be reused"
            }
            Self::Unavailable => "the kernel could not be reached; the same call may work later",
            Self::UnknownTool => "no such tool on this surface",
            Self::BackendError => "the kernel failed for a reason the arguments cannot fix",
        }
    }
}

impl fmt::Display for ToolErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A failure and what kind it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    pub code: ToolErrorCode,
    pub message: String,
}

impl ToolError {
    pub fn new(code: ToolErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(ToolErrorCode::InvalidArgument, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ToolErrorCode::NotFound, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ToolErrorCode::Conflict, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ToolErrorCode::Unavailable, message)
    }

    pub fn unknown_tool(message: impl Into<String>) -> Self {
        Self::new(ToolErrorCode::UnknownTool, message)
    }

    pub fn backend(message: impl Into<String>) -> Self {
        Self::new(ToolErrorCode::BackendError, message)
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// An error that arrives as bare text is a backend failure, because whoever
/// raised it did not say otherwise. This is the default that lets a producer
/// stay untyped; it is not an inspection of the words.
impl From<String> for ToolError {
    fn from(message: String) -> Self {
        Self::backend(message)
    }
}

impl From<&str> for ToolError {
    fn from(message: &str) -> Self {
        Self::backend(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewording_a_message_cannot_change_its_code() {
        // The property the old substring matcher could not have: the code is
        // chosen by the producer, so the words are free to change.
        for message in [
            "the store must be migrated before it can be opened",
            "no se pudo abrir el almacén",
            "invalid, missing, required, not found, unavailable",
            "",
        ] {
            assert_eq!(
                ToolError::backend(message).code,
                ToolErrorCode::BackendError
            );
            assert_eq!(
                ToolError::invalid_argument(message).code,
                ToolErrorCode::InvalidArgument
            );
        }
    }

    #[test]
    fn a_backend_condition_phrased_like_a_bad_argument_stays_a_backend_error() {
        // This exact message used to be classified `invalid_argument` because
        // it contains "must", sending the agent to retry with different
        // arguments against something no argument can fix.
        let error = ToolError::backend("the store must be migrated before it can be opened");
        assert_eq!(error.code, ToolErrorCode::BackendError);
    }

    #[test]
    fn an_untyped_error_defaults_to_backend_rather_than_to_the_callers_fault() {
        let error: ToolError = "something went wrong".to_string().into();
        assert_eq!(error.code, ToolErrorCode::BackendError);
    }

    #[test]
    fn every_code_is_enumerated_and_carries_guidance() {
        assert_eq!(ToolErrorCode::ALL.len(), 6);
        for code in ToolErrorCode::ALL {
            assert!(!code.as_str().is_empty());
            assert!(!code.guidance().is_empty(), "{code} has no guidance");
        }
        // A conflict has to be sayable, or the caller cannot distinguish a
        // safe concurrency replay from a genuine content mismatch.
        assert!(ToolErrorCode::ALL.contains(&ToolErrorCode::Conflict));
    }
}
