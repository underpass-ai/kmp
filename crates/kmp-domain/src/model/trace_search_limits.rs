use crate::DomainError;

/// Work limits, independent of transport pagination and rendering tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceSearchLimits {
    pub nodes: u32,
    pub edges: u32,
    pub depth: u32,
    pub states: u32,
}

impl TraceSearchLimits {
    pub fn validate(self) -> Result<(), DomainError> {
        if !(1..=4096).contains(&self.nodes)
            || !(1..=32768).contains(&self.edges)
            || !(1..=1024).contains(&self.depth)
            || !(1..=32768).contains(&self.states)
        {
            return Err(DomainError::InvalidState(
                "trace search requires max_nodes 1..4096, max_edges 1..32768, max_depth 1..1024 and max_states 1..32768"
                    .into(),
            ));
        }
        Ok(())
    }
}

impl Default for TraceSearchLimits {
    fn default() -> Self {
        Self {
            nodes: 256,
            edges: 2048,
            depth: 128,
            states: 4096,
        }
    }
}
