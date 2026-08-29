use std::fmt::{Display, Formatter};

use crate::domain::release_error::ReleaseError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CalendarDate(String);

impl CalendarDate {
    pub fn parse(value: impl Into<String>) -> Result<Self, ReleaseError> {
        let value = value.into();
        let bytes = value.as_bytes();
        if bytes.len() != 10
            || bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes
                .iter()
                .enumerate()
                .any(|(index, byte)| !matches!(index, 4 | 7) && !byte.is_ascii_digit())
        {
            return Err(ReleaseError::invalid(format!(
                "invalid release date `{value}`; expected YYYY-MM-DD"
            )));
        }
        Ok(Self(value))
    }

    pub fn today_utc() -> Result<Self, ReleaseError> {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| {
                ReleaseError::invalid(format!("system clock predates Unix epoch: {error}"))
            })?
            .as_secs();
        let days = i64::try_from(seconds / 86_400)
            .map_err(|_| ReleaseError::invalid("current date is outside the supported range"))?;
        let era_day = days + 719_468;
        let era = if era_day >= 0 {
            era_day
        } else {
            era_day - 146_096
        } / 146_097;
        let day_of_era = era_day - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let mut year = year_of_era + era * 400;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_prime = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
        let month = month_prime + if month_prime < 10 { 3 } else { -9 };
        year += i64::from(month <= 2);
        Self::parse(format!("{year:04}-{month:02}-{day:02}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for CalendarDate {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
