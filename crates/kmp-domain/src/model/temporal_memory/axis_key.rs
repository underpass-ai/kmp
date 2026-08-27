use std::cmp::Ordering;

use crate::{TemporalAxis, TemporalCoordinate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TemporalKeyKind {
    Time,
    Sequence,
    Rank,
    Ref,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TemporalAxisKey {
    axis: TemporalKeyKind,
    value: String,
}

impl TemporalAxisKey {
    pub(super) fn time(value: &str) -> Self {
        Self {
            axis: TemporalKeyKind::Time,
            value: value.to_string(),
        }
    }

    pub(super) fn sequence(value: u32) -> Self {
        Self {
            axis: TemporalKeyKind::Sequence,
            value: format!("{value:010}"),
        }
    }

    fn rank(value: u32) -> Self {
        Self {
            axis: TemporalKeyKind::Rank,
            value: format!("{value:010}"),
        }
    }

    fn ref_id(value: &str) -> Self {
        Self {
            axis: TemporalKeyKind::Ref,
            value: value.to_string(),
        }
    }

    pub(super) fn axis(&self) -> TemporalKeyKind {
        self.axis
    }

    pub(super) fn from_coordinate(
        ref_id: &str,
        coordinate: &TemporalCoordinate,
        requested_axis: TemporalAxis,
    ) -> Vec<Self> {
        let mut keys = Vec::new();
        let selected_time = match requested_axis {
            TemporalAxis::Default => coordinate
                .occurred_at()
                .or(coordinate.valid_from())
                .or(coordinate.observed_at())
                .or(coordinate.ingested_at()),
            TemporalAxis::Occurred => coordinate.occurred_at(),
            TemporalAxis::Observed => coordinate.observed_at(),
            TemporalAxis::Ingested => coordinate.ingested_at(),
            TemporalAxis::Validity => coordinate.valid_from().or(coordinate.valid_until()),
        };
        if let Some(value) = selected_time {
            keys.push(Self::time(value));
        }
        if let Some(value) = coordinate.sequence() {
            keys.push(Self::sequence(value));
        }
        if let Some(value) = coordinate.rank() {
            keys.push(Self::rank(value));
        }
        if keys.is_empty() {
            keys.push(Self::ref_id(ref_id));
        }

        keys
    }
}

impl PartialOrd for TemporalAxisKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TemporalAxisKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.axis
            .cmp(&other.axis)
            .then_with(|| self.value.cmp(&other.value))
    }
}

pub(super) fn primary_coordinate_key(coordinate: &TemporalCoordinate) -> TemporalAxisKey {
    TemporalAxisKey::from_coordinate("", coordinate, TemporalAxis::Default)
        .into_iter()
        .next()
        .expect("coordinate key should always exist")
}
