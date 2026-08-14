use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortError {
    InvalidState(String),
    Unavailable(String),
    Conflict(String),
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState(message) | Self::Unavailable(message) | Self::Conflict(message) => {
                f.write_str(message)
            }
        }
    }
}

impl Error for PortError {}
