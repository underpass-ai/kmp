use super::{ConsolidatedView, ConsolidationReadStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationRead {
    /// current, stale, missing, or historical_audit. Only current is reusable.
    pub status: ConsolidationReadStatus,
    pub changed_sources: Vec<String>,
    pub view: Option<ConsolidatedView>,
}
