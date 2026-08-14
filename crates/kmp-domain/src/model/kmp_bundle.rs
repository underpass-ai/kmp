use std::collections::BTreeSet;

use crate::{
    BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId, DomainError,
    KmpStats, Role,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KmpBundle {
    root_node_id: CaseId,
    role: Role,
    root_node: BundleNode,
    neighbor_nodes: Vec<BundleNode>,
    relationships: Vec<BundleRelationship>,
    node_details: Vec<BundleNodeDetail>,
    stats: KmpStats,
    metadata: BundleMetadata,
}

impl KmpBundle {
    pub fn new(
        root_node_id: CaseId,
        role: Role,
        root_node: BundleNode,
        neighbor_nodes: Vec<BundleNode>,
        relationships: Vec<BundleRelationship>,
        node_details: Vec<BundleNodeDetail>,
        metadata: BundleMetadata,
    ) -> Result<Self, DomainError> {
        if root_node.node_id() != root_node_id.as_str() {
            return Err(DomainError::InvalidState(format!(
                "root node id `{}` does not match bundle root `{}`",
                root_node.node_id(),
                root_node_id.as_str()
            )));
        }

        let mut node_ids = BTreeSet::new();
        node_ids.insert(root_node.node_id().to_string());
        for node in &neighbor_nodes {
            if !node_ids.insert(node.node_id().to_string()) {
                return Err(DomainError::InvalidState(format!(
                    "duplicate bundle node `{}`",
                    node.node_id()
                )));
            }
        }

        for relationship in &relationships {
            if !node_ids.contains(relationship.source_node_id())
                || !node_ids.contains(relationship.target_node_id())
            {
                return Err(DomainError::InvalidState(format!(
                    "relationship `{}` -> `{}` references nodes outside the bundle",
                    relationship.source_node_id(),
                    relationship.target_node_id()
                )));
            }
        }

        for detail in &node_details {
            if !node_ids.contains(detail.node_id()) {
                return Err(DomainError::InvalidState(format!(
                    "node detail `{}` does not belong to this bundle",
                    detail.node_id()
                )));
            }
        }

        let stats = KmpStats::new(
            node_ids.len() as u32,
            relationships.len() as u32,
            node_details.len() as u32,
        );

        Ok(Self {
            root_node_id,
            role,
            root_node,
            neighbor_nodes,
            relationships,
            node_details,
            stats,
            metadata,
        })
    }

    pub fn root_node_id(&self) -> &CaseId {
        &self.root_node_id
    }

    pub fn role(&self) -> &Role {
        &self.role
    }

    pub fn root_node(&self) -> &BundleNode {
        &self.root_node
    }

    pub fn neighbor_nodes(&self) -> &[BundleNode] {
        &self.neighbor_nodes
    }

    pub fn relationships(&self) -> &[BundleRelationship] {
        &self.relationships
    }

    pub fn node_details(&self) -> &[BundleNodeDetail] {
        &self.node_details
    }

    pub fn stats(&self) -> &KmpStats {
        &self.stats
    }

    pub fn metadata(&self) -> &BundleMetadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId, DomainError,
        RelationExplanation, RelationSemanticClass, Role,
    };

    use super::KmpBundle;

    #[test]
    fn bundle_rejects_mismatched_root_node_ids() {
        let error = KmpBundle::new(
            CaseId::new("node-123").expect("case id is valid"),
            Role::new("developer").expect("role is valid"),
            BundleNode::new(
                "node-999",
                "capability",
                "Node 999",
                "summary",
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            ),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            BundleMetadata::initial("0.1.0"),
        )
        .expect_err("mismatched root ids must fail");

        assert_eq!(
            error,
            DomainError::InvalidState(
                "root node id `node-999` does not match bundle root `node-123`".to_string()
            )
        );
    }

    #[test]
    fn bundle_rejects_duplicate_neighbor_nodes() {
        let error = KmpBundle::new(
            CaseId::new("node-123").expect("case id is valid"),
            Role::new("developer").expect("role is valid"),
            root_node("node-123"),
            vec![neighbor_node("node-456"), neighbor_node("node-456")],
            Vec::new(),
            Vec::new(),
            BundleMetadata::initial("0.1.0"),
        )
        .expect_err("duplicate neighbors must fail");

        assert_eq!(
            error,
            DomainError::InvalidState("duplicate bundle node `node-456`".to_string())
        );
    }

    #[test]
    fn bundle_rejects_details_for_missing_nodes() {
        let error = KmpBundle::new(
            CaseId::new("node-123").expect("case id is valid"),
            Role::new("developer").expect("role is valid"),
            root_node("node-123"),
            vec![neighbor_node("node-456")],
            vec![BundleRelationship::new(
                "node-123",
                "node-456",
                "RELATES_TO",
                RelationExplanation::new(RelationSemanticClass::Structural),
            )],
            vec![BundleNodeDetail::new("node-789", "detail", "hash-1", 1)],
            BundleMetadata::initial("0.1.0"),
        )
        .expect_err("orphan details must fail");

        assert_eq!(
            error,
            DomainError::InvalidState(
                "node detail `node-789` does not belong to this bundle".to_string()
            )
        );
    }

    fn root_node(node_id: &str) -> BundleNode {
        BundleNode::new(
            node_id,
            "capability",
            format!("Node {node_id}"),
            "summary",
            "ACTIVE",
            vec!["projection-node".to_string()],
            BTreeMap::new(),
        )
    }

    fn neighbor_node(node_id: &str) -> BundleNode {
        BundleNode::new(
            node_id,
            "artifact",
            format!("Node {node_id}"),
            "detail",
            "ACTIVE",
            vec!["artifact".to_string()],
            BTreeMap::new(),
        )
    }
}
