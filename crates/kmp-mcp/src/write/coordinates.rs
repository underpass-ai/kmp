use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
pub(super) struct WriterCoordinate<'a> {
    pub(super) occurred_at: Option<&'a str>,
    pub(super) observed_at: &'a str,
    pub(super) valid_from: Option<&'a str>,
    pub(super) valid_until: Option<&'a str>,
    pub(super) rank: Option<u32>,
}

pub(super) fn coordinate(
    dimension: &str,
    scope_id: &str,
    sequence: Option<u32>,
    clocks: WriterCoordinate<'_>,
) -> Value {
    let mut coordinate = json!({
        "dimension": dimension,
        "scope_id": scope_id,
        "observed_at": clocks.observed_at
    });
    let fields = coordinate
        .as_object_mut()
        .expect("coordinate literal is a JSON object");
    if let Some(occurred_at) = clocks.occurred_at {
        fields.insert("occurred_at".to_string(), json!(occurred_at));
    }
    if let Some(valid_from) = clocks.valid_from {
        fields.insert("valid_from".to_string(), json!(valid_from));
    }
    if let Some(valid_until) = clocks.valid_until {
        fields.insert("valid_until".to_string(), json!(valid_until));
    }
    if let Some(rank) = clocks.rank {
        fields.insert("rank".to_string(), json!(rank));
    }
    if let Some(sequence) = sequence {
        fields.insert("sequence".to_string(), json!(sequence));
    }
    coordinate
}

pub(super) fn shifted_coordinates(coordinates: &Value, offset: u32) -> Value {
    let mut shifted = coordinates.clone();
    if let Some(coordinates) = shifted.as_array_mut() {
        for coordinate in coordinates {
            if let Some(sequence) = coordinate.get_mut("sequence") {
                *sequence = json!(sequence.as_u64().unwrap_or_default() + u64::from(offset));
            }
        }
    }
    shifted
}

#[allow(clippy::too_many_arguments)]
pub(super) fn reject_a_time_that_has_not_happened(
    observed_at: &str,
    now: i64,
) -> Result<(), String> {
    let Some(stamped) = crate::clock::rfc3339_to_seconds(observed_at) else {
        return Ok(());
    };
    let ahead = stamped - now;
    if ahead <= crate::clock::FUTURE_TOLERANCE_SECONDS {
        return Ok(());
    }
    Err(format!(
        "`observed_at` is {} ahead of this kernel's clock, so it has not happened yet. \
         RFC3339 permits an offset and `{observed_at}` claims UTC: local wall-clock time \
         written with a `Z` is the usual cause. Stamp the real UTC time — memory is ordered \
         by this field, and an entry above the present is one that reading forward from now \
         never finds.",
        humanise(ahead)
    ))
}

/// A duration a person can judge at a glance. "12240 seconds" is a number to
/// do arithmetic on; "3h 24m" is recognisably a timezone.
pub(super) fn humanise(seconds: i64) -> String {
    let (hours, minutes) = (seconds / 3_600, (seconds % 3_600) / 60);
    match (hours, minutes) {
        (0, 0) => format!("{seconds}s"),
        (0, minutes) => format!("{minutes}m"),
        (hours, 0) => format!("{hours}h"),
        (hours, minutes) => format!("{hours}h {minutes}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::{humanise, reject_a_time_that_has_not_happened};

    /// 2026-08-24T20:44:21Z — the wall clock in the log line that exposed the
    /// skew, used as "now" so these do not drift with the calendar.
    const NOW: i64 = 1787604261;

    #[test]
    fn a_stamp_from_the_past_is_a_backfill_and_is_allowed() {
        // Recording something that happened earlier is legitimate and common:
        // an incident written up the next morning is still true.
        assert!(reject_a_time_that_has_not_happened("2026-08-18T05:57:01.068Z", NOW).is_ok());
    }
    #[test]
    fn a_stamp_inside_the_tolerance_survives_a_skewed_clock() {
        assert!(reject_a_time_that_has_not_happened("2026-08-24T20:46:00Z", NOW).is_ok());
    }
    #[test]
    fn local_wall_clock_written_as_utc_is_refused_and_told_why() {
        // The exact failure: CEST written with a `Z`, three hours and twenty
        // minutes into the future, accepted in silence.
        let error = reject_a_time_that_has_not_happened("2026-08-25T00:05:00Z", NOW)
            .expect_err("a time that has not happened");
        assert!(error.contains("3h 20m"), "say how far out it is: {error}");
        assert!(
            error.contains("local wall-clock"),
            "and name the cause, because it is always the same one: {error}"
        );
    }
    #[test]
    fn a_correct_offset_is_read_as_the_time_it_actually_is() {
        // 22:44+02:00 is 20:44Z — the same instant, not two hours ahead. A
        // check that refused this would punish the writers doing it right.
        assert!(reject_a_time_that_has_not_happened("2026-08-24T22:44:21+02:00", NOW).is_ok());
    }
    #[test]
    fn a_shape_the_clock_cannot_read_is_left_to_the_ingest_layer() {
        assert!(reject_a_time_that_has_not_happened("yesterday", NOW).is_ok());
    }
    #[test]
    fn a_duration_reads_as_a_timezone_rather_than_a_number_of_seconds() {
        assert_eq!(humanise(12_000), "3h 20m");
        assert_eq!(humanise(7_200), "2h");
        assert_eq!(humanise(600), "10m");
        assert_eq!(humanise(42), "42s");
    }
}
