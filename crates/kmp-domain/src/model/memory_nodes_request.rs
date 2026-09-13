use crate::{DomainError, TraceSearchLimits};

/// Already selected references whose headers and coordinates are needed together.
/// This read never loads canonical bodies or claims to assemble complete proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryNodesRequest {
    pub about: String,
    pub refs: Vec<String>,
    pub max_edges: u32,
    /// Optional identity from a preceding batch; adapters reject changed stores.
    pub expect_snapshot: Option<String>,
}

impl MemoryNodesRequest {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.about.trim().is_empty()
            || self.refs.is_empty()
            || self.refs.len() > 64
            || self
                .refs
                .iter()
                .any(|r| r.trim().is_empty() || r.trim() != r)
            || !(1..=32768).contains(&self.max_edges)
        {
            return Err(DomainError::InvalidState(
                "node batch requires an about, 1..64 nonblank refs without surrounding whitespace and max_edges 1..32768".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn limits(&self) -> TraceSearchLimits {
        TraceSearchLimits {
            nodes: 4096,
            edges: self.max_edges,
            ..Default::default()
        }
    }
}
