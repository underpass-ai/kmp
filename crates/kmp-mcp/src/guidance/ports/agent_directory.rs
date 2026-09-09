use crate::guidance::{AgentContext, AgentOpen, AgentSession, GuidanceError};

pub(crate) trait AgentDirectory: Send + Sync {
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
