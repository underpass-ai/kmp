use std::future::Future;

use crate::{ApiCapabilities, ApiError, MemoryRecordRequest, RecordedMemoryView};

/// What an embedding product may put into the kernel.
///
/// A separate trait from [`crate::MemoryRecallApi`] on purpose: that trait
/// promises reads only, and a consumer that can recall should not acquire the
/// power to write by holding the same object. A consumer that records —
/// typically an integration applying its own outbox — asks for this trait by
/// name, and a reviewer sees the write dependency in its bounds.
///
/// Same conventions as the recall side: named `Send` futures, not object-safe,
/// consumed by generic parameter, stubbed in consumer tests.
pub trait MemoryRecordApi: Send + Sync {
    /// What this implementation is and what it can do. The report is the same
    /// one the recall side gives — one implementation, one account of itself.
    fn capabilities(&self) -> ApiCapabilities;

    /// Put something into the memory of one about, idempotently.
    ///
    /// A retry with the same `idempotency_key` and the same content returns
    /// the recorded outcome; the same key with different content is refused.
    /// That contract is what lets an at-least-once delivery pipeline apply
    /// records without counting them twice.
    fn record(
        &self,
        request: MemoryRecordRequest,
    ) -> impl Future<Output = Result<RecordedMemoryView, ApiError>> + Send;
}
