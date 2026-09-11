#[derive(Debug)]
pub(super) struct TracePathState {
    pub node: String,
    pub depth: u32,
    pub parent: Option<usize>,
    pub edge: Option<usize>,
}
