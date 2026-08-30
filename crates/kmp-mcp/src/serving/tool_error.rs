use std::fmt;

pub use crate::serving::tool_error_code::ToolErrorCode;

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
