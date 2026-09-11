#[derive(Debug)]
pub(super) struct TracePathState {
    pub node: String,
    pub preferred_nodes: u32,
    pub depth: u32,
    pub parent: Option<usize>,
    pub edge: Option<usize>,
}
