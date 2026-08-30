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
