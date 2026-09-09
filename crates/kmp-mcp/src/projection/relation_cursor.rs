//! Stateless MCP continuation bound to both the query and the backend selection.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::serving::ToolError;

const VERSION: &str = "kmpr1";

#[derive(Clone)]
pub(crate) struct RelationCursor {
    arguments: Value,
    tool: &'static str,
    hash: String,
    pub(crate) offset: usize,
}

impl RelationCursor {
    /// Only the validated position goes to the kernel's positional page API.
    /// The MCP boundary checks its content digest before serving that page.
    pub(crate) fn kernel_position(value: &str) -> Result<String, String> {
        Self::parse(value).map(|(offset, _)| offset.to_string())
    }

    pub(crate) fn read(
        tool: &'static str,
        fingerprint: &str,
        arguments: &Value,
        total: usize,
    ) -> Result<Self, ToolError> {
        if fingerprint.is_empty() {
            return Err(ToolError::backend(
                "backend lacks the required read selection fingerprint; update the kernel",
            ));
        }
        let mut bound = arguments.clone();
        let object = bound.as_object_mut().expect("validated arguments");
        object.remove("page");
        if let Some(budget) = object.get_mut("budget").and_then(Value::as_object_mut) {
            budget.remove("max_bytes");
            if budget.is_empty() {
                object.remove("budget");
            }
        }
        let hash = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&json!([VERSION, tool, bound, fingerprint]))
                    .expect("cursor selection serializes")
            )
        );
        let cursor = arguments
            .pointer("/page/cursor")
            .and_then(Value::as_str)
            .map(Self::parse)
            .transpose()
            .map_err(ToolError::invalid_argument)?;
        let result = Self {
            arguments: arguments.clone(),
            tool,
            hash,
            offset: cursor.map_or(0, |(offset, _)| offset),
        };
        if let Some((offset, expected)) = cursor {
            if expected != result.hash {
                let mut restart = arguments.clone();
                if let Some(page) = restart.get_mut("page").and_then(Value::as_object_mut) {
                    page.remove("cursor");
                    if page.is_empty() {
                        restart.as_object_mut().expect("arguments").remove("page");
                    }
                }
                return Err(ToolError::conflict(
                    "read cursor selection or content changed; restart the read",
                )
                .with_feedback(
                    json!({"code":"READ_SELECTION_CHANGED","field":"page.cursor",
                        "action":{"tool":tool,"arguments":restart}}),
                ));
            }
            if offset >= total {
                return Err(ToolError::invalid_argument(
                    "read page.cursor is exhausted or out of range",
                ));
            }
        }
        Ok(result)
    }

    pub(crate) fn token(&self, offset: usize) -> String {
        format!("{VERSION}:{offset}:{}", self.hash)
    }

    pub(crate) fn action(&self, offset: usize) -> Value {
        let mut arguments = self.arguments.clone();
        arguments["page"]["cursor"] = json!(self.token(offset));
        json!({"tool":self.tool,"arguments":arguments})
    }

    pub(crate) fn with_budget(&self, bytes: usize) -> Self {
        let mut result = self.clone();
        result.arguments["budget"]["max_bytes"] = json!(bytes);
        result
    }

    fn parse(cursor: &str) -> Result<(usize, &str), String> {
        let mut parts = cursor.split(':');
        let version = parts.next();
        let offset = parts.next().and_then(|s| s.parse::<usize>().ok());
        let hash = parts.next();
        if version != Some(VERSION)
            || offset.is_none()
            || parts.next().is_some()
            || !hash.is_some_and(|h| h.len() == 64 && h.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(
                "malformed read page.cursor; copy the opaque cursor returned by this tool"
                    .to_string(),
            );
        }
        Ok((offset.expect("checked offset"), hash.expect("checked hash")))
    }
}
