use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::bidirectional_path_search;
use crate::{
    AdjacencyPage, AdjacencyRequest, NodeProjection, PortError, RelationDirection,
    TraceSearchLimits, TraceSearchStop, TraceSnapshotReader,
};

/// Relations as the store keys them: (source, target) in key order.
#[derive(Default)]
struct Graph {
    outgoing: BTreeMap<String, BTreeSet<String>>,
    incoming: BTreeMap<String, BTreeSet<String>>,
}

impl Graph {
    fn link(&mut self, from: impl Into<String>, to: impl Into<String>) {
        let (from, to) = (from.into(), to.into());
        self.outgoing
            .entry(from.clone())
            .or_default()
            .insert(to.clone());
        self.incoming.entry(to).or_default().insert(from);
    }

    /// The unbounded walk the single-destination trace used before.
    fn legacy(&self, root: &str, target: &str) -> Option<Vec<String>> {
        let mut predecessor = BTreeMap::<String, String>::new();
        let mut visited = BTreeSet::from([root.to_string()]);
        let mut queue = VecDeque::from([root.to_string()]);
        while let Some(node) = queue.pop_front() {
            for next in self.outgoing.get(&node).into_iter().flatten() {
                if !visited.insert(next.clone()) {
                    continue;
                }
                predecessor.insert(next.clone(), node.clone());
                if next == target {
                    let mut path = vec![next.clone()];
                    while let Some(previous) = predecessor.get(path.last().expect("path")) {
                        path.push(previous.clone());
                    }
                    path.reverse();
                    return Some(path);
                }
                queue.push_back(next.clone());
            }
        }
        None
    }
}

impl TraceSnapshotReader for Graph {
    fn node(&self, _id: &str) -> Result<Option<NodeProjection>, PortError> {
        unreachable!("the path search reads structure only")
    }

    fn adjacency(&self, _request: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        unreachable!("the path search reads neighbour ids only")
    }

    fn neighbor_ids(
        &self,
        node: &str,
        direction: RelationDirection,
        limit: u32,
    ) -> Result<Vec<String>, PortError> {
        let side = match direction {
            RelationDirection::Outgoing => &self.outgoing,
            RelationDirection::Incoming => &self.incoming,
        };
        Ok(side
            .get(node)
            .into_iter()
            .flatten()
            .take(limit as usize)
            .cloned()
            .collect())
    }
}

fn wide() -> TraceSearchLimits {
    TraceSearchLimits {
        nodes: 4096,
        edges: 32768,
        depth: 1024,
        states: 32768,
    }
}

/// The scale probe's topology: every observation builds on the one before
/// and the seventh before.
fn chain(n: usize) -> Graph {
    let mut graph = Graph::default();
    for i in 1..n {
        graph.link(format!("f{i:06}"), format!("f{:06}", i - 1));
        if i >= 7 {
            graph.link(format!("f{i:06}"), format!("f{:06}", i - 7));
        }
    }
    graph
}

#[test]
fn it_finds_the_path_the_unbounded_walk_found_on_random_graphs() {
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut next = |bound: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % bound
    };
    let mut compared = 0;
    for _ in 0..60 {
        let nodes = 5 + next(60);
        let mut graph = Graph::default();
        for _ in 0..nodes * (1 + next(3)) {
            graph.link(
                format!("n{:02}", next(nodes)),
                format!("n{:02}", next(nodes)),
            );
        }
        for _ in 0..20 {
            let (root, target) = (
                format!("n{:02}", next(nodes)),
                format!("n{:02}", next(nodes)),
            );
            if root == target {
                continue;
            }
            let search = bidirectional_path_search(&graph, &root, &target, wide()).expect("read");
            let legacy = graph.legacy(&root, &target);
            assert_eq!(search.path, legacy, "{root} -> {target}");
            let expected = if legacy.is_some() {
                TraceSearchStop::TargetsReached
            } else {
                TraceSearchStop::FrontierExhausted
            };
            assert_eq!(search.stop, expected);
            compared += 1;
        }
    }
    assert!(compared > 1000, "{compared}");
}

#[test]
fn a_far_destination_costs_the_meeting_not_everything_the_root_reaches() {
    let graph = chain(20_000);
    let root = "f019999";
    let target = "f019950";
    let search = bidirectional_path_search(&graph, root, target, TraceSearchLimits::default())
        .expect("read");
    assert_eq!(search.path, graph.legacy(root, target));
    assert_eq!(search.stop, TraceSearchStop::TargetsReached);
    assert!(search.scanned_edges < 200, "{search:?}");
}

#[test]
fn a_destination_beyond_the_allowance_is_partial_not_absent() {
    let graph = chain(20_000);
    let search =
        bidirectional_path_search(&graph, "f019999", "f000000", TraceSearchLimits::default())
            .expect("read");
    assert_eq!(search.path, None);
    assert!(search.is_partial(), "{search:?}");
    assert!(search.discovered_nodes <= 256 && search.scanned_edges <= 2048);
}

#[test]
fn each_work_limit_stops_the_search_with_its_own_reason() {
    let graph = chain(2_000);
    let limit = |nodes, edges, depth, states| TraceSearchLimits {
        nodes,
        edges,
        depth,
        states,
    };
    let stop = |limits| {
        bidirectional_path_search(&graph, "f001999", "f000000", limits)
            .expect("read")
            .stop
    };
    assert_eq!(
        stop(limit(8, 32768, 1024, 32768)),
        TraceSearchStop::NodeBudget
    );
    assert_eq!(
        stop(limit(4096, 8, 1024, 32768)),
        TraceSearchStop::EdgeBudget
    );
    assert_eq!(
        stop(limit(4096, 32768, 3, 32768)),
        TraceSearchStop::DepthBudget
    );
    assert_eq!(
        stop(limit(4096, 32768, 1024, 8)),
        TraceSearchStop::StateBudget
    );
    let widest = bidirectional_path_search(&graph, "f001999", "f000000", wide()).expect("read");
    assert_eq!(
        widest.path,
        graph.legacy("f001999", "f000000"),
        "{widest:?}"
    );
}

#[test]
fn an_unreachable_destination_exhausts_a_side() {
    let mut graph = chain(50);
    graph.link("island", "f000010");
    let search = bidirectional_path_search(&graph, "f000010", "island", wide()).expect("read");
    assert_eq!(search.path, None);
    assert_eq!(search.stop, TraceSearchStop::FrontierExhausted);
    assert!(!search.is_partial());
    let same = bidirectional_path_search(&graph, "f000003", "f000003", wide()).expect("read");
    assert_eq!(same.path, Some(vec!["f000003".to_string()]));
}
