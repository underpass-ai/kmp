//! The one clock spelling the crate shares: epoch seconds rendered as
//! RFC3339 UTC, used by the memory-read views and the view context's
//! wall clock alike so neither reaches into the other for a formatter.

/// Epoch seconds to `YYYY-MM-DDTHH:MM:SSZ`, without a date library.
///
/// Howard Hinnant's civil-from-days, which is exact for every day this store
/// can hold. The crate ships no dependency the embedded edition does not
/// already carry, and a viewer is not the place to start.
pub(crate) fn rfc3339_utc(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let time_of_day = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (
        time_of_day / 3_600,
        (time_of_day % 3_600) / 60,
        time_of_day % 60,
    );

    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::rfc3339_utc;

    #[test]
    fn civil_from_days_is_exact_across_epochs_and_leap_years() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(-1), "1969-12-31T23:59:59Z");
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(4_107_542_400), "2100-03-01T00:00:00Z");
    }
}
