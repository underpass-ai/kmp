use super::SourceSpan;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A caller-declared proof group assembled from existing native read packets.
///
/// Projection preserves this boundary, but does not certify semantic coverage.
/// Keep packet completeness/selection warnings and all dependent proof here.
/// `reads` contains the original native calls that can retrieve an omitted group;
/// response-local passage names and expiring continuation handles are not refs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextGroup {
    pub id: String,
    pub packets: Vec<Value>,
    pub reads: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<SourceSpan>,
}
