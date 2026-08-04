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
}

/// Full detail for one node, with the hash that makes it citable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDetailView {
    pub node_id: String,
    pub detail: String,
    pub content_hash: String,
    pub revision: u64,
}

/// The rendered context, ready for a reader.
///
/// `content_hash` covers `content` exactly: a consumer that hands the text to
/// a model can verify the model received what the kernel rendered, and cite
/// the hash instead of quoting itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedMemoryView {
    pub content: String,
    pub content_hash: String,
    pub token_count: u32,
}

/// What one recall returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecallView {
    /// The about that was asked for, echoed back unchanged.
    pub about: String,
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
            root: node("about:project:checkout"),
            neighbors: vec![node("decision:first")],
            relationships: vec![MemoryRelationshipView {
                source_node_id: "about:project:checkout".to_string(),
                target_node_id: "decision:first".to_string(),
                relationship_type: "contains".to_string(),
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
            },
        };
        let bytes = serde_json::to_vec(&view).expect("serializes");
        assert_eq!(
            serde_json::from_slice::<MemoryRecallView>(&bytes).expect("deserializes"),
            view
        );
    }
}
