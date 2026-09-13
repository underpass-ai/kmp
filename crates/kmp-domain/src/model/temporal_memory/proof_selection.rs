use crate::{
    TemporalAxis, TemporalCoordinate, TemporalCursor, TemporalDirection, TemporalInterval,
    TemporalSelection, TemporalTraversalResult, compare_temporal_instants,
};

impl TemporalTraversalResult {
    /// Antecedents may predate a history interval. Only its exclusive end, or
    /// Goto's stricter inclusive cursor, bounds their proof.
    pub fn proof_selection(&self) -> TemporalSelection {
        let mut selection = self.interval().and_then(|interval| interval.end()).map_or(
            TemporalSelection::Frontier,
            |end| {
                TemporalSelection::within(
                    TemporalInterval::new(None, Some(end.to_string()))
                        .expect("validated interval end"),
                    self.axis(),
                )
            },
        );
        if self.direction() == TemporalDirection::Goto
            && let Some(instant) = self
                .resolved_cursor()
                .and_then(|cursor| proof_cursor_instant(cursor, self.axis()))
            && self
                .interval()
                .and_then(|interval| interval.end())
                .is_none_or(|end| {
                    compare_temporal_instants(end, instant) == Some(std::cmp::Ordering::Greater)
                })
        {
            selection = TemporalSelection::AsOf {
                cursor: TemporalCursor::Time(instant.to_string()),
                axis: self.axis(),
            };
        }
        selection
    }
}

fn proof_cursor_instant(cursor: &TemporalCoordinate, axis: TemporalAxis) -> Option<&str> {
    match axis {
        TemporalAxis::Validity => cursor.valid_from().or(cursor.valid_until()),
        TemporalAxis::Occurred => cursor.occurred_at(),
        TemporalAxis::Observed => cursor.observed_at(),
        TemporalAxis::Ingested => cursor.ingested_at(),
        TemporalAxis::Default => cursor
            .occurred_at()
            .or(cursor.observed_at())
            .or(cursor.ingested_at())
            .or(cursor.valid_from()),
    }
}
