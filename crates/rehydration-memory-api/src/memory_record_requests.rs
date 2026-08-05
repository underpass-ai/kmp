use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A dimension the memory files things under.
///
/// Carried with every record rather than declared once, so a record is
/// self-contained: replayed alone, on an empty kernel, it still lands.
/// Redeclaring an existing dimension is a no-op, not an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDimensionSpec {
    pub id: String,
    pub kind: String,
    pub title: Option<String>,
}

/// Where an entry sits in one dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCoordinateSpec {
    /// The dimension's kind, matching a [`MemoryDimensionSpec::kind`].
    pub dimension: String,
    /// The dimension's id, matching a [`MemoryDimensionSpec::id`].
    pub scope_id: String,
    /// When it happened, RFC 3339. Distinct from when it was recorded.
    pub occurred_at: Option<String>,
    /// Order within the scope, for entries whose sequence matters.
    pub sequence: Option<u32>,
}

/// One thing the memory should hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEntrySpec {
    /// Chosen by the consumer and stable: recording the same id again is an
    /// update of that entry, not a second one.
    pub id: String,
    pub kind: String,
    pub text: String,
    pub coordinates: Vec<MemoryCoordinateSpec>,
    pub metadata: BTreeMap<String, String>,
}

/// What backs an entry up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEvidenceSpec {
    pub id: String,
    /// Entry ids this evidence supports.
    pub supports: Vec<String>,
    pub text: String,
    /// Where it came from, in the consumer's terms. The kernel carries it
    /// without reading it.
    pub source: Option<String>,
    /// When it was captured, RFC 3339.
    pub time: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

/// A directed link between two things the memory holds.
///
/// `from` and `to` name entry ids from this record, or ids the memory already
/// holds — not evidence declared alongside, which the kernel resolves after
/// relations. Arrived with its first consumer, like every field here: what
/// the contract carries is the subset a consumer has needed, not the kernel's
/// full relation vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRelationSpec {
    pub from: String,
    pub to: String,
    /// The relation, in the kernel's naming: `supports`, `conflicts_with`, …
    pub rel: String,
    /// `structural` or `evidential`. The kernel requires a non-structural
    /// relation to carry `confidence` and a `why` — a claimed link without a
    /// stated reason is exactly what a memory must not hold.
    pub semantic_class: String,
    pub why: Option<String>,
    pub confidence: Option<String>,
    pub sequence: Option<u32>,
}

/// Who put this into the memory, and on the strength of what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryProvenanceSpec {
    /// What kind of thing observed it. The kernel's closed vocabulary:
    /// `human`, `agent`, `projection`, `derived` or `unknown`. A record
    /// derived from another system's committed truth is a `projection`.
    pub source_kind: String,
    /// Which one.
    pub source_agent: String,
    /// When it was observed, RFC 3339.
    pub observed_at: String,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
}

/// Put something into the memory of one about.
///
/// The write side of the contract. Idempotent by declaration: the kernel keys
/// the record on `idempotency_key`, so a retry with the same key and the same
/// content returns the recorded outcome, and the same key with *different*
/// content is refused rather than silently replacing what a previous caller
/// meant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecordRequest {
    /// What the memory is about. The kernel's addressing, opaque to it — the
    /// same field, with the same meaning, as on the recall side.
    pub about: String,
    pub dimensions: Vec<MemoryDimensionSpec>,
    pub entries: Vec<MemoryEntrySpec>,
    pub relations: Vec<MemoryRelationSpec>,
    pub evidence: Vec<MemoryEvidenceSpec>,
    pub provenance: Option<MemoryProvenanceSpec>,
    /// Stable across retries of the same logical record.
    pub idempotency_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_request_survives_the_wire() {
        let request = MemoryRecordRequest {
            about: "project:checkout".to_string(),
            dimensions: vec![MemoryDimensionSpec {
                id: "timeline:work".to_string(),
                kind: "timeline".to_string(),
                title: None,
            }],
            entries: vec![MemoryEntrySpec {
                id: "observation:first".to_string(),
                kind: "observation".to_string(),
                text: "The retries began after the deploy.".to_string(),
                coordinates: vec![MemoryCoordinateSpec {
                    dimension: "timeline".to_string(),
                    scope_id: "timeline:work".to_string(),
                    occurred_at: Some("2026-08-04T10:00:00Z".to_string()),
                    sequence: Some(1),
                }],
                metadata: BTreeMap::new(),
            }],
            relations: vec![MemoryRelationSpec {
                from: "evidence:first".to_string(),
                to: "observation:first".to_string(),
                rel: "supports".to_string(),
                semantic_class: "evidential".to_string(),
                why: Some("the reading backs the observation".to_string()),
                confidence: Some("high".to_string()),
                sequence: None,
            }],
            evidence: vec![MemoryEvidenceSpec {
                id: "evidence:first".to_string(),
                supports: vec!["observation:first".to_string()],
                text: "Latency doubled at 10:00.".to_string(),
                source: Some("synthetic-fixture".to_string()),
                time: Some("2026-08-04T10:00:00Z".to_string()),
                metadata: BTreeMap::new(),
            }],
            provenance: Some(MemoryProvenanceSpec {
                source_kind: "projection".to_string(),
                source_agent: "contract-test".to_string(),
                observed_at: "2026-08-04T10:00:05Z".to_string(),
                correlation_id: Some("corr-1".to_string()),
                causation_id: None,
            }),
            idempotency_key: "record:contract-test:1".to_string(),
        };
        let bytes = serde_json::to_vec(&request).expect("serializes");
        assert_eq!(
            serde_json::from_slice::<MemoryRecordRequest>(&bytes).expect("deserializes"),
            request
        );
    }
}
