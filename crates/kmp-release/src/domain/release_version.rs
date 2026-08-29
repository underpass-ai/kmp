use std::fmt::{Display, Formatter};

use crate::domain::release_error::ReleaseError;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ReleaseVersion(String);

impl ReleaseVersion {
    pub fn parse(value: impl Into<String>) -> Result<Self, ReleaseError> {
        let value = value.into();
        let split = value.split_once('-');
        let (core, suffix) = split.unwrap_or((&value, ""));
        let components = core.split('.').collect::<Vec<_>>();
        let valid_core = components.len() == 3
            && components.iter().all(|component| {
                !component.is_empty() && component.chars().all(|c| c.is_ascii_digit())
            });
        let valid_suffix = split.is_none()
            || (!suffix.is_empty()
                && suffix.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
                }));
        if !valid_core || !valid_suffix {
            return Err(ReleaseError::invalid(format!(
                "invalid release version `{value}`; expected X.Y.Z with an optional SemVer suffix"
            )));
        }
        Ok(Self(value))
    }

    pub fn tag(&self) -> String {
        format!("v{}", self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ReleaseVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
