//! Wire intent → its logical digest.

use sha2::{Digest, Sha256};

use crate::view::application::dto::ViewIntentDto;
use crate::view::domain::IntentDigest;

/// The stable identity of what the caller asked the loom to change, taken on
/// the intent as it arrived — before store-local selectors are resolved — so
/// a retry remains the same intent even if the mounted catalog changes
/// between calls.
pub fn logical_digest(intent: &ViewIntentDto) -> IntentDigest {
    let encoded = serde_json::to_vec(intent)
        .expect("view intents serialize: they hold only strings and vectors");
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    IntentDigest::new(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_identifies_content_not_the_retry() {
        let intent = || ViewIntentDto {
            search: Some(Some("attempt-000005".into())),
            ..ViewIntentDto::default()
        };
        assert_eq!(logical_digest(&intent()), logical_digest(&intent()));
        let other = ViewIntentDto {
            search: Some(Some("something else".into())),
            ..ViewIntentDto::default()
        };
        assert_ne!(logical_digest(&intent()), logical_digest(&other));
    }
}
