use serde::{Deserialize, Serialize};

use crate::domain::derivation_operation::DerivationOperation;
use crate::domain::interpreted_value::InterpretedValue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivationResult {
    pub plugin: String,
    pub operation: DerivationOperation,
    pub answer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<InterpretedValue>,
    pub included_refs: Vec<String>,
    pub excluded_refs: Vec<String>,
    pub diagnostics: Vec<String>,
}
