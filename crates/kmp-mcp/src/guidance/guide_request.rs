use super::{AgentContextId, AgentId, AgentOpen};
use crate::serving::ToolError;
use serde::Deserialize;
use serde_json::Value;

/// JSON exists only at this boundary. Identity transitions use typed values.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuideRequest {
    registration_key: Option<String>,
    context_id: Option<String>,
    agent_id: Option<String>,
    context_key: Option<String>,
    pub(crate) topic: Option<String>,
    #[serde(default)]
    pub(crate) fold: bool,
}

impl GuideRequest {
    pub(crate) fn parse(arguments: &Value) -> Result<Self, ToolError> {
        let request: Self = serde_json::from_value(arguments.clone())
            .map_err(|e| ToolError::invalid_argument(format!("invalid guide arguments: {e}")))?;
        if request.fold && request.topic.is_none() {
            return Err(ToolError::invalid_argument(
                "fold requires the topic to hide",
            ));
        }
        if request.topic.as_deref().is_some_and(|topic| {
            !super::guide_scheme::TOPICS
                .iter()
                .any(|(key, _)| *key == topic)
        }) {
            return Err(ToolError::invalid_argument(
                "topic must name one row of the KMP scheme",
            ));
        }
        Ok(request)
    }

    pub(crate) fn opening(&self) -> Result<AgentOpen, ToolError> {
        match (
            &self.registration_key,
            &self.context_id,
            &self.agent_id,
            &self.context_key,
        ) {
            (Some(key), None, None, None) if !key.trim().is_empty() => {
                Ok(AgentOpen::Register { key: key.clone() })
            }
            (None, Some(context), None, None) => Ok(AgentOpen::Resume {
                context: AgentContextId::parse(context)
                    .map_err(|e| ToolError::invalid_argument(e.to_string()))?,
            }),
            (None, None, Some(agent), Some(key)) if !key.trim().is_empty() => {
                Ok(AgentOpen::NewContext {
                    agent: AgentId::parse(agent)
                        .map_err(|e| ToolError::invalid_argument(e.to_string()))?,
                    key: key.clone(),
                })
            }
            _ => Err(ToolError::invalid_argument(
                "use registration_key for one new agent, context_id to resume, or agent_id + context_key after losing context; do not combine these forms",
            )),
        }
    }
}
