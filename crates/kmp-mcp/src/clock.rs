//! What time it is, and whether a writer's stamp could have happened.
//!
//! `observed_at` is supplied by the writer and was checked against nothing.
//! On this project's own store that produced a frontier in the future: agents
//! wrote local wall-clock time with a `Z`, and since RFC3339 permits an
//! offset they were not even out of spec. The whole read path is ordered by
//! this field, so `kernel_forward` from a correct present returned nothing
//! while unread entries sat above it — an empty delta that looks exactly like
//! a quiet week.
//!
//! An observation cannot have happened after now. That is the one thing about
//! this field a kernel can check without trusting anybody, and in an
//! append-only log with no delete it has to be checked before the write, not
//! after.
//!
//! Parsing is hand-rolled for the same reason `kmp-viewer` renders times
//! without a date library: the embedded edition carries no dependency it does
//! not already need, and this is one function in each direction.

use std::time::{SystemTime, UNIX_EPOCH};

/// How far ahead of the kernel's clock a stamp may sit before it is refused.
///
/// Not zero: an agent and a kernel can be minutes apart across a network, and
/// refusing a write over a second of skew would be its own bug. Wide enough to
/// forgive a clock, far too narrow to forgive a timezone.
pub const FUTURE_TOLERANCE_SECONDS: i64 = 300;

/// Epoch seconds now, from the system clock.
pub fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default()
}

/// Epoch seconds for an RFC3339 timestamp, or `None` when it is not one.
///
/// `None` is "not a timestamp I can read", never "fine": the caller decides
/// what to do about a shape it does not recognise, and this function does not
/// get to wave one through by failing to parse it.
pub fn rfc3339_to_seconds(value: &str) -> Option<i64> {
    let value = value.trim();
    let bytes = value.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let number = |from: usize, to: usize| -> Option<i64> { value.get(from..to)?.parse().ok() };
    let at = |index: usize, expected: &[u8]| -> bool { expected.contains(&bytes[index]) };

    if !at(4, b"-") || !at(7, b"-") || !at(10, b"Tt ") || !at(13, b":") || !at(16, b":") {
        return None;
    }
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let mut rest = &value[19..];
    // Fractional seconds are dropped rather than rounded: this exists to
    // compare against a tolerance measured in minutes.
    if rest.starts_with('.') {
        let digits = rest[1..].bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &rest[1 + digits..];
    }

    let offset_seconds = match rest.as_bytes() {
        [] => return None, // A time with no zone is not RFC3339, and not UTC.
        [b'Z' | b'z'] => 0,
        [sign @ (b'+' | b'-'), ..] if rest.len() == 6 && rest.as_bytes()[3] == b':' => {
            let hours: i64 = rest.get(1..3)?.parse().ok()?;
            let minutes: i64 = rest.get(4..6)?.parse().ok()?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let magnitude = hours * 3_600 + minutes * 60;
            if *sign == b'-' { -magnitude } else { magnitude }
        }
        _ => return None,
    };

    Some(
        days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second
            - offset_seconds,
    )
}

/// Days since 1970-01-01 — Howard Hinnant's days-from-civil, the inverse of
/// the civil-from-days the viewer uses to render.
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
    use super::*;

    #[test]
    fn a_utc_timestamp_round_trips_against_a_known_epoch_second() {
        // 2026-08-24T20:44:21Z, the wall clock in the log line that exposed
        // the skew this module exists to stop.
        assert_eq!(rfc3339_to_seconds("2026-08-24T20:44:21Z"), Some(1787604261));
        assert_eq!(rfc3339_to_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_to_seconds("2000-02-29T00:00:00Z"), Some(951782400));
    }

    #[test]
    fn an_offset_is_honoured_rather_than_ignored() {
        // The bug in the wild: the same wall clock written three ways. Only
        // one of them was the time it actually was.
        let utc = rfc3339_to_seconds("2026-08-24T20:44:21Z").expect("utc");
        assert_eq!(rfc3339_to_seconds("2026-08-24T22:44:21+02:00"), Some(utc));
        assert_eq!(rfc3339_to_seconds("2026-08-24T18:44:21-02:00"), Some(utc));
        // …and this one, which is what agents were writing, is two hours out.
        assert_eq!(
            rfc3339_to_seconds("2026-08-24T22:44:21Z"),
            Some(utc + 7_200)
        );
    }

    #[test]
    fn fractional_seconds_parse_and_do_not_move_the_answer() {
        assert_eq!(
            rfc3339_to_seconds("2026-08-18T05:57:01.068Z"),
            rfc3339_to_seconds("2026-08-18T05:57:01Z")
        );
    }

    #[test]
    fn a_time_without_a_zone_is_not_a_time_this_kernel_accepts() {
        // RFC3339 requires one, and the whole failure this guards against was
        // a zone that was assumed rather than stated.
        assert_eq!(rfc3339_to_seconds("2026-08-24T20:44:21"), None);
    }

    #[test]
    fn shapes_that_are_not_timestamps_come_back_as_none() {
        for value in [
            "",
            "yesterday",
            "2026-08-24",
            "unix:101787521200:000000000",
            "2026-13-01T00:00:00Z",
            "2026-08-24T25:00:00Z",
            "2026-08-24T20:44:21+0200",
            "2026-08-24T20:44:21.Z",
        ] {
            assert_eq!(rfc3339_to_seconds(value), None, "`{value}` is not a time");
        }
    }

    #[test]
    fn the_clock_reads_something_after_this_was_written() {
        assert!(now_seconds() > rfc3339_to_seconds("2026-08-01T00:00:00Z").expect("a real time"));
    }
}
