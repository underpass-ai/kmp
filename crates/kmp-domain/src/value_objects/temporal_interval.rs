use std::cmp::Ordering;

use crate::{DomainError, compare_temporal_instants};

/// A half-open span on one clock: `start` inclusive, `end` exclusive.
///
/// Either bound may be open, so "since March" and "before the release" are
/// intervals too; an interval with neither is refused, because it selects
/// nothing. Instants are kept as the caller wrote them and compared on the
/// kernel's canonical axis, which reads both spellings the store uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalInterval {
    start: Option<String>,
    end: Option<String>,
}

impl TemporalInterval {
    pub fn new(start: Option<String>, end: Option<String>) -> Result<Self, DomainError> {
        let start = normalize(start);
        let end = normalize(end);
        if start.is_none() && end.is_none() {
            return Err(DomainError::InvalidState(
                "temporal interval needs a start or an end".to_string(),
            ));
        }
        for (name, bound) in [("start", &start), ("end", &end)] {
            if let Some(bound) = bound
                && compare_temporal_instants(bound, bound).is_none()
            {
                return Err(DomainError::InvalidState(format!(
                    "temporal interval {name} `{bound}` is not an instant the kernel can read"
                )));
            }
        }
        if let (Some(start), Some(end)) = (&start, &end)
            && compare_temporal_instants(start, end) != Some(Ordering::Less)
        {
            return Err(DomainError::InvalidState(
                "temporal interval must start before it ends".to_string(),
            ));
        }
        Ok(Self { start, end })
    }

    pub fn start(&self) -> Option<&str> {
        self.start.as_deref()
    }

    pub fn end(&self) -> Option<&str> {
        self.end.as_deref()
    }

    /// Whether an instant falls inside: `start <= instant < end`, with an
    /// open bound admitting everything on its side. An instant the kernel
    /// cannot read falls outside.
    pub fn contains(&self, instant: &str) -> bool {
        let after_start = self.start.as_deref().is_none_or(|start| {
            matches!(
                compare_temporal_instants(start, instant),
                Some(Ordering::Less | Ordering::Equal)
            )
        });
        let before_end = self
            .end
            .as_deref()
            .is_none_or(|end| compare_temporal_instants(instant, end) == Some(Ordering::Less));
        after_start && before_end
    }

    /// Whether a validity span `[valid_from, valid_until)` overlaps this
    /// interval: it started before this ends and ended after this starts,
    /// with an open bound on either side counting as overlap on that side.
    pub fn overlaps(&self, valid_from: Option<&str>, valid_until: Option<&str>) -> bool {
        let starts_before_end = match (valid_from, self.end.as_deref()) {
            (Some(from), Some(end)) => compare_temporal_instants(from, end) == Some(Ordering::Less),
            _ => true,
        };
        let ends_after_start = match (valid_until, self.start.as_deref()) {
            (Some(until), Some(start)) => {
                compare_temporal_instants(until, start) == Some(Ordering::Greater)
            }
            _ => true,
        };
        starts_before_end && ends_after_start
    }

    /// How far an instant lies outside, in nanoseconds: zero inside, and
    /// `None` for an instant the kernel cannot read.
    pub fn distance_outside(&self, instant: &str) -> Option<i128> {
        let nanos = crate::temporal_instant_nanos;
        let at = nanos(instant)?;
        if let Some(start) = self.start.as_deref()
            && let Some(start) = nanos(start)
            && at < start
        {
            return Some(start - at);
        }
        if let Some(end) = self.end.as_deref()
            && let Some(end) = nanos(end)
            && at >= end
        {
            return Some(at - end + 1);
        }
        Some(0)
    }
}

