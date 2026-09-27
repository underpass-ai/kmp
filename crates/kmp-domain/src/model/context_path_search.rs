use crate::TraceSearchStop;

/// What the bounded search behind a single-destination trace found and spent.
///
/// A path is a real directed walk of stored relations. A search that stopped
/// on a work budget before meeting says nothing about absence: only
/// [`TraceSearchStop::FrontierExhausted`] proves that no directed path exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPathSearch {
    /// Node ids from the root to the target, in hop order.
    pub path: Option<Vec<String>>,
    pub stop: TraceSearchStop,
    /// Distinct refs either side discovered, root and target included.
    pub discovered_nodes: u32,
    /// Adjacency rows read, both directions.
    pub scanned_edges: u32,
    /// Nodes whose adjacency was read.
    pub expanded_nodes: u32,
}

impl ContextPathSearch {
    /// The search was cut by a work budget before it met or exhausted a side.
    pub fn is_partial(&self) -> bool {
        self.path.is_none()
            && matches!(
                self.stop,
                TraceSearchStop::NodeBudget
                    | TraceSearchStop::EdgeBudget
                    | TraceSearchStop::StateBudget
                    | TraceSearchStop::DepthBudget
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(path: Option<Vec<String>>, stop: TraceSearchStop) -> ContextPathSearch {
        ContextPathSearch {
            path,
            stop,
            discovered_nodes: 2,
            scanned_edges: 0,
            expanded_nodes: 0,
        }
    }

    #[test]
    fn only_a_budget_stop_without_a_path_is_partial() {
        assert!(search(None, TraceSearchStop::NodeBudget).is_partial());
        assert!(search(None, TraceSearchStop::DepthBudget).is_partial());
        assert!(!search(None, TraceSearchStop::FrontierExhausted).is_partial());
        assert!(
            !search(
                Some(vec!["a".into(), "b".into()]),
                TraceSearchStop::EdgeBudget
            )
            .is_partial()
        );
    }
}
