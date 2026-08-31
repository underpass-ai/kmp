//! The name a retried intent travels under.

/// An idempotency key: a retried intent with the same key is the same
/// intent, not a second one. Keys are opaque and compared byte for byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// A key exactly as the caller sent it.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// The key, byte for byte.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
