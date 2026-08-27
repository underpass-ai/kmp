use crate::{NodeDetailProjection, NodeProjection, NodeRelationProjection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionMutation {
    EnsureNode(NodeProjection),
    UpsertNode(NodeProjection),
    UpdateNodeStatus { node_id: String, status: String },
    UpsertNodeRelation(Box<NodeRelationProjection>),
    UpsertNodeDetail(NodeDetailProjection),
}
