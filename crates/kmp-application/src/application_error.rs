use std::error::Error;
use std::fmt;

use kmp_domain::{DomainError, PortError};

#[derive(Debug)]
pub enum ApplicationError {
    Domain(DomainError),
    Ports(PortError),
    NotFound(String),
    Validation(String),
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(f),
            Self::Ports(error) => error.fmt(f),
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
