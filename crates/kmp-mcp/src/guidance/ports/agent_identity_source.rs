use crate::guidance::{AgentContextId, AgentIdentity, GuidanceError};

pub(crate) trait AgentIdentitySource: Send + Sync {
    fn agent(&self) -> Result<AgentIdentity, GuidanceError>;
    fn context(&self) -> Result<AgentContextId, GuidanceError>;
}
