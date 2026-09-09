use serde_json::{Value, json};

use crate::serving::ToolError;

/// A writer refusal, located where the rule is checked, never by parsing prose.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WriteValidationError {
    pub(crate) message: String,
    code: &'static str,
    field: String,
    global: bool,
    action: Option<Value>,
}

impl WriteValidationError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: "INVALID_WRITE",
            field: String::new(),
            global: false,
            action: None,
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

    pub(crate) fn action(mut self, tool: &str, arguments: Value) -> Self {
        self.action = Some(json!({"tool": tool, "arguments": arguments}));
        self
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
        let feedback = json!({
            "code": error.code,
            "severity": "error",
            "field": error.field,
            "reason": error.message,
            "action": error.action
        });
        Self::invalid_argument(error.message).with_feedback(feedback)
    }
}
