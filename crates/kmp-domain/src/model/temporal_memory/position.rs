use std::cmp::Ordering;

use crate::TemporalCoordinate;

use super::axis_key::TemporalAxisKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedTemporalCursor {
    pub(super) axis_key: Option<TemporalAxisKey>,
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
            // Sequence and rank are scoped coordinates, and legacy writers
            // may leave many entries tied. Their recorded clock is the
            // temporal tiebreak; lexical refs are only the final stable key.
            .then_with(|| {
                super::axis_key::primary_coordinate_key(&self.coordinate)
                    .cmp(&super::axis_key::primary_coordinate_key(&other.coordinate))
            })
            .then_with(|| {
                self.coordinate
                    .dimension()
                    .cmp(other.coordinate.dimension())
            })
            .then_with(|| self.coordinate.scope_id().cmp(other.coordinate.scope_id()))
            .then_with(|| self.coordinate.sequence().cmp(&other.coordinate.sequence()))
            .then_with(|| self.coordinate.rank().cmp(&other.coordinate.rank()))
            .then_with(|| self.ref_id.cmp(&other.ref_id))
    }
}
