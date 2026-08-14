#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDetailProjection {
    pub node_id: String,
    pub detail: String,
    pub content_hash: String,
    pub revision: u64,
}
