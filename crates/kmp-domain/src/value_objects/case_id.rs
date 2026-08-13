use crate::DomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CaseId(String);

impl CaseId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainError::EmptyValue("case_id"));
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
