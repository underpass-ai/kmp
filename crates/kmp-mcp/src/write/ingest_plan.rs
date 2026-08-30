use super::accepted_counts::AcceptedCounts;
use super::ingest_change::KmpIngestChange;

/// The canonical batch one validated ingest will apply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct KmpIngestPlan {
    pub(crate) about: String,
    pub(crate) memory_id: String,
    pub(crate) idempotency_key: String,
    pub(crate) requested_by: Option<String>,
    pub(crate) correlation_id: Option<String>,
    pub(crate) causation_id: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) accepted: AcceptedCounts,
    pub(crate) changes: Vec<KmpIngestChange>,
}
