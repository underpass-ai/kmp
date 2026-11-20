use std::time::Duration;

/// What a started container must show before a test may use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitFor {
    /// Wait a fixed time after the container starts.
    Duration(Duration),
    /// Wait until the container's standard output contains this text.
    StdoutMessage(String),
    /// Wait until the container's standard error contains this text.
    StderrMessage(String),
}

impl WaitFor {
    pub fn seconds(seconds: u64) -> Self {
        Self::Duration(Duration::from_secs(seconds))
    }

    pub fn message_on_stdout(message: impl Into<String>) -> Self {
        Self::StdoutMessage(message.into())
    }

    pub fn message_on_stderr(message: impl Into<String>) -> Self {
        Self::StderrMessage(message.into())
    }
}
