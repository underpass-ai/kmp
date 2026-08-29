use serde_json::{Value, json};

use crate::guide::domain::guide_error::GuideError;

#[derive(Debug, Clone, PartialEq)]
pub struct GuideRequestDocumentDto {
    body: Value,
    about: String,
}

impl GuideRequestDocumentDto {
    pub fn parse(body: Value) -> Result<Self, GuideError> {
        let about = body
            .get("about")
            .and_then(Value::as_str)
            .filter(|about| matches!(*about, "guide:kmp" | "guide:kmp-agent"))
            .ok_or_else(|| GuideError::invalid("guide request has an unsupported about"))?
            .to_string();
        if body
            .get("idempotency_key")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(GuideError::invalid("guide request has no idempotency key"));
        }
        if !body.get("memory").is_some_and(Value::is_object) {
            return Err(GuideError::invalid("guide request has no memory document"));
        }
        Ok(Self { body, about })
    }

    pub fn about(&self) -> &str {
        &self.about
    }

    pub fn mcp_call(&self, identifier: u64) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": identifier,
            "method": "tools/call",
            "params": {"name": "kmp_ingest", "arguments": self.body}
        })
    }
}
