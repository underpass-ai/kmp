use crate::guidance::{
    AgentContext, AgentContextId, AgentOpen, AgentSession, AgentUse, GuidanceError, UseOutcome,
};

pub(crate) trait AgentDirectory: Send + Sync {
    /// Resume the last observed revision without reading or writing memory.
    fn context(&self, id: &AgentContextId) -> Result<AgentContext, GuidanceError>;
    fn record_use(
        &self,
        session: &AgentSession,
        revision: &str,
        tool: &str,
        outcome: UseOutcome,
    ) -> Result<AgentUse, GuidanceError>;
    fn open(&self, request: &AgentOpen, revision: &str) -> Result<AgentContext, GuidanceError>;
    /// Records the body the server served, not an acknowledgement or learning.
    fn served(
        &self,
        session: &AgentSession,
        revision: &str,
        topic: &str,
    ) -> Result<AgentContext, GuidanceError>;
    /// Folding changes the displayed scheme, retaining the delivery history.
    fn fold(
        &self,
        session: &AgentSession,
        revision: &str,
        topic: &str,
    ) -> Result<AgentContext, GuidanceError>;
}
