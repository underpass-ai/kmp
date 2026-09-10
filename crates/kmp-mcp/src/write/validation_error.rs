use serde_json::{Value, json};

use super::json_value_type::JsonValueType;
use crate::serving::ToolError;

/// A writer refusal, located where the rule is checked, never by parsing prose.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WriteValidationError {
    pub(crate) message: String,
    code: &'static str,
    field: String,
    global: bool,
    action: Option<Value>,
    allowed_values: Option<Box<[String]>>,
    type_mismatch: Option<(JsonValueType, JsonValueType)>,
}

impl WriteValidationError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: "INVALID_WRITE",
            field: String::new(),
            global: false,
            action: None,
            allowed_values: None,
            type_mismatch: None,
        }
    }

    pub(crate) fn at(mut self, field: impl Into<String>) -> Self {
        self.field = field.into();
        self
    }

    pub(crate) fn code(mut self, code: &'static str) -> Self {
        self.code = code;
        self
    }

    pub(crate) fn global(mut self) -> Self {
        self.global = true;
        self
    }

    pub(crate) fn within(mut self, field: &str) -> Self {
        if !self.global {
            self.field = if self.field.is_empty() {
                field.to_owned()
            } else {
                format!("{field}.{}", self.field)
            };
        }
        self
    }

    pub(crate) fn allowed_values(
        mut self,
        values: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Self {
        self.allowed_values = Some(
            values
                .into_iter()
                .map(|value| value.as_ref().to_owned())
                .collect(),
        );
        self
    }

    pub(super) fn wrong_type(field: &str, expected: JsonValueType, value: &Value) -> Self {
        let received = JsonValueType::from(value);
        let mut error = Self::new(format!(
            "{field} must be a JSON {expected}; received {received}"
        ))
        .at(field)
        .code("INVALID_TYPE");
        error.type_mismatch = Some((expected, received));
        error
    }
}

impl From<String> for WriteValidationError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for WriteValidationError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl std::fmt::Display for WriteValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<WriteValidationError> for ToolError {
    fn from(error: WriteValidationError) -> Self {
        let mut feedback = json!({
            "code": error.code,
            "severity": "error",
            "field": error.field,
            "reason": error.message,
            "action": error.action
        });
        if let Some(values) = error.allowed_values {
            feedback["allowed_values"] = json!(values);
        }
        if let Some((expected, received)) = error.type_mismatch {
            feedback["expected_type"] = json!(expected);
            feedback["received_type"] = json!(received);
        }
        Self::invalid_argument(error.message).with_feedback(feedback)
    }
}
