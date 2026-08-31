use std::time::Duration;

/// How long a publication waits for a release to become fully public.
///
/// The catalog must never point at a release that is still uploading, so the
/// wait is part of the release contract rather than a sleep buried in a loop:
/// it is the difference between "not yet" and "something is wrong".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetWait {
    timeout: Duration,
    poll: Duration,
}

impl AssetWait {
    pub fn new(timeout: Duration, poll: Duration) -> Self {
        Self { timeout, poll }
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn poll(&self) -> Duration {
        self.poll
    }

    pub fn timeout_minutes(&self) -> u64 {
        self.timeout.as_secs() / 60
    }
}

impl Default for AssetWait {
    /// A release build that uploads twenty assets across five targets is slow,
    /// and a poll that is too eager only spends API budget.
    fn default() -> Self {
        Self::new(Duration::from_secs(45 * 60), Duration::from_secs(20))
    }
}
