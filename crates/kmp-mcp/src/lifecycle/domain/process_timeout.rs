use std::time::Duration;

/// Bounded wall-clock time for an external lifecycle process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessTimeout(Duration);

impl ProcessTimeout {
    pub const fn seconds(seconds: u64) -> Self {
        Self(Duration::from_secs(seconds))
    }

    pub const fn duration(self) -> Duration {
        self.0
    }
}

impl Default for ProcessTimeout {
    fn default() -> Self {
        Self::seconds(120)
    }
}
