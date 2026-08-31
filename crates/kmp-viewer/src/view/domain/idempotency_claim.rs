//! A keyed intent's claim on the aggregate.

use crate::view::domain::idempotency_key::IdempotencyKey;
use crate::view::domain::intent_digest::IntentDigest;

/// The key and digest an intent arrives under. The pair is what makes the
/// idempotency contract checkable: the key names the intent, the digest
/// proves the content is the same one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyClaim {
    /// The name the intent travels under.
    pub key: IdempotencyKey,
    /// The identity of what it asked for.
    pub digest: IntentDigest,
}
