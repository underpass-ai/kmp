use super::ConsolidationAssertion;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsolidationWrite {
    pub about: String,
    pub view: String,
    /// Zero creates a view. Other values compare-and-set its current revision.
    pub expect_revision: u64,
    pub idempotency_key: String,
    pub author: String,
    /// Exact stamps returned by source capture. Includes ungrouped dependencies.
    pub sources: BTreeMap<String, String>,
    pub assertions: Vec<ConsolidationAssertion>,
}
