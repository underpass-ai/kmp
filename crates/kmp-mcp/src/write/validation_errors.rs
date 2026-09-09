use super::validation_error::WriteValidationError;
use crate::serving::ToolError;

/// Nonempty independent record failures; the rejected packet writes nothing.
#[derive(Debug)]
pub(crate) struct WriteValidationErrors(Vec<WriteValidationError>);

impl WriteValidationErrors {
    pub(crate) fn collected(mut errors: Vec<WriteValidationError>) -> Option<Self> {
        // A shared packet error can surface identically in adjacent members.
        errors.dedup();
        (!errors.is_empty()).then_some(Self(errors))
    }
}

impl From<WriteValidationError> for WriteValidationErrors {
    fn from(error: WriteValidationError) -> Self {
        Self(vec![error])
    }
}

impl From<String> for WriteValidationErrors {
    fn from(message: String) -> Self {
        WriteValidationError::from(message).into()
    }
}

impl From<&str> for WriteValidationErrors {
    fn from(message: &str) -> Self {
        WriteValidationError::from(message).into()
    }
}

impl From<WriteValidationErrors> for ToolError {
    fn from(errors: WriteValidationErrors) -> Self {
        let count = errors.0.len();
        let mut errors = errors.0.into_iter();
        let mut result = Self::from(errors.next().expect("nonempty writer failures"));
        if count > 1 {
            result.message = format!(
                "{count} records failed validation; repair the listed fields and resend the whole packet. No changes were written; later checks may reveal further errors."
            );
            for error in errors {
                result.feedback.extend(Self::from(error).feedback);
            }
        }
        result
    }
}
