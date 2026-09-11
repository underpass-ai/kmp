use crate::{NodeRelationProjection, RelationPosition};

/// At most the requested number of indexed relationships, in key order.
/// A full page returns a position without probing another row: it may be the
/// final page. Only exhausted=true establishes the end of this adjacency.
/// An empty page does not establish that the node exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdjacencyPage {
    pub edges: Vec<NodeRelationProjection>,
    pub next: Option<RelationPosition>,
    pub exhausted: bool,
}
