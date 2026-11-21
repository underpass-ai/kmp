//! `kmp_guide`: a stateless lookup. Without a topic it lists the topics; with
//! one it serves that topic's card, compiled into the engine, and the argument
//! prose of the tools it teaches. It reads no store and records nothing, so
//! every install answers the same way.

use super::json_rpc::jsonrpc_result;
use super::telemetry::{CallOrigin, ToolErrorKind, record_call_error, record_call_success};
use super::tool_result::tool_error_result;
use super::{KernelMcpServer, ToolError, tool_success_result};
use crate::contract::parameter_guide::topic_parameters;
use crate::contract::schema::relation_vocabulary::relation_vocabulary_lines;
use crate::guide::GuideTopic;
use serde_json::{Value, json};
use std::time::Instant;

impl KernelMcpServer {
    pub(super) async fn handle_kmp_guide(
        &self,
        id: Value,
        arguments: &Value,
        start: Instant,
        origin: &CallOrigin,
    ) -> String {
        let result = match self.guide_response(arguments) {
            Ok(response) => {
                let result = tool_success_result(response);
                record_call_success(
                    self.call_source(origin, false),
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_guide",
                    arguments,
                    &result,
                    start.elapsed(),
                );
                result
            }
            Err(error) => {
                record_call_error(
                    self.call_source(origin, false),
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_guide",
                    arguments,
                    ToolErrorKind::Backend,
                    &error.message,
                    &error.feedback,
                    start.elapsed(),
                );
                tool_error_result("kmp_guide", arguments, &error)
            }
        };
        jsonrpc_result(id, result)
    }

    fn guide_response(&self, arguments: &Value) -> Result<Value, ToolError> {
        let Some(name) = arguments.get("topic") else {
            return Ok(json!({
                "summary":"KMP guide topics. Call kmp_guide with one topic before a verb's first use.",
                "topics":GuideTopic::all().iter()
                    .map(|topic| json!({"topic":topic.name,"purpose":topic.purpose}))
                    .collect::<Vec<_>>(),
            }));
        };
        let topic = name
            .as_str()
            .and_then(GuideTopic::named)
            .ok_or_else(|| ToolError::invalid_argument("topic must name one row of kmp_guide"))?;
        let mut card = json!({
            "summary":format!("Card {}: read text, then parameters for each argument.",topic.name),
            "topic":topic.name,
            "ref":topic.card_ref(),
            "text":topic.text,
            "parameters":topic_parameters(topic),
        });
        let rules = topic_rules(topic);
        if !rules.is_empty() {
            card["rules"] = Value::Object(rules);
        }
        if topic.name == "relations" {
            card["relation_vocabulary"] = json!(relation_vocabulary_lines());
        }
        if topic.name == "guide" {
            card["error_codes"] = guide_error_codes();
        }
        Ok(card)
    }
}

/// The cross-field rules this topic's arguments carry: id → the rule and its
/// fix. A refusal names the same id.
fn topic_rules(topic: &GuideTopic) -> serde_json::Map<String, Value> {
    let mut rules = serde_json::Map::new();
    for rule in crate::contract::cross_field_rule::all() {
        if topic.serves_path(rule.tool, rule.path) && !rules.contains_key(rule.id) {
            rules.insert(
                rule.id.to_string(),
                json!(format!("{} Fix: {}", rule.rule, rule.fix)),
            );
        }
    }
    rules
}

/// The error codes a tool result can carry, each with what to do about it.
pub(crate) fn guide_error_codes() -> Value {
    Value::Object(
        super::ToolErrorCode::ALL
            .iter()
            .map(|code| (code.as_str().to_string(), json!(code.guidance())))
            .collect(),
    )
}
