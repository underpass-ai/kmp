use serde::{Deserialize, Serialize};

use crate::domain::interpreted_value::InterpretedValue;
use crate::domain::operand_label::OperandLabel;
use crate::domain::operand_role::OperandRole;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivationOperand {
    pub ref_id: String,
    pub label: OperandLabel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<OperandRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<InterpretedValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl DerivationOperand {
    pub fn included(ref_id: impl Into<String>, value: InterpretedValue) -> Self {
        Self {
            ref_id: ref_id.into(),
            label: OperandLabel::Include,
            role: None,
            entity: None,
            value: Some(value),
            raw: None,
            reason: None,
        }
    }

    pub fn with_role(mut self, role: OperandRole) -> Self {
        self.role = Some(role);
        self
    }

    pub fn with_entity(mut self, entity: impl Into<String>) -> Self {
        self.entity = Some(entity.into());
        self
    }
}
