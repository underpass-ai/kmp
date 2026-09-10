/// Observed calls in one context and guide revision, never a learning score.
#[derive(Debug)]
pub(crate) struct AgentUse {
    pub(crate) tool: String,
    pub(crate) attempts: u64,
    pub(crate) rejected: u64,
    pub(crate) unknown: u64,
}