fn normalize(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_open_interval_admits_its_start_and_refuses_its_end() {
        let interval = TemporalInterval::new(
            Some("2026-03-01T00:00:00Z".to_string()),
            Some("2026-04-01T00:00:00Z".to_string()),
        )
        .expect("a span");
        assert!(interval.contains("2026-03-01T00:00:00Z"));
        assert!(interval.contains("2026-03-15T12:00:00Z"));
        assert!(!interval.contains("2026-04-01T00:00:00Z"));
        assert!(!interval.contains("2026-02-28T23:59:59Z"));
        assert!(!interval.contains("not an instant"));
    }

    #[test]
    fn an_open_side_admits_everything_on_that_side() {
        let since = TemporalInterval::new(Some("2026-03-01T00:00:00Z".to_string()), None)
            .expect("since March");
        assert!(since.contains("2030-01-01T00:00:00Z"));
        assert!(!since.contains("2026-02-01T00:00:00Z"));
        let before = TemporalInterval::new(None, Some("2026-03-01T00:00:00Z".to_string()))
            .expect("before March");
        assert!(before.contains("2020-01-01T00:00:00Z"));
        assert!(!before.contains("2026-03-01T00:00:00Z"));
    }

    #[test]
    fn both_spellings_of_an_instant_compare_on_one_axis() {
        let interval = TemporalInterval::new(
            Some("2026-03-01T00:00:00Z".to_string()),
            Some("2026-04-01T00:00:00Z".to_string()),
        )
        .expect("a span");
        // 2026-03-15T00:00:00Z as the store's sort key.
        let seconds = 1_773_532_800i64 + 100_000_000_000i64;
        assert!(interval.contains(&format!("unix:{seconds:012}:000000000")));
    }

    #[test]
    fn a_validity_span_overlaps_when_it_touches_the_interval_at_all() {
        let interval = TemporalInterval::new(
            Some("2026-03-01T00:00:00Z".to_string()),
            Some("2026-04-01T00:00:00Z".to_string()),
        )
        .expect("a span");
        // Started before, ended inside: it was in force when the span began.
        assert!(interval.overlaps(Some("2026-01-01T00:00:00Z"), Some("2026-03-10T00:00:00Z")));
        // Ended exactly at the start: exclusive end, no overlap.
        assert!(!interval.overlaps(Some("2026-01-01T00:00:00Z"), Some("2026-03-01T00:00:00Z")));
        // Started exactly at the end: no overlap.
        assert!(!interval.overlaps(Some("2026-04-01T00:00:00Z"), None));
        // Still in force, started before the span: overlap.
        assert!(interval.overlaps(Some("2026-01-01T00:00:00Z"), None));
        // No clocks at all: nothing to say against it here; the caller
        // decides whether an absent clock is admitted.
        assert!(interval.overlaps(None, None));
    }

    #[test]
    fn an_interval_needs_a_bound_and_must_run_forwards() {
        assert!(TemporalInterval::new(None, None).is_err());
        assert!(
            TemporalInterval::new(
                Some("2026-04-01T00:00:00Z".to_string()),
                Some("2026-03-01T00:00:00Z".to_string())
            )
            .is_err()
        );
        assert!(
            TemporalInterval::new(
                Some("2026-03-01T00:00:00Z".to_string()),
                Some("2026-03-01T00:00:00Z".to_string())
            )
            .is_err(),
            "an empty span selects nothing"
        );
        assert!(TemporalInterval::new(Some("yesterday".to_string()), None).is_err());
    }

    #[test]
    fn distance_outside_is_zero_inside_and_grows_away_from_either_bound() {
        let interval = TemporalInterval::new(
            Some("2026-03-01T00:00:00Z".to_string()),
            Some("2026-03-02T00:00:00Z".to_string()),
        )
        .expect("a day");
        assert_eq!(interval.distance_outside("2026-03-01T12:00:00Z"), Some(0));
        assert_eq!(
            interval.distance_outside("2026-02-28T00:00:00Z"),
            Some(86_400 * 1_000_000_000)
        );
        assert!(
            interval.distance_outside("2026-03-03T00:00:00Z")
                > interval.distance_outside("2026-03-02T00:00:00Z")
        );
        assert_eq!(interval.distance_outside("never"), None);
    }
}
