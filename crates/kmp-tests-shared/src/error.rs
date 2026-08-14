use std::error::Error;

/// Canonical error type for test infrastructure.
pub type BoxError = Box<dyn Error + Send + Sync>;
