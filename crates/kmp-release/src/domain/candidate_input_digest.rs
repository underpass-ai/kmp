use std::fmt::{Display, Formatter};

use crate::domain::release_error::ReleaseError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CandidateInputDigest(String);

impl CandidateInputDigest {
    pub fn parse(value: impl Into<String>) -> Result<Self, ReleaseError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
        {
            return Err(ReleaseError::invalid(format!(
                "invalid SHA-256 digest `{value}`"
            )));
        }
        Ok(Self(value))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for CandidateInputDigest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
