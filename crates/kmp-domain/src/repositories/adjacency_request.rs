use std::num::NonZeroU32;

use crate::{DomainError, RelationDirection, RelationPosition};

/// A strict row bound for one directed adjacency read. No full-scan fallback.
/// Scope, clock admission and the complete traversal's budget belong to its
/// caller; this storage request neither authorizes nor certifies those reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdjacencyRequest {
    node_id: String,
    direction: RelationDirection,
    limit: NonZeroU32,
    after: Option<RelationPosition>,
}

impl AdjacencyRequest {
    pub fn new(
        node_id: impl Into<String>,
        direction: RelationDirection,
        limit: u32,
    ) -> Result<Self, DomainError> {
        let node_id = node_id.into();
        if node_id.trim().is_empty() {
            return Err(DomainError::EmptyValue("node_id"));
        }
        let limit = NonZeroU32::new(limit).ok_or_else(|| {
            DomainError::InvalidState("adjacency row limit must be positive".into())
        })?;
        Ok(Self {
            node_id,
            direction,
            limit,
            after: None,
        })
    }

    pub fn with_after(mut self, position: RelationPosition) -> Self {
        self.after = Some(position);
        self
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }
    pub fn direction(&self) -> RelationDirection {
        self.direction
    }
    pub fn limit(&self) -> u32 {
        self.limit.get()
    }
    pub fn after(&self) -> Option<&RelationPosition> {
        self.after.as_ref()
    }
}
