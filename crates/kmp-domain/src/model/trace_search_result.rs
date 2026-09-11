use crate::{NodeRelationProjection, TraceRoute, TraceSearchStop};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceSearchResult {
    pub routes: Vec<TraceRoute>,
    pub relations: Vec<NodeRelationProjection>,
    pub unreached: Vec<String>,
    pub stop: TraceSearchStop,
    pub discovered_nodes: u32,
    pub scanned_edges: u32,
    pub expanded_nodes: u32,
    /// Existing nodes whose entire selected directed adjacency had no eligible link.
    pub leaves: u32,
    pub coordinate_rows: u32,
    pub clock_unknown_edges: Vec<u32>,
    pub temporal_axis: crate::TemporalAxis,
    pub resolved_as_of: Option<String>,
    pub temporal_selection_resolved: bool,
}
