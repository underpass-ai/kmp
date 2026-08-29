use std::fmt::{Display, Formatter};

use crate::domain::release_error::ReleaseError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SourceCommit(String);

impl SourceCommit {
    pub fn parse(value: impl Into<String>) -> Result<Self, ReleaseError> {
        let value = value.into();
        if value.len() != 40
            || !value
                .chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
        {
            return Err(ReleaseError::invalid(format!(
                "invalid source commit `{value}`; expected a lowercase 40-character SHA"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SourceCommit {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
