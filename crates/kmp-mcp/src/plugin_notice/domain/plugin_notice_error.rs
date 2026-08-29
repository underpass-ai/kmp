use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginNoticeError {
    InvalidCommand(String),
    InvalidManifest(String),
}

impl fmt::Display for PluginNoticeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCommand(detail) => write!(formatter, "invalid notice command: {detail}"),
            Self::InvalidManifest(detail) => write!(formatter, "invalid plugin manifest: {detail}"),
        }
    }
}

impl std::error::Error for PluginNoticeError {}
