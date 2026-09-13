//! The token a writer hands back after reading the served neighborhood.
//!
//! One concept: the shape of that token. Whether it still matches the
//! packet is the kernel's judgement, not this module's.

use serde_json::Value;

use super::json_value_type::JsonValueType;
use super::validation_error::WriteValidationError;

/// Refuses anything that cannot be a returned neighborhood token before the
/// packet reaches storage, so a typo reads as a typo rather than a stale
/// review.
pub(crate) fn validate_review_token(token: Option<&Value>) -> Result<(), WriteValidationError> {
    let Some(token) = token else {
        return Ok(());
    };
    let token = token.as_str().ok_or_else(|| {
        WriteValidationError::wrong_type("review_token", JsonValueType::String, token)
    })?;
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(WriteValidationError::new(
            "review_token must be the returned 64-character neighborhood token",
        )
        .at("review_token")
        .code("INVALID_REVIEW_TOKEN"));
    }
    Ok(())
}
