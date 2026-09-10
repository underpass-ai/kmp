use crate::guidance::{
    AgentContextId, AgentId, AgentIdentity, AgentIdentitySource, GuidanceError, ReadContinuationId,
};

pub(super) struct RandomAgentIdentity;

impl RandomAgentIdentity {
    pub(super) fn read_continuation() -> Result<ReadContinuationId, GuidanceError> {
        ReadContinuationId::parse(&format!("read_{}", Self::random_hex()?))
    }
    fn random_hex() -> Result<String, GuidanceError> {
        let mut bytes = [0_u8; 16];
        getrandom::getrandom(&mut bytes).map_err(|error| {
            GuidanceError::Unavailable(format!("agent identity entropy: {error}"))
        })?;
        Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }
}

impl AgentIdentitySource for RandomAgentIdentity {
    fn agent(&self) -> Result<AgentIdentity, GuidanceError> {
        let id = AgentId::parse(&format!("agent_{}", Self::random_hex()?))?;
        // Separate random draws: the display name is not a second identifier.
        let name = format!("traveler-{}", &Self::random_hex()?[..12]);
        Ok(AgentIdentity { id, name })
    }

    fn context(&self) -> Result<AgentContextId, GuidanceError> {
        AgentContextId::parse(&format!("context_{}", Self::random_hex()?))
    }
}
