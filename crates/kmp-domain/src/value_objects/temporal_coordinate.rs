use crate::{DomainError, RelationExplanation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalCoordinate {
    dimension: String,
    scope_id: String,
    sequence: Option<u32>,
    rank: Option<u32>,
    occurred_at: Option<String>,
    observed_at: Option<String>,
    ingested_at: Option<String>,
    valid_from: Option<String>,
    valid_until: Option<String>,
}

impl TemporalCoordinate {
    pub fn from_relation_explanation(
        explanation: &RelationExplanation,
    ) -> Result<Option<Self>, DomainError> {
        let Some(dimension) = normalize_optional(explanation.dimension()) else {
            return Ok(None);
        };
        let Some(scope_id) = normalize_optional(explanation.scope_id()) else {
            return Ok(None);
        };

        Ok(Some(Self {
            dimension,
            scope_id,
            sequence: explanation.sequence(),
            rank: explanation.rank(),
            occurred_at: normalize_optional(explanation.occurred_at()),
            observed_at: normalize_optional(explanation.observed_at()),
            ingested_at: normalize_optional(explanation.ingested_at()),
            valid_from: normalize_optional(explanation.valid_from()),
            valid_until: normalize_optional(explanation.valid_until()),
        }))
    }

    pub fn cursor_time(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = normalize_required(value.into(), "temporal cursor time")?;
        Ok(Self {
            dimension: String::new(),
            scope_id: String::new(),
            sequence: None,
            rank: None,
            occurred_at: Some(value),
            observed_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
        })
    }

    pub fn cursor_sequence(value: u32) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::InvalidState(
                "temporal cursor sequence must be greater than zero".to_string(),
            ));
        }
        Ok(Self {
            dimension: String::new(),
            scope_id: String::new(),
            sequence: Some(value),
            rank: None,
            occurred_at: None,
            observed_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
        })
    }

    pub fn dimension(&self) -> &str {
        &self.dimension
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub fn sequence(&self) -> Option<u32> {
        self.sequence
    }

    pub fn rank(&self) -> Option<u32> {
        self.rank
    }

    pub fn occurred_at(&self) -> Option<&str> {
        self.occurred_at.as_deref()
    }

    pub fn observed_at(&self) -> Option<&str> {
        self.observed_at.as_deref()
    }

    pub fn ingested_at(&self) -> Option<&str> {
        self.ingested_at.as_deref()
    }

    pub fn valid_from(&self) -> Option<&str> {
        self.valid_from.as_deref()
    }

    pub fn valid_until(&self) -> Option<&str> {
        self.valid_until.as_deref()
    }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalize_required(value: String, field: &'static str) -> Result<String, DomainError> {
    let value = value.trim();
    if value.is_empty() {
        Err(DomainError::EmptyValue(field))
    } else {
        Ok(value.to_string())
    }
}
