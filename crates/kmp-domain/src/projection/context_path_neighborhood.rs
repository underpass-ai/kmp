use crate::{NodeProjection, NodeRelationProjection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPathNeighborhood {
    pub root: NodeProjection,
    pub neighbors: Vec<NodeProjection>,
    pub relations: Vec<NodeRelationProjection>,
    pub path_node_ids: Vec<String>,
}
