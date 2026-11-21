use crate::continuation::{ContinuationError, ReadContinuation, ReadContinuationId};

/// Retains a proposed continuation so a page hands back a short handle
/// instead of restating the call. Bounded and expiring: the handle names a
/// call, it grants nothing, and every transport authorizes the resolved
/// request.
pub(crate) trait ContinuationStore: Send + Sync {
    fn save(&self, call: &ReadContinuation) -> Result<ReadContinuationId, ContinuationError>;
    fn load(&self, id: &ReadContinuationId) -> Result<Option<ReadContinuation>, ContinuationError>;
}
