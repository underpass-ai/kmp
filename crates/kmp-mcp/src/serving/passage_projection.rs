//! Optional host representation after native selection, budgeting and guidance.
use serde_json::{Value, json};

use super::KernelMcpServer;
use crate::guidance::ReadContinuation;

impl KernelMcpServer {
    /// Opt in at the host boundary. The default native contract stays inline;
    /// this setting adds no verb, call argument, store write or retained page.
    pub fn with_shared_passages(mut self, enabled: bool) -> Self {
        self.shared_passages = enabled;
        self
    }

    /// Compose the same mode in stdio (with or without viewer) and HTTP startup.
    pub fn with_passage_format_from_env(self) -> Result<Self, String> {
        let enabled = match std::env::var("KMP_MCP_PASSAGES").as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("inline") => false,
            Ok("shared") => true,
            _ => return Err("KMP_MCP_PASSAGES must be inline or shared".into()),
        };
        Ok(self.with_shared_passages(enabled))
    }

    pub(super) fn passage_initialize(&self, mut result: Value) -> Value {
        if self.shared_passages {
            let instructions = result["instructions"].as_str().unwrap_or_default();
            result["instructions"] = json!(format!(
                "{instructions}\nThis host enables shared passages: in memory read results a prose slot {{\"passage\":\"p1\"}} means passages.p1 in that same response. A graph reference slot {{\"citation\":\"c1\"}} resolves to citations.c1, the exact canonical ref. Record definitions and source records remain distinct; text sharing never asserts identity. Keep each page's table with its packet."
            ));
        }
        result
    }

    pub(super) fn passage_tools(&self, mut result: Value) -> Value {
        if self.shared_passages {
            for tool in result["tools"].as_array_mut().into_iter().flatten() {
                if ReadContinuation::supports(tool["name"].as_str().unwrap_or_default()) {
                    tool["outputSchema"]["properties"]["citations"] = json!({"type":"object","additionalProperties":{"type":"string"},"description":"Response-local mapping from citation handles to exact canonical refs. A graph-reference slot {citation:id} resolves here; source/record identities and original read actions remain explicit."});
                    tool["outputSchema"]["properties"]["passages"] = json!({
                        "type":"object", "additionalProperties":{"type":"string"},
                        "description":"Optional response-local text table. A text, why or evidence slot may contain {passage:id}; resolve it here. Refs, sources, support and clocks remain on the original records. Absent when sharing would not save bytes."
                    });
                }
            }
        }
        result
    }

    pub(super) fn project_read_passages(&self, tool: &str, response: String) -> String {
        if !self.shared_passages || !ReadContinuation::supports(tool) {
            return response;
        }
        let Ok(mut envelope) = serde_json::from_str::<Value>(&response) else {
            return response;
        };
        if envelope["result"]["isError"] == true {
            return response;
        }
        let Some(body) = envelope.pointer_mut("/result/structuredContent") else {
            return response;
        };
        let shared = kmp_proto_mapping::context_projection::share_packet(body.clone());
        if shared.get("passages").is_none() && shared.get("citations").is_none() {
            return response;
        }
        *body = shared;
        // Native floors and required_bytes describe the inline selection, as
        // with short continuations. Only used_bytes reports the smaller packet.
        if body.pointer("/projection/budget/used_bytes").is_some() {
            loop {
                let bytes = body.to_string().len();
                if body["projection"]["budget"]["used_bytes"] == bytes {
                    break;
                }
                body["projection"]["budget"]["used_bytes"] = json!(bytes);
            }
        }
        envelope.to_string()
    }
}
