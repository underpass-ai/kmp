use kmp_domain::consolidation::{ConsolidationClocks, ConsolidationSource};
use serde::Serialize;
use std::collections::BTreeMap;

/// Writer input delivers canonical text once. The source stamp still covers
/// the complete captured source; immutable audits retain all properties.
#[derive(Serialize)]
pub(super) struct SourceInput<'a> {
    reference: &'a str,
    stamp: &'a str,
    body: &'a str,
    status: &'a str,
    properties: BTreeMap<&'a str, &'a str>,
    provenance: &'a BTreeMap<String, String>,
    coordinates: &'a [ConsolidationClocks],
    dependency_clocks: &'a [ConsolidationClocks],
    relations: &'a [(Vec<String>, BTreeMap<String, String>)],
}

impl<'a> From<&'a ConsolidationSource> for SourceInput<'a> {
    fn from(source: &'a ConsolidationSource) -> Self {
        Self {
            reference: &source.reference,
            stamp: &source.stamp,
            body: &source.body,
            status: &source.status,
            properties: source
                .properties
                .iter()
                .filter(|(key, _)| !matches!(key.as_str(), "payload_text" | "memory_payload_json"))
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect(),
            provenance: &source.provenance,
            coordinates: &source.coordinates,
            dependency_clocks: &source.dependency_clocks,
            relations: &source.relations,
        }
    }
}
