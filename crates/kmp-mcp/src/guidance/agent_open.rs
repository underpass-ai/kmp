use super::{AgentContextId, AgentId};

pub(crate) enum AgentOpen {
    Register { key: String },
    Resume { context: AgentContextId },
    NewContext { agent: AgentId, key: String },
}
