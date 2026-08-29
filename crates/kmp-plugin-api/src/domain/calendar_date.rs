use std::fmt;

use serde::{Deserialize, Serialize};

use super::interpretation_error::InterpretationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl CalendarDate {
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, InterpretationError> {
        if !(1..=12).contains(&month) {
            return Err(InterpretationError::new(format!(
                "invalid calendar month `{month}`"
            )));
        }
        let max_day = days_in_month(year, month);
        if day == 0 || day > max_day {
            return Err(InterpretationError::new(format!(
                "invalid calendar day `{day}` for {year:04}-{month:02}"
            )));
        }
        Ok(Self { year, month, day })
    }

    pub fn ordinal_days(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let adjusted_year = year - i32::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let month_i32 = i32::from(month);
    let day_i32 = i32::from(day);
    let day_of_year =
        (153 * (month_i32 + if month_i32 > 2 { -3 } else { 9 }) + 2) / 5 + day_i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    i64::from(era * 146_097 + day_of_era)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_day_for_month() {
        assert_eq!(
            CalendarDate::new(2026, 2, 29)
                .expect_err("invalid")
                .to_string(),
            "invalid calendar day `29` for 2026-02"
        );
    }
}
