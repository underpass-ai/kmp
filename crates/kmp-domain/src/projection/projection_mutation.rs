use crate::{NodeDetailProjection, NodeProjection, NodeRelationProjection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionMutation {
    EnsureNode(NodeProjection),
    UpsertNode(NodeProjection),
    UpdateNodeStatus {
        node_id: String,
        status: String,
    },
    UpsertNodeRelation(Box<NodeRelationProjection>),
    /// Drops one relation by its identity. Removing an absent relation is
    /// not an error, and the nodes at either end stay. A relabel taking a
    /// label off an entry is what emits this: the `contains_entry` edge
    /// goes, the entry and the dimension remain.
    RemoveNodeRelation {
        source_node_id: String,
        target_node_id: String,
        relation_type: String,
    },
    UpsertNodeDetail(NodeDetailProjection),
}
