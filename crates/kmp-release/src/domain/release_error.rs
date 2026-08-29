use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReleaseError(String);

impl ReleaseError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn io(action: &str, path: &std::path::Path, error: &std::io::Error) -> Self {
        Self(format!("could not {action} `{}`: {error}", path.display()))
    }
}

impl Display for ReleaseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for ReleaseError {}
