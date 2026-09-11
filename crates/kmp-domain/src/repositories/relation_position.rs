/// Exclusive key position inside one node's directed adjacency index.
/// It is not a public continuation or a snapshot/revision token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationPosition {
    pub neighbor: String,
    pub relation: String,
}
