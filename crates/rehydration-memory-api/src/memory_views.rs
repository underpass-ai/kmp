use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One node of a recalled bundle, as a consumer sees it.
///
/// A projection, never the aggregate. The kernel's node gains fields as its
/// domain needs them; a consumer that read it directly would inherit each one
/// as a contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryNodeView {
    pub node_id: String,
    pub node_kind: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub labels: Vec<String>,
    pub properties: BTreeMap<String, String>,
}

/// One relationship between recalled nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRelationshipView {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relationship_type: String,
    /// The stated reason for the link, when its recorder gave one. A consumer
    /// surfacing a conflict quotes this instead of inventing a rationale.
    pub why: Option<String>,
    /// The evidence the link cites, when its recorder cited any.
    pub evidence: Option<String>,
}

/// Full detail for one node, with the hash that makes it citable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDetailView {
    pub node_id: String,
    pub detail: String,
    pub content_hash: String,
    pub revision: u64,
}

/// How well the rendering served its budget.
///
/// The kernel's own account of the trade it made: how much raw memory the
/// rendering stands in for, and what was kept against what was let go. Ratios
/// are `f64`, which is why this view — and everything holding it — is
/// `PartialEq` and not `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryQualityView {
    /// Tokens the un-rendered memory would have cost.
    pub raw_equivalent_tokens: u32,
    pub compression_ratio: f64,
    pub causal_density: f64,
    pub noise_ratio: f64,
    pub detail_coverage: f64,
}

/// The rendered context, ready for a reader.
///
/// `content_hash` covers `content` exactly: a consumer that hands the text to
/// a model can verify the model received what the kernel rendered, and cite
/// the hash instead of quoting itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderedMemoryView {
    pub content: String,
    pub content_hash: String,
    pub token_count: u32,
    pub quality: MemoryQualityView,
}

/// What one recall returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecallView {
    /// The about that was asked for, echoed back unchanged.
    pub about: String,
    /// The revision of the memory this recall was answered from. Two recalls
    /// answering with the same `revision` and `content_hash` saw one
    /// snapshot; a consumer combining recalls checks this instead of hoping.
    pub revision: u64,
    /// Hash of the memory state behind this recall — the snapshot's identity,
    /// distinct from `rendered.content_hash`, which covers the rendered text.
    pub content_hash: String,
    pub root: MemoryNodeView,
    pub neighbors: Vec<MemoryNodeView>,
    pub relationships: Vec<MemoryRelationshipView>,
    pub details: Vec<MemoryDetailView>,
    pub rendered: RenderedMemoryView,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str) -> MemoryNodeView {
        MemoryNodeView {
            node_id: id.to_string(),
            node_kind: "decision".to_string(),
            title: "First decision".to_string(),
            summary: "What was decided".to_string(),
            status: "active".to_string(),
            labels: vec!["timeline".to_string()],
            properties: BTreeMap::new(),
        }
    }

    #[test]
    fn a_recall_survives_the_wire() {
        let view = MemoryRecallView {
            about: "project:checkout".to_string(),
            revision: 7,
            content_hash: "snapshot-hash".to_string(),
            root: node("about:project:checkout"),
            neighbors: vec![node("decision:first")],
            relationships: vec![MemoryRelationshipView {
                source_node_id: "about:project:checkout".to_string(),
                target_node_id: "decision:first".to_string(),
                relationship_type: "contains".to_string(),
                why: Some("the about holds its decisions".to_string()),
                evidence: None,
            }],
            details: vec![MemoryDetailView {
                node_id: "decision:first".to_string(),
                detail: "The full text.".to_string(),
                content_hash: "abc".to_string(),
                revision: 1,
            }],
            rendered: RenderedMemoryView {
                content: "# Context".to_string(),
                content_hash: "def".to_string(),
                token_count: 3,
                quality: MemoryQualityView {
                    raw_equivalent_tokens: 12,
                    compression_ratio: 4.0,
                    causal_density: 0.5,
                    noise_ratio: 0.1,
                    detail_coverage: 0.9,
                },
            },
        };
        let bytes = serde_json::to_vec(&view).expect("serializes");
        assert_eq!(
            serde_json::from_slice::<MemoryRecallView>(&bytes).expect("deserializes"),
            view
        );
    }
}
