use std::fmt::{Display, Formatter};

use crate::domain::release_error::ReleaseError;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ReleaseArchivePath(String);

impl ReleaseArchivePath {
    pub fn parse(value: impl Into<String>) -> Result<Self, ReleaseError> {
        let value = value.into();
        if value.is_empty()
            || value.starts_with('/')
            || value.split('/').any(|component| component == "..")
        {
            return Err(ReleaseError::invalid(format!(
                "invalid release archive path `{value}`"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ReleaseArchivePath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
