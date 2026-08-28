use std::error::Error;
use std::fmt;

use kmp_domain::{DomainError, PortError};

#[derive(Debug)]
pub enum ApplicationError {
    Domain(DomainError),
    Ports(PortError),
    /// Optimistic concurrency rejected this attempt before it committed.
    /// Re-reading current state and replaying the same logical command with
    /// the same idempotency key is safe.
    RetryableConflict(String),
    NotFound(String),
    Validation(String),
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(f),
            Self::Ports(error) => error.fmt(f),
            Self::RetryableConflict(message) => f.write_str(message),
            Self::NotFound(message) => f.write_str(message),
            Self::Validation(message) => f.write_str(message),
        }
    }
}

impl Error for ApplicationError {}

impl From<DomainError> for ApplicationError {
    fn from(value: DomainError) -> Self {
        Self::Domain(value)
    }
}

impl From<PortError> for ApplicationError {
    fn from(value: PortError) -> Self {
        Self::Ports(value)
    }
}
