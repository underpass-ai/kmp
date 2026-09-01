//! What an intent actually asked for, as identity.

/// The stable identity of what a caller asked the loom to change, taken
/// before store-local selectors are resolved — so a retry remains the same
/// intent even if the mounted catalog changes between calls, and a key
/// reused for different content is caught as a collision, not honored as a
/// replay. How the digest is computed is the boundary's business; the
/// aggregate only compares them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentDigest(String);

impl IntentDigest {
    /// A digest as the boundary computed it.
    pub fn new(digest: impl Into<String>) -> Self {
        Self(digest.into())
    }

    /// The digest, byte for byte.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
