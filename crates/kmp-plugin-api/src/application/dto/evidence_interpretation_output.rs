use serde::{Deserialize, Serialize};

use super::interpreted_value_mention::InterpretedValueMention;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceInterpretationOutput {
    pub plugin: String,
    pub values: Vec<InterpretedValueMention>,
    pub diagnostics: Vec<String>,
}
