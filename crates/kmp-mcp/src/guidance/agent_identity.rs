use super::AgentId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentIdentity {
    pub(crate) id: AgentId,
    pub(crate) name: String,
}
