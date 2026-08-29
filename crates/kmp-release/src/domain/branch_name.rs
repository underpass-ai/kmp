use std::fmt::{Display, Formatter};

use crate::domain::release_error::ReleaseError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchName(String);

impl BranchName {
    pub fn parse(value: impl Into<String>) -> Result<Self, ReleaseError> {
        let value = value.into();
        if value.trim().is_empty() || value == "HEAD" {
            return Err(ReleaseError::invalid(
                "release workflow requires a named branch",
            ));
        }
        Ok(Self(value))
    }

    pub fn is_main(&self) -> bool {
        self.0 == "main"
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for BranchName {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
