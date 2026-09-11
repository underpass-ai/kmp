/// Hop order expressed as indexes into the complete, deduplicated relation table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceRoute {
    pub target: String,
    pub edge_indexes: Vec<u32>,
}
