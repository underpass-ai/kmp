use crate::{
    RelationExplanation, TemporalAxis, TemporalCoordinate, TemporalSelection,
    compare_temporal_instants, temporal_instant_nanos,
};
use std::cmp::Ordering;

/// The same clock/interval admission for catalogue and incremental graph reads.
/// A ref cursor must first be resolved from canonical entry coordinates.
pub struct TemporalReadWindow<'a> {
    selection: &'a TemporalSelection,
    resolved_as_of: Option<&'a str>,
    boundary: Option<(i128, bool)>,
}

impl<'a> TemporalReadWindow<'a> {
    pub fn new(selection: &'a TemporalSelection, resolved_as_of: Option<&'a str>) -> Self {
        let boundary = match selection {
            TemporalSelection::Frontier => None,
            TemporalSelection::AsOf { .. } => resolved_as_of
                .and_then(temporal_instant_nanos)
                .map(|t| (t, true)),
            TemporalSelection::Within { interval, .. } => interval
                .end()
                .and_then(temporal_instant_nanos)
                .map(|t| (t, false)),
        };
        Self {
            selection,
            resolved_as_of,
            boundary,
        }
    }

    pub(crate) fn is_frontier(&self) -> bool {
        self.selection.is_frontier()
    }

    pub fn admits_coordinate(&self, coordinate: &TemporalCoordinate) -> bool {
        let axis = self.selection.axis().unwrap_or_default();
        match self.selection {
            TemporalSelection::Frontier => true,
            TemporalSelection::AsOf { .. } => {
                let Some(at) = self.resolved_as_of else {
                    return false;
                };
                if axis == TemporalAxis::Validity {
                    if coordinate.valid_from().is_none() && coordinate.valid_until().is_none() {
                        return false;
                    }
                    return coordinate.valid_from().is_none_or(|from| {
                        matches!(
                            compare_temporal_instants(from, at),
                            Some(Ordering::Less | Ordering::Equal)
                        )
                    }) && coordinate.valid_until().is_none_or(|until| {
                        compare_temporal_instants(at, until) == Some(Ordering::Less)
                    });
                }
                temporal_clock_instant(coordinate, axis).is_some_and(|(instant, _)| {
                    matches!(
                        compare_temporal_instants(instant, at),
                        Some(Ordering::Less | Ordering::Equal)
                    )
                })
            }
            TemporalSelection::Within { interval, .. } => {
                if axis == TemporalAxis::Validity {
                    if coordinate.valid_from().is_none() && coordinate.valid_until().is_none() {
                        return false;
                    }
                    return interval.overlaps(coordinate.valid_from(), coordinate.valid_until());
                }
                temporal_clock_instant(coordinate, axis)
                    .is_some_and(|(instant, _)| interval.contains(instant))
            }
        }
    }

    pub fn admits_relation_clock(&self, explanation: &RelationExplanation) -> bool {
        let Some((end, inclusive)) = self.boundary else {
            return true;
        };
        self.relation_instant(explanation)
            .is_none_or(|at| at < end || (inclusive && at == end))
    }

    pub fn relation_clock_known(&self, explanation: &RelationExplanation) -> bool {
        self.relation_instant(explanation).is_some()
    }

    fn relation_instant(&self, explanation: &RelationExplanation) -> Option<i128> {
        let at = match self.selection.axis().unwrap_or_default() {
            TemporalAxis::Occurred => explanation.occurred_at(),
            TemporalAxis::Observed => explanation.observed_at(),
            TemporalAxis::Ingested => explanation.ingested_at(),
            TemporalAxis::Validity => explanation.valid_from(),
            TemporalAxis::Default => explanation
                .occurred_at()
                .or(explanation.valid_from())
                .or(explanation.observed_at())
                .or(explanation.ingested_at()),
        };
        at.and_then(temporal_instant_nanos)
    }

    pub fn admits_dependency_relation(&self, explanation: &RelationExplanation) -> bool {
        if !self.admits_relation_clock(explanation) {
            return false;
        }
        if self.selection.axis() != Some(TemporalAxis::Validity) {
            return true;
        }
        let start = match self.selection {
            TemporalSelection::AsOf { .. } => self.resolved_as_of,
            TemporalSelection::Within { interval, .. } => interval.start(),
            TemporalSelection::Frontier => None,
        };
        match (explanation.valid_until(), start) {
            (Some(end), Some(start)) => {
                compare_temporal_instants(end, start) == Some(Ordering::Greater)
            }
            _ => true,
        }
    }
}

/// Explicit clocks never fall back. Default retains the existing precedence.
pub fn temporal_clock_instant(
    coordinate: &TemporalCoordinate,
    axis: TemporalAxis,
) -> Option<(&str, TemporalAxis)> {
    match axis {
        TemporalAxis::Occurred => coordinate.occurred_at().map(|at| (at, axis)),
        TemporalAxis::Observed => coordinate.observed_at().map(|at| (at, axis)),
        TemporalAxis::Ingested => coordinate.ingested_at().map(|at| (at, axis)),
        TemporalAxis::Validity => coordinate.valid_from().map(|at| (at, axis)),
        TemporalAxis::Default => coordinate
            .occurred_at()
            .map(|at| (at, TemporalAxis::Occurred))
            .or_else(|| {
                coordinate
                    .valid_from()
                    .map(|at| (at, TemporalAxis::Validity))
            })
            .or_else(|| {
                coordinate
                    .observed_at()
                    .map(|at| (at, TemporalAxis::Observed))
            })
            .or_else(|| {
                coordinate
                    .ingested_at()
                    .map(|at| (at, TemporalAxis::Ingested))
            }),
    }
}
