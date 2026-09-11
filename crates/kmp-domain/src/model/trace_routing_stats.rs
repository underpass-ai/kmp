/// Work and exclusions of dimensional routing, not truth confidence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceRoutingStats {
    pub preferred_route_entries: Vec<u32>,
    pub focused: bool,
    pub evaluated_entries: u32,
    pub preferred_entries: u32,
    pub dimensional_rejections: u32,
    pub priority_pops: u32,
    pub exploration_pops: u32,
}
