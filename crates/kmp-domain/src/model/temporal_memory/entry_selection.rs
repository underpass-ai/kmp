use std::collections::BTreeSet;

use crate::DomainError;

/// Explicit history entries to select; scope, clocks and proof remain separate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalEntrySelection {
    refs: BTreeSet<String>,
}

impl TemporalEntrySelection {
    pub fn new(refs: Vec<String>) -> Result<Self, DomainError> {
        if refs.is_empty() {
            return Err(DomainError::InvalidState(
                "temporal refs must be non-empty; omit refs to select all matching entries".into(),
            ));
        }
        if refs
            .iter()
            .any(|value| value.is_empty() || value.trim() != value)
        {
            return Err(DomainError::InvalidState(
                "temporal refs must contain non-blank references without surrounding whitespace"
                    .into(),
            ));
        }
        let count = refs.len();
        let refs = refs.into_iter().collect::<BTreeSet<_>>();
        if refs.len() != count {
            return Err(DomainError::InvalidState(
                "temporal refs must be unique".into(),
            ));
        }
        Ok(Self { refs })
    }

    pub fn admits(&self, reference: &str) -> bool {
        self.refs.contains(reference)
    }
}
