use crate::{NodeProjection, NodeRelationProjection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeNeighborhood {
    pub root: NodeProjection,
    pub neighbors: Vec<NodeProjection>,
    pub relations: Vec<NodeRelationProjection>,
}
