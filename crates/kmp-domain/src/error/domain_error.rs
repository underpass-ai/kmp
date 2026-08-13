use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyValue(&'static str),
    InvalidState(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyValue(field) => write!(f, "{field} cannot be empty"),
            Self::InvalidState(message) => f.write_str(message),
        }
    }
}

impl Error for DomainError {}
