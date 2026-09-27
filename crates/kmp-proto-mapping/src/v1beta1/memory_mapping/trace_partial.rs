use kmp_domain::ContextPathSearch;
use kmp_proto::v1beta1::TraceSearchSelection;

/// The report of a single-destination trace whose bounded bidirectional
/// search stopped on a work limit before the two ends met (DESIGN L7). It
/// is not a finding of absence, so it names the limit and the destination
/// it left unreached; a search that exhausted a side keeps the answer it
/// always had.
pub(super) struct TracePartial;

impl TracePartial {
    pub(super) fn selection(
        search: &ContextPathSearch,
        from: &str,
        to: &str,
    ) -> Option<TraceSearchSelection> {
        search.is_partial().then(|| TraceSearchSelection {
            stop_reason: search.stop.as_str().into(),
            discovered_nodes: search.discovered_nodes,
            scanned_edges: search.scanned_edges,
            expanded_nodes: search.expanded_nodes,
            unreached_targets: vec![to.to_string()],
            direction: "bidirectional".into(),
            from: from.to_string(),
            paths_per_target: 1,
            ..Default::default()
        })
    }

    pub(super) fn warning(search: &ContextPathSearch, from: &str, to: &str) -> String {
        format!(
            "trace search stopped at {} before reaching `{to}` from `{from}`; this is not \
             proof that no directed path exists. search.widen repeats the same trace over \
             every relation with the largest allowance",
            search.stop.as_str()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_domain::TraceSearchStop;

    fn search(stop: TraceSearchStop) -> ContextPathSearch {
        ContextPathSearch {
            path: None,
            stop,
            discovered_nodes: 256,
            scanned_edges: 700,
            expanded_nodes: 90,
        }
    }

    #[test]
    fn only_a_budget_stop_is_reported_as_partial() {
        let partial = TracePartial::selection(&search(TraceSearchStop::NodeBudget), "a", "b")
            .expect("partial");
        assert_eq!(partial.stop_reason, "node_budget");
        assert_eq!(partial.unreached_targets, ["b"]);
        assert_eq!(partial.direction, "bidirectional");
        assert_eq!(partial.discovered_nodes, 256);
        assert!(
            TracePartial::selection(&search(TraceSearchStop::FrontierExhausted), "a", "b")
                .is_none()
        );
        assert!(
            TracePartial::warning(&search(TraceSearchStop::DepthBudget), "a", "b")
                .contains("depth_budget")
        );
    }
}
