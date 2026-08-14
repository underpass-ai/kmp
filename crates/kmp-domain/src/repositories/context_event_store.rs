use std::future::Future;
use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::PortError;

/// Domain event emitted when a context update is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextUpdatedEvent {
    pub root_node_id: String,
    pub role: String,
    pub revision: u64,
    pub content_hash: String,
    pub changes: Vec<ContextEventChange>,
    pub idempotency_key: Option<String>,
    /// Digest of the logical command behind this event, when the caller
    /// supplied one. `content_hash` covers the *translated* changes, which can
    /// legitimately differ between a first apply and a replay of the same
    /// logical command; this digest is what stays equal across that replay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_digest: Option<String>,
    pub requested_by: Option<String>,
    #[serde(with = "system_time_serde")]
    pub occurred_at: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEventChange {
    pub operation: String,
    pub entity_kind: String,
    pub entity_id: String,
    pub payload_json: String,
    /// Why this change was made. Preserved for audit trail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Authorization scopes under which this change was made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
}

/// Outcome previously accepted for a given idempotency key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotentOutcome {
    pub revision: u64,
    pub content_hash: String,
    /// The event's logical digest, when it carried one. `None` on records
    /// written before this field existed — a comparison must fall back to
    /// `content_hash`, never assume equality.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_digest: Option<String>,
}

/// Append-only event store for context update commands.
///
/// Provides optimistic concurrency via `expected_revision` on append,
/// and deduplication via idempotency key lookup.
pub trait ContextEventStore {
    /// Append an event. Fails with `PortError::Conflict` if
    /// `expected_revision` does not match the current revision for this
    /// `(root_node_id, role)` aggregate.
    fn append(
        &self,
        event: ContextUpdatedEvent,
        expected_revision: u64,
    ) -> impl Future<Output = Result<u64, PortError>> + Send;

    /// Returns the current revision for the aggregate, or 0 if no events exist.
    fn current_revision(
        &self,
        root_node_id: &str,
        role: &str,
    ) -> impl Future<Output = Result<u64, PortError>> + Send;

    /// Returns the content hash of the last accepted event, or None if no events exist.
    fn current_content_hash(
        &self,
        root_node_id: &str,
        role: &str,
    ) -> impl Future<Output = Result<Option<String>, PortError>> + Send;

    /// Checks if an event with this idempotency key was already accepted.
    fn find_by_idempotency_key(
        &self,
        key: &str,
    ) -> impl Future<Output = Result<Option<IdempotentOutcome>, PortError>> + Send;
}

impl<T> ContextEventStore for Arc<T>
where
    T: ContextEventStore + Send + Sync + ?Sized,
{
    async fn append(
        &self,
        event: ContextUpdatedEvent,
        expected_revision: u64,
    ) -> Result<u64, PortError> {
        self.as_ref().append(event, expected_revision).await
    }

    async fn current_revision(&self, root_node_id: &str, role: &str) -> Result<u64, PortError> {
        self.as_ref().current_revision(root_node_id, role).await
    }

    async fn current_content_hash(
        &self,
        root_node_id: &str,
        role: &str,
    ) -> Result<Option<String>, PortError> {
        self.as_ref().current_content_hash(root_node_id, role).await
    }

    async fn find_by_idempotency_key(
        &self,
        key: &str,
    ) -> Result<Option<IdempotentOutcome>, PortError> {
        self.as_ref().find_by_idempotency_key(key).await
    }
}

mod system_time_serde {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let millis = time
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64;
        millis.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(UNIX_EPOCH + Duration::from_millis(millis))
    }
}
