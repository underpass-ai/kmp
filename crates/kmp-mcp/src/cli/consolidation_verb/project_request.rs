use kmp_domain::consolidation::ConsolidationSelection;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectRequest {
    pub about: String,
    pub view: String,
    pub selection: Option<ConsolidationSelection>,
    pub max_bytes: Option<usize>,
}
