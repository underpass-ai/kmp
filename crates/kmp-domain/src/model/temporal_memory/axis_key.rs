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
            value: canonical_time_key(value),
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

/// Normalize the two timestamp spellings that cross the kernel boundary.
///
/// Protobuf timestamps are persisted as `unix:<offset seconds>:<nanos>` so
/// byte order is chronological. HTTP and MCP callers commonly use RFC3339.
/// Keeping either spelling verbatim splits one clock into two lexical axes:
/// every `unix:` value sorts after every RFC3339 year. Unknown legacy values
/// remain orderable by their original bytes instead of being discarded.
fn canonical_time_key(value: &str) -> String {
    timestamp_nanos(value)
        .map(nanos_timestamp)
        .unwrap_or_else(|| value.to_string())
}

/// Compares two timestamp spellings on KMP's canonical temporal axis.
///
/// This is the boundary-safe comparison for callers that validate ranges:
/// persisted `unix:` clocks and RFC3339 input denote the same instants even
/// though their raw strings do not share a useful lexical order.
pub fn compare_temporal_instants(left: &str, right: &str) -> Option<Ordering> {
    Some(timestamp_nanos(left)?.cmp(&timestamp_nanos(right)?))
}

/// Reads an instant in either spelling the kernel stores as nanoseconds on
/// its canonical axis, for callers that measure distances rather than
/// compare; `None` for a value that is not an instant.
pub fn temporal_instant_nanos(value: &str) -> Option<i128> {
    timestamp_nanos(value)
}

fn timestamp_nanos(value: &str) -> Option<i128> {
    if let Some(value) = value.strip_prefix("unix:") {
        let (seconds, nanos) = value.split_once(':')?;
        let seconds = seconds.parse::<i128>().ok()? - 100_000_000_000i128;
        let nanos = nanos.parse::<i128>().ok()?;
        if !(0..1_000_000_000).contains(&nanos) {
            return None;
        }
        return Some(seconds * 1_000_000_000 + nanos);
    }
    basic_rfc3339_nanos(value)
}

fn nanos_timestamp(value: i128) -> String {
    let seconds = value.div_euclid(1_000_000_000);
    let nanos = value.rem_euclid(1_000_000_000);
    format!("unix:{:012}:{:09}", seconds + 100_000_000_000i128, nanos)
}

fn basic_rfc3339_nanos(value: &str) -> Option<i128> {
    let value = value.trim();
    if value.len() < 20 {
        return None;
    }
    let number = |from: usize, to: usize| -> Option<i64> { value.get(from..to)?.parse().ok() };
    let year = number(0, 4)?;
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    let second = number(17, 19)?;
    if value.get(4..5)? != "-"
        || value.get(7..8)? != "-"
        || value.get(10..11)? != "T"
        || value.get(13..14)? != ":"
        || value.get(16..17)? != ":"
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let tail = value.get(19..)?;
    let timezone_start = tail
        .char_indices()
        .find_map(|(index, character)| matches!(character, 'Z' | '+' | '-').then_some(index))?;
    let fraction = tail.get(..timezone_start)?;
    let timezone = tail.get(timezone_start..)?;
    let nanos = match fraction.strip_prefix('.') {
        Some(digits) if !digits.is_empty() && digits.len() <= 9 => {
            let padded = format!("{digits:0<9}");
            padded.parse::<i128>().ok()?
        }
        None if fraction.is_empty() => 0,
        _ => return None,
    };
    let offset_seconds = match timezone {
        "Z" => 0,
        offset if offset.len() == 6 && offset.get(3..4) == Some(":") => {
            let sign = match offset.get(..1)? {
                "+" => 1,
                "-" => -1,
                _ => return None,
            };
            let hours = offset.get(1..3)?.parse::<i64>().ok()?;
            let minutes = offset.get(4..6)?.parse::<i64>().ok()?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3_600 + minutes * 60)
        }
        _ => return None,
    };
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second
        - offset_seconds;
    Some(seconds as i128 * 1_000_000_000 + nanos)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{TemporalAxisKey, compare_temporal_instants};

    #[test]
    fn rfc3339_and_persisted_sortable_times_share_one_axis() {
        assert_eq!(
            TemporalAxisKey::time("2026-07-01T10:00:00Z"),
            TemporalAxisKey::time("unix:101782900000:000000000")
        );
        assert_eq!(
            TemporalAxisKey::time("2026-07-01T12:00:00+02:00"),
            TemporalAxisKey::time("unix:101782900000:000000000")
        );
        assert!(
            TemporalAxisKey::time("2026-07-01T10:00:00Z")
                < TemporalAxisKey::time("2026-07-02T10:00:00Z")
        );
    }

    #[test]
    fn public_instant_comparison_accepts_both_boundary_spellings() {
        assert_eq!(
            compare_temporal_instants("2026-07-01T12:00:00+02:00", "unix:101782900001:000000000"),
            Some(Ordering::Less)
        );
        assert_eq!(compare_temporal_instants("not-a-clock", "also-not"), None);
    }
}
