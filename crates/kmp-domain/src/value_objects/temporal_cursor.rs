use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalDirection {
    Goto,
    Near,
    Rewind,
    Forward,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalCursor {
    Ref(String),
    Time(String),
    Sequence(u32),
}

impl TemporalCursor {
    pub fn ref_id(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = normalize_required(value.into(), "temporal cursor ref")?;
        Ok(Self::Ref(value))
    }

    pub fn time(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = normalize_required(value.into(), "temporal cursor time")?;
        Ok(Self::Time(value))
    }

    pub fn sequence(value: u32) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::InvalidState(
                "temporal cursor sequence must be greater than zero".to_string(),
            ));
        }
        Ok(Self::Sequence(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemporalWindow {
    before_entries: usize,
    after_entries: usize,
}

impl TemporalWindow {
    pub fn new(before_entries: usize, after_entries: usize) -> Self {
        Self {
            before_entries,
            after_entries,
        }
    }

    pub fn before_entries(&self) -> usize {
        self.before_entries
    }

    pub fn after_entries(&self) -> usize {
        self.after_entries
    }
}

impl Default for TemporalWindow {
    fn default() -> Self {
        Self {
            before_entries: 2,
            after_entries: 2,
        }
    }
}

fn normalize_required(value: String, field: &'static str) -> Result<String, DomainError> {
    let value = value.trim();
    if value.is_empty() {
        Err(DomainError::EmptyValue(field))
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use crate::{DomainError, TemporalCursor};

    #[test]
    fn cursor_requires_non_empty_values() {
        assert_eq!(
            TemporalCursor::ref_id(" ").expect_err("empty ref must fail"),
            DomainError::EmptyValue("temporal cursor ref")
        );
        assert!(TemporalCursor::sequence(1).is_ok());
        assert!(TemporalCursor::sequence(0).is_err());
    }
}
