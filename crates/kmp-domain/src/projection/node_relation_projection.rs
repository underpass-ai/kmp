use crate::RelationExplanation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRelationProjection {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub explanation: RelationExplanation,
}
