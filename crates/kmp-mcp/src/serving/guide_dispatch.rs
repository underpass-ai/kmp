use super::json_rpc::jsonrpc_result;
use super::telemetry::{ToolErrorKind, record_tool_error, record_tool_success};
use super::tool_result::tool_error_result;
use super::{KernelMcpServer, ToolError, tool_success_result};
use crate::guidance::{AgentDirectory, GuidanceError, GuideRequest, SqliteAgentDirectory};
use serde_json::{Value, json};
use std::time::Instant;

pub(super) fn guidance_error(error: &GuidanceError) -> ToolError {
    match error {
        GuidanceError::InvalidSession(message) => ToolError::invalid_argument(message),
        GuidanceError::StaleGuide => ToolError::conflict(error.to_string()),
        GuidanceError::Unavailable(message) => ToolError::backend(message),
    }
}

impl KernelMcpServer {
    pub(super) fn guidance_directory(
        &self,
        require_existing: bool,
    ) -> Result<&dyn AgentDirectory, ToolError> {
        if self.agent_directory.get().is_none() {
            let directory = match &self.agent_directory_path {
                Some(path) if !require_existing || path.is_file() => SqliteAgentDirectory::at(path),
                None if !require_existing => SqliteAgentDirectory::volatile(),
                _ => {
                    return Err(ToolError::invalid_argument(
                        "no agent context is registered here; open kmp_guide first",
                    ));
                }
            }
            .map_err(|e| guidance_error(&e))?;
            // Cache only successful opens. A transient storage failure must be retryable.
            let _ = self.agent_directory.set(std::sync::Arc::new(directory));
        }
        Ok(self
            .agent_directory
            .get()
            .expect("initialized directory")
            .as_ref())
    }

    pub(super) async fn handle_kmp_guide(
        &self,
        id: Value,
        arguments: &Value,
        start: Instant,
    ) -> String {
        let result = match self.guide_response(arguments).await {
            Ok(response) => {
                let result = tool_success_result(response);
                record_tool_success(
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
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_guide",
                    arguments,
                    ToolErrorKind::Backend,
                    &error.message,
                    start.elapsed(),
                );
                tool_error_result("kmp_guide", arguments, &error)
            }
        };
        jsonrpc_result(id, result)
    }

    async fn guide_node(&self, node_ref: &str) -> Result<Value, ToolError> {
        let response = self
            .backend
            .call_tool(
                "kmp_inspect",
                &json!({
                    "about":"guide:kmp-agent","ref":node_ref,
                    "include":{"details":false,"incoming":false,"outgoing":false,"raw":false},
                    "budget":{"max_bytes":40000}
                }),
            )
            .await?;
        let object = response["structuredContent"]["object"].clone();
        if !object["text"].is_string() {
            return Err(ToolError::backend(
                "guide read did not return its complete canonical object",
            ));
        }
        // The object's body is stable, including on an Inspect floor. We serve
        // the instruction body, not a claim that all adjacent proof was audited.
        Ok(object)
    }

    async fn guide_response(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request = GuideRequest::parse(arguments)?;
        let opening = request.opening()?;
        let overview = self.guide_node("guide:kmp-agent:overview").await?;
        let revision = overview["metadata"]["guide_revision"].as_str().filter(|v| !v.is_empty()).ok_or_else(|| ToolError::conflict("the installed guide has no asset revision; sync guide assets matching this binary"))?;
        let card = if let Some(topic) = &request.topic {
            if request.fold {
                None
            } else {
                let node = self
                    .guide_node(&format!("guide:kmp-agent:card:{topic}"))
                    .await?;
                if node["metadata"]["guide_revision"] != revision {
                    return Err(ToolError::conflict(
                        "guide changed between overview and card; reopen the same guide request",
                    ));
                }
                Some(node)
            }
        } else {
            None
        };
        let directory = self.guidance_directory(false)?;
        let mut context = directory
            .open(&opening, revision)
            .map_err(|e| guidance_error(&e))?;
        let changed = context.guide_changed;
        if let Some(topic) = &request.topic {
            context = if request.fold {
                directory.fold(&context.session, revision, topic)
            } else {
                directory.served(&context.session, revision, topic)
            }
            .map_err(|e| guidance_error(&e))?;
        }
        let next_actions = request.topic.as_ref().filter(|_| !request.fold).map(|topic| vec![json!({
            "tool":"kmp_inspect","arguments":{"context_id":context.session.context_id.as_str(),"about":"guide:kmp-agent","ref":format!("guide:kmp-agent:verb:{topic}"),"include":{"details":false,"incoming":false,"outgoing":false,"raw":false}}
        })]).unwrap_or_default();
        Ok(json!({
            "summary":format!("KMP agent {}. Expand a topic with kmp_guide and this context_id; served records are not proof of understanding.",context.identity.name),
            "agent":{"id":context.identity.id.as_str(),"name":context.identity.name},
            "context_id":context.session.context_id.as_str(),"guide_revision":context.guide_revision,
            "guide_changed":changed,"durable":context.durable,
            "scheme":crate::guidance::scheme(),"expanded":context.expanded,"served":context.served,
            "used":context.used.iter().map(|item| json!({"tool":item.tool,"attempts":item.attempts,"rejected":item.rejected,"unknown":item.unknown})).collect::<Vec<_>>(),
            "card":card.map(|node| json!({"ref":node["ref"],"text":node["text"]})),
            "next_actions":next_actions,
        }))
    }
}
