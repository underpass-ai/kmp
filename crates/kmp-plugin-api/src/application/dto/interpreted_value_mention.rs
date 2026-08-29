use serde::{Deserialize, Serialize};

use crate::domain::interpreted_value::InterpretedValue;
use crate::domain::text_span::TextSpan;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterpretedValueMention {
    pub plugin: String,
    pub ref_id: String,
    pub raw: String,
    pub span: TextSpan,
    pub value: InterpretedValue,
    pub confidence: f64,
}
