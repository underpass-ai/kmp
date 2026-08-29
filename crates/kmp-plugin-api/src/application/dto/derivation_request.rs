use serde::{Deserialize, Serialize};

use crate::domain::derivation_operation::DerivationOperation;

use super::derivation_operand::DerivationOperand;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivationRequest {
    pub question: String,
    pub operation: DerivationOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub operands: Vec<DerivationOperand>,
}
