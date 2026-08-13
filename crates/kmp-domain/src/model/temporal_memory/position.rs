use std::cmp::Ordering;

use crate::TemporalCoordinate;

use super::axis_key::TemporalAxisKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedTemporalCursor {
    pub(super) axis_key: TemporalAxisKey,
    pub(super) coordinate: TemporalCoordinate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TemporalPosition {
    pub(super) ref_id: String,
    pub(super) kind: String,
    pub(super) text: String,
    pub(super) coordinate: TemporalCoordinate,
    pub(super) axis_key: TemporalAxisKey,
}

impl PartialOrd for TemporalPosition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TemporalPosition {
    fn cmp(&self, other: &Self) -> Ordering {
        self.axis_key
            .cmp(&other.axis_key)
            .then_with(|| {
                self.coordinate
                    .dimension()
                    .cmp(other.coordinate.dimension())
            })
            .then_with(|| self.coordinate.scope_id().cmp(other.coordinate.scope_id()))
            .then_with(|| self.ref_id.cmp(&other.ref_id))
    }
}
