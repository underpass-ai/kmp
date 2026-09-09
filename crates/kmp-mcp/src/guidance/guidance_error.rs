#[derive(Debug)]
pub(crate) enum GuidanceError {
    InvalidSession(String),
    StaleGuide,
    Unavailable(String),
}

impl std::fmt::Display for GuidanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSession(message) | Self::Unavailable(message) => f.write_str(message),
            Self::StaleGuide => f.write_str(
                "guide revision changed; reopen guidance before recording this delivery",
            ),
        }
    }
}

impl std::error::Error for GuidanceError {}
