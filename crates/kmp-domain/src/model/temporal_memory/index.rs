use crate::{DomainError, KmpBundle, TemporalAxis};

use super::extract::temporal_positions;
use super::position::TemporalPosition;
use super::{TemporalMemoryTraversal, TemporalTraversalRequest, TemporalTraversalResult};

/// Immutable catalogue ordered on one requested clock. Its owner binds reuse
/// to the complete graph snapshot; no body participates in this index.
#[derive(Debug)]
pub struct TemporalMemoryIndex {
    bundle: KmpBundle,
    axis: TemporalAxis,
    positions: Vec<TemporalPosition>,
}

impl TemporalMemoryIndex {
    pub fn new(bundle: KmpBundle, axis: TemporalAxis) -> Result<Self, DomainError> {
        let mut positions = temporal_positions(&bundle, axis)?;
        positions.sort();
        Ok(Self {
            bundle,
            axis,
            positions,
        })
    }

    pub fn bundle(&self) -> &KmpBundle {
        &self.bundle
    }

    pub fn traverse(
        &self,
        request: &TemporalTraversalRequest,
    ) -> Result<TemporalTraversalResult, DomainError> {
        if request.axis() != self.axis {
            return Err(DomainError::InvalidState(
                "temporal index clock does not match request".into(),
            ));
        }
        TemporalMemoryTraversal::traverse_index(&self.bundle, request, &self.positions)
    }
}
