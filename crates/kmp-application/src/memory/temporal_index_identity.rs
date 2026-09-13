use kmp_domain::{GraphReadRevision, TemporalAxis};

/// Inputs to the immutable catalogue, before temporal and label admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TemporalIndexIdentity {
    revision: GraphReadRevision,
    ordered_roots: Vec<String>,
    depth: u32,
    axis: TemporalAxis,
}

impl TemporalIndexIdentity {
    pub(super) fn new(
        revision: GraphReadRevision,
        ordered_roots: Vec<String>,
        depth: u32,
        axis: TemporalAxis,
    ) -> Self {
        Self {
            revision,
            ordered_roots,
            depth,
            axis,
        }
    }
}
