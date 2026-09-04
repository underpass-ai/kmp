use crate::{DomainError, TemporalAxis, TemporalCursor, TemporalInterval};

/// Which instants a recall stands on.
///
/// Wake and Ask read the memory's own frontier unless the caller names an
/// instant or a span. Naming one changes two things at once: which entries
/// compete, and when the lifecycles — supersession, expiry — are read, so an
/// entry replaced after the instant is current for that question.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TemporalSelection {
    /// The latest instant the memory knows about: today's behaviour.
    #[default]
    Frontier,
    /// What was in effect at one instant on one clock.
    AsOf {
        cursor: TemporalCursor,
        axis: TemporalAxis,
    },
    /// What fell within a half-open span on one clock.
    Within {
        interval: TemporalInterval,
        axis: TemporalAxis,
    },
}

impl TemporalSelection {
    /// A ref stands at that entry's own instant on the axis; a time stands
    /// where it says. A sequence is relative to one dimension and names no
    /// instant, so it is refused.
    pub fn as_of(cursor: TemporalCursor, axis: TemporalAxis) -> Result<Self, DomainError> {
        if matches!(cursor, TemporalCursor::Sequence(_)) {
            return Err(DomainError::InvalidState(
                "as_of takes a ref or a time; a sequence is relative to one dimension and \
                 names no instant"
                    .to_string(),
            ));
        }
        Ok(Self::AsOf { cursor, axis })
    }

    pub fn within(interval: TemporalInterval, axis: TemporalAxis) -> Self {
        Self::Within { interval, axis }
    }

    pub fn is_frontier(&self) -> bool {
        matches!(self, Self::Frontier)
    }

    /// The clock the selection reads, when there is one.
    pub fn axis(&self) -> Option<TemporalAxis> {
        match self {
            Self::Frontier => None,
            Self::AsOf { axis, .. } | Self::Within { axis, .. } => Some(*axis),
        }
    }

    pub fn interval(&self) -> Option<&TemporalInterval> {
        match self {
            Self::Within { interval, .. } => Some(interval),
            _ => None,
        }
    }

    pub fn cursor(&self) -> Option<&TemporalCursor> {
        match self {
            Self::AsOf { cursor, .. } => Some(cursor),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sequence_cursor_names_no_instant_and_is_refused() {
        let error = TemporalSelection::as_of(
            TemporalCursor::sequence(3).expect("a sequence"),
            TemporalAxis::Default,
        )
        .expect_err("a sequence is relative to one dimension");
        assert!(error.to_string().contains("names no instant"), "{error}");
        assert!(
            TemporalSelection::as_of(
                TemporalCursor::time("2026-03-01T00:00:00Z").expect("a time"),
                TemporalAxis::Observed
            )
            .is_ok()
        );
        assert!(
            TemporalSelection::as_of(
                TemporalCursor::ref_id("about:x:entry:e1").expect("a ref"),
                TemporalAxis::Default
            )
            .is_ok()
        );
    }

    #[test]
    fn the_frontier_is_the_default_and_reads_no_axis() {
        let selection = TemporalSelection::default();
        assert!(selection.is_frontier());
        assert_eq!(selection.axis(), None);
        assert!(selection.interval().is_none());
        let within = TemporalSelection::within(
            TemporalInterval::new(Some("2026-03-01T00:00:00Z".to_string()), None)
                .expect("since March"),
            TemporalAxis::Ingested,
        );
        assert_eq!(within.axis(), Some(TemporalAxis::Ingested));
        assert!(within.interval().is_some());
    }
}
