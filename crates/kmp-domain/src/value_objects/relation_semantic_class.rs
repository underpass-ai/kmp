use serde::{Deserialize, Serialize};

use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationSemanticClass {
    Structural,
    Causal,
    Motivational,
    Procedural,
    Evidential,
    Constraint,
}

impl RelationSemanticClass {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value.trim() {
            "structural" => Ok(Self::Structural),
            "causal" => Ok(Self::Causal),
            "motivational" => Ok(Self::Motivational),
            "procedural" => Ok(Self::Procedural),
            "evidential" => Ok(Self::Evidential),
            "constraint" => Ok(Self::Constraint),
            other => Err(DomainError::InvalidState(format!(
                "invalid relation semantic_class `{other}`"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Structural => "structural",
            Self::Causal => "causal",
            Self::Motivational => "motivational",
            Self::Procedural => "procedural",
            Self::Evidential => "evidential",
            Self::Constraint => "constraint",
        }
    }

    /// Returns a salience rank for token-budget packing.
    ///
    /// Lower values are higher priority. Explanatory classes (causal,
    /// motivational, evidential, constraint) rank before structural and
    /// procedural because the paper's core claim is that explanatory
    /// relationships carry the dominant signal for diagnosis and recovery.
    pub fn salience_rank(&self) -> u8 {
        match self {
            Self::Causal => 0,
            Self::Motivational => 1,
            Self::Evidential => 2,
            Self::Constraint => 3,
            Self::Procedural => 4,
            Self::Structural => 5,
        }
    }
}
