use std::future::Future;

use crate::{ApiCapabilities, ApiError, MemoryAskRequest, MemoryRecallView, MemoryWakeRequest};

/// What an embedding product may ask of the kernel.
///
/// Reads only. Ingestion, projection and export have their own commands,
/// idempotency and provenance inside the kernel; a consumer reaches them
/// through the kernel's own surfaces. What this trait promises is the part a
/// consumer needs to *recall* — and to keep working, with a stub, when the
/// kernel is absent.
///
/// Methods return named `Send` futures rather than using `async fn`, so the
/// trait can be consumed generically from multi-threaded runtimes. It is not
/// object-safe; consumers hold an implementation by generic parameter, which
/// is also what lets a stub replace the kernel in their tests.
pub trait MemoryRecallApi: Send + Sync {
    /// What this implementation is and what it can do. Checked by consumers at
    /// startup, before anything is at stake.
    fn capabilities(&self) -> ApiCapabilities;

    /// Recall the bounded context of one about.
    fn wake(
        &self,
        request: MemoryWakeRequest,
    ) -> impl Future<Output = Result<MemoryRecallView, ApiError>> + Send;

    /// Ask one question of the memory, under an explicit answer policy.
    fn ask(
        &self,
        request: MemoryAskRequest,
    ) -> impl Future<Output = Result<MemoryRecallView, ApiError>> + Send;
}
