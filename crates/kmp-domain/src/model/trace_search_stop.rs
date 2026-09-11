#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceSearchStop {
    TargetsReached,
    FrontierExhausted,
    NodeBudget,
    EdgeBudget,
    DepthBudget,
    SourceOutsideSelection,
}

impl TraceSearchStop {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TargetsReached => "targets_reached",
            Self::FrontierExhausted => "frontier_exhausted",
            Self::NodeBudget => "node_budget",
            Self::EdgeBudget => "edge_budget",
            Self::DepthBudget => "depth_budget",
            Self::SourceOutsideSelection => "source_outside_selection",
        }
    }
}
