use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTimingBreakdown {
    pub graph_load: Duration,
    pub detail_load: Duration,
    pub bundle_assembly: Duration,
    pub role_count: usize,
    pub batch_size: usize,
}

impl QueryTimingBreakdown {
    pub fn not_found(graph_load: Duration) -> Self {
        Self {
            graph_load,
            detail_load: Duration::ZERO,
            bundle_assembly: Duration::ZERO,
            role_count: 0,
            batch_size: 0,
        }
    }
}
