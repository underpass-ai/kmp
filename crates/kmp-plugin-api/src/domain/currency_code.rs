use std::fmt;

use serde::{Deserialize, Serialize};

use super::interpretation_error::InterpretationError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn new(value: impl AsRef<str>) -> Result<Self, InterpretationError> {
        let normalized = value.as_ref().trim().to_ascii_uppercase();
        if normalized.len() != 3 || !normalized.chars().all(|char| char.is_ascii_uppercase()) {
            return Err(InterpretationError::new(format!(
                "invalid currency code `{}`",
                value.as_ref()
            )));
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_to_uppercase_iso_like_code() {
        assert_eq!(CurrencyCode::new(" usd ").expect("code").as_str(), "USD");
    }

    #[test]
    fn rejects_non_iso_like_values() {
        assert_eq!(
            CurrencyCode::new("US").expect_err("invalid").to_string(),
            "invalid currency code `US`"
        );
    }
}
