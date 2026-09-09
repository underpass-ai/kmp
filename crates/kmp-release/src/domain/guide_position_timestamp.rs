use super::calendar_date::CalendarDate;
use super::release_error::ReleaseError;

/// Synthetic guide positions, one minute apart from the established origin.
pub struct GuidePositionTimestamp(String);

impl GuidePositionTimestamp {
    pub fn from_sequence(sequence: usize) -> Result<Self, ReleaseError> {
        const ORIGIN: u64 = 1_787_875_200; // 2026-08-28T00:00:00Z
        let minutes = u64::try_from(sequence.saturating_sub(1))
            .map_err(|_| ReleaseError::invalid("guide position is outside the timestamp range"))?;
        let seconds = minutes
            .checked_mul(60)
            .and_then(|offset| ORIGIN.checked_add(offset))
            .ok_or_else(|| {
                ReleaseError::invalid("guide position is outside the timestamp range")
            })?;
        let date = CalendarDate::from_unix_seconds(seconds)?;
        let within_day = seconds % 86_400;
        Ok(Self(format!(
            "{date}T{:02}:{:02}:00Z",
            within_day / 3600,
            (within_day % 3600) / 60
        )))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
