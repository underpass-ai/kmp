use serde::{Deserialize, Serialize};

/// Source coordinates decoded at the storage boundary. No clock is inferred.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationClocks {
    pub occurred_at: Option<String>,
    pub observed_at: Option<String>,
    pub ingested_at: Option<String>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
}
