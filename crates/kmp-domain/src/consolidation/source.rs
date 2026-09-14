use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Kernel-captured source and direct relations, bound to one read transaction.
/// Properties and relation clocks are preserved literally, including absences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsolidationSource {
    pub reference: String,
    pub stamp: String,
    pub body: String,
    pub status: String,
    pub properties: BTreeMap<String, String>,
    pub provenance: BTreeMap<String, String>,
    pub coordinates: Vec<super::ConsolidationClocks>,
    pub dependency_clocks: Vec<super::ConsolidationClocks>,
    /// [source ref, target ref, relation type] -> literal explanation fields.
    pub relations: Vec<(Vec<String>, BTreeMap<String, String>)>,
}
