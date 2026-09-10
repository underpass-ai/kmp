/// An aggregate revision observed while preparing a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextRevision {
    pub root_node_id: String,
    pub role: String,
    pub revision: u64,
}
