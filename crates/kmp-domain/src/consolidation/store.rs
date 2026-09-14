use super::{ConsolidatedView, ConsolidationRead, ConsolidationSource, ConsolidationWrite};
use crate::PortError;
use std::{fmt::Debug, future::Future, pin::Pin};

pub type ConsolidationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, PortError>> + Send + 'a>>;

/// Capture and read each use one snapshot. Admission, CAS, source checks,
/// idempotency receipt and immutable revision insertion share one transaction.
pub trait ConsolidationStore: Debug + Send + Sync {
    fn consolidation_sources(
        &self,
        about: String,
        refs: Vec<String>,
    ) -> ConsolidationFuture<'_, Vec<ConsolidationSource>>;
    fn write_consolidation(
        &self,
        command: ConsolidationWrite,
    ) -> ConsolidationFuture<'_, ConsolidatedView>;
    /// An explicit revision is an audit, never current evidence. Current reads
    /// return no derived claims if any declared dependency has changed.
    fn read_consolidation(
        &self,
        about: String,
        view: String,
        revision: Option<u64>,
    ) -> ConsolidationFuture<'_, ConsolidationRead>;
}
