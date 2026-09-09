use super::{AgentContextId, AgentId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentSession {
    pub(crate) agent_id: AgentId,
    pub(crate) context_id: AgentContextId,
}
