#[derive(Debug)]
pub(crate) enum ContinuationError {
    /// The caller sent something that is not a retained call.
    Invalid(String),
    /// The metadata store could not be read or written.
    Unavailable(String),
}

impl std::fmt::Display for ContinuationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) | Self::Unavailable(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ContinuationError {}
