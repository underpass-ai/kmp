/// A calendar date `YYYY-MM-DD` for a Unix instant, in UTC. The judge reads
/// it before a fact's text: which of two facts came first decides the
/// direction of a replacement or an update, and the text alone rarely says.
pub(crate) fn fact_date(seconds: i64) -> String {
    // Howard Hinnant's days-to-civil.
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants_read_as_their_dates() {
        assert_eq!(fact_date(0), "1970-01-01");
        assert_eq!(fact_date(1_772_445_600), "2026-03-02");
        assert_eq!(fact_date(951_782_400), "2000-02-29");
        assert_eq!(fact_date(-86_400), "1969-12-31");
    }
}
