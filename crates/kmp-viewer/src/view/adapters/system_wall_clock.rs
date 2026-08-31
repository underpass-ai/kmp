//! The wall clock this process actually reads.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::view::domain::Timestamp;
use crate::view::ports::WallClock;

/// Reads the operating system's clock and speaks it in the kernel's RFC3339
/// notation.
#[derive(Default)]
pub struct SystemWallClock;

impl SystemWallClock {
    /// The system clock.
    pub fn new() -> Self {
        Self
    }
}

impl WallClock for SystemWallClock {
    fn now(&self) -> Timestamp {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or(0);
        Timestamp::new(crate::time_format::rfc3339_utc(seconds))
    }
}
