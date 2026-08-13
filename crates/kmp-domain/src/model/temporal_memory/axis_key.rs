use std::cmp::Ordering;

use crate::TemporalCoordinate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TemporalAxis {
    Time,
    Sequence,
    Rank,
    Ref,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TemporalAxisKey {
    axis: TemporalAxis,
    value: String,
}

impl TemporalAxisKey {
    pub(super) fn time(value: &str) -> Self {
        Self {
            axis: TemporalAxis::Time,
            value: value.to_string(),
        }
    }

    pub(super) fn sequence(value: u32) -> Self {
        Self {
            axis: TemporalAxis::Sequence,
            value: format!("{value:010}"),
        }
    }

    fn rank(value: u32) -> Self {
        Self {
            axis: TemporalAxis::Rank,
            value: format!("{value:010}"),
        }
    }

    fn ref_id(value: &str) -> Self {
        Self {
            axis: TemporalAxis::Ref,
            value: value.to_string(),
        }
    }

    pub(super) fn axis(&self) -> TemporalAxis {
        self.axis
    }

    pub(super) fn from_coordinate(ref_id: &str, coordinate: &TemporalCoordinate) -> Vec<Self> {
        let mut keys = Vec::new();
        if let Some(value) = coordinate
            .occurred_at()
            .or(coordinate.valid_from())
            .or(coordinate.observed_at())
            .or(coordinate.ingested_at())
        {
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
    TemporalAxisKey::from_coordinate("", coordinate)
        .into_iter()
        .next()
        .expect("coordinate key should always exist")
}
