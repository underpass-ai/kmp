use super::{AgentIdentity, AgentSession, AgentUse};

#[derive(Debug)]
pub(crate) struct AgentContext {
    pub(crate) identity: AgentIdentity,
    pub(crate) session: AgentSession,
    pub(crate) guide_revision: String,
    pub(crate) expanded: Vec<String>,
    pub(crate) served: Vec<String>,
    pub(crate) guide_changed: bool,
    pub(crate) durable: bool,
    pub(crate) used: Vec<AgentUse>,
}
