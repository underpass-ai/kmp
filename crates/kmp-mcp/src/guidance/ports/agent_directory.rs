use crate::guidance::{
    AgentContext, AgentContextId, AgentOpen, AgentSession, AgentUse, GuidanceError,
    ReadContinuation, ReadContinuationId, UseOutcome,
};

pub(crate) trait AgentDirectory: Send + Sync {
    fn save_read(
        &self,
        session: &AgentSession,
        call: &ReadContinuation,
    ) -> Result<ReadContinuationId, GuidanceError>;
    fn load_read(&self, id: &ReadContinuationId)
    -> Result<Option<ReadContinuation>, GuidanceError>;
    /// Retain a continuation proposed without a guidance context (#544 C3):
    /// the page hands back a short handle instead of restating the call.
    /// Bounded, expiring transport state; the handle names a call, it grants
    /// nothing, and every transport authorizes the resolved request.
    fn save_open(&self, call: &ReadContinuation) -> Result<ReadContinuationId, GuidanceError>;
    fn load_open(&self, id: &ReadContinuationId)
    -> Result<Option<ReadContinuation>, GuidanceError>;
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
