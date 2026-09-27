use std::collections::{HashMap, HashSet, VecDeque};

use super::path_search_side::PathSearchSide;
use crate::{
    ContextPathSearch, PortError, RelationDirection, TraceSearchLimits, TraceSearchStop,
    TraceSnapshotReader,
};

/// Bounded bidirectional breadth-first search for one directed path from
/// `root` to `target` over every stored relation (DESIGN L7).
///
/// Each turn expands one whole level of the side with the smaller frontier:
/// outgoing rows from the root, incoming rows into the target. The search
/// stops when the sides meet, when either side runs out (no directed path),
/// or when a work limit would be exceeded: `nodes` distinct refs, `edges`
/// adjacency rows, `depth` hops between the two ends, `states` frontier
/// insertions on both sides.
///
/// When the sides meet at the end of a complete level, the returned path is
/// exactly the one the unbounded breadth-first walk from the root finds:
/// every shortest path's relations were read by one side or the other, and
/// replaying the root-first walk over the shortest-path subgraph visits its
/// nodes in the same relative order. Cost is O(b^{d/2}) rows instead of
/// everything the root reaches.
pub fn bidirectional_path_search(
    reader: &impl TraceSnapshotReader,
    root: &str,
    target: &str,
    limits: TraceSearchLimits,
) -> Result<ContextPathSearch, PortError> {
    let mut result = ContextPathSearch {
        path: None,
        stop: TraceSearchStop::FrontierExhausted,
        discovered_nodes: if root == target { 1 } else { 2 },
        scanned_edges: 0,
        expanded_nodes: 0,
    };
    if root == target {
        result.path = Some(vec![root.to_string()]);
        result.stop = TraceSearchStop::TargetsReached;
        return Ok(result);
    }
    let mut forward = PathSearchSide::new(root, RelationDirection::Outgoing);
    let mut backward = PathSearchSide::new(target, RelationDirection::Incoming);
    let mut states = 2u32;
    let mut met = false;
    'search: loop {
        if forward.frontier.is_empty() || backward.frontier.is_empty() {
            result.stop = TraceSearchStop::FrontierExhausted;
            break;
        }
        if forward.depth + backward.depth + 1 > limits.depth {
            result.stop = TraceSearchStop::DepthBudget;
            break;
        }
        let (side, other) = if forward.frontier.len() <= backward.frontier.len() {
            (&mut forward, &backward)
        } else {
            (&mut backward, &forward)
        };
        let level = std::mem::take(&mut side.frontier);
        let mut next = Vec::new();
        for node in level {
            let remaining = limits.edges.saturating_sub(result.scanned_edges);
            let neighbours =
                reader.neighbor_ids(&node, side.direction, remaining.saturating_add(1))?;
            result.expanded_nodes += 1;
            if neighbours.len() as u64 > u64::from(remaining) {
                result.scanned_edges = limits.edges;
                result.stop = TraceSearchStop::EdgeBudget;
                break 'search;
            }
            result.scanned_edges += neighbours.len() as u32;
            for neighbour in &neighbours {
                if side.knows(neighbour) {
                    continue;
                }
                let fresh = !other.knows(neighbour);
                if fresh && result.discovered_nodes >= limits.nodes {
                    result.stop = TraceSearchStop::NodeBudget;
                    break 'search;
                }
                if states >= limits.states {
                    result.stop = TraceSearchStop::StateBudget;
                    break 'search;
                }
                states += 1;
                result.discovered_nodes += u32::from(fresh);
                met |= !fresh;
                side.distance.insert(neighbour.clone(), side.depth + 1);
                next.push(neighbour.clone());
            }
            side.read.push((node, neighbours));
        }
        side.depth += 1;
        side.frontier = next;
        if met {
            result.stop = TraceSearchStop::TargetsReached;
            break;
        }
    }
    if met {
        result.path = replay(&forward, &backward, root, target);
        if result.path.is_some() {
            result.stop = TraceSearchStop::TargetsReached;
        }
    }
    Ok(result)
}

/// The root-first breadth-first walk over the relations both sides read,
/// restricted to shortest root-to-target paths, neighbours in key order.
fn replay(
    forward: &PathSearchSide,
    backward: &PathSearchSide,
    root: &str,
    target: &str,
) -> Option<Vec<String>> {
    let mut outgoing = HashMap::<&str, Vec<&str>>::new();
    let mut incoming = HashMap::<&str, Vec<&str>>::new();
    for (node, neighbours) in &forward.read {
        for next in neighbours {
            outgoing.entry(node).or_default().push(next);
            incoming.entry(next).or_default().push(node);
        }
    }
    for (node, neighbours) in &backward.read {
        for previous in neighbours {
            outgoing.entry(previous).or_default().push(node);
            incoming.entry(node).or_default().push(previous);
        }
    }
    let from_root = distances(&outgoing, root);
    let to_target = distances(&incoming, target);
    let length = *from_root.get(target)?;
    let on_shortest = |node: &str| {
        from_root
            .get(node)
            .zip(to_target.get(node))
            .is_some_and(|(f, b)| f + b == length)
    };
    let mut predecessor = HashMap::<&str, &str>::new();
    let mut visited = HashSet::from([root]);
    let mut queue = VecDeque::from([root]);
    while let Some(node) = queue.pop_front() {
        let depth = from_root[node];
        let mut next = outgoing
            .get(node)
            .into_iter()
            .flatten()
            .copied()
            .filter(|next| on_shortest(next) && from_root[next] == depth + 1)
            .collect::<Vec<_>>();
        next.sort_unstable();
        next.dedup();
        for next in next {
            if !visited.insert(next) {
                continue;
            }
            predecessor.insert(next, node);
            if next == target {
                let mut path = vec![target.to_string()];
                let mut at = target;
                while let Some(previous) = predecessor.get(at) {
                    path.push((*previous).to_string());
                    at = previous;
                }
                path.reverse();
                return Some(path);
            }
            queue.push_back(next);
        }
    }
    None
}

fn distances<'a>(
    adjacency: &HashMap<&'a str, Vec<&'a str>>,
    start: &'a str,
) -> HashMap<&'a str, u32> {
    let mut distance = HashMap::from([(start, 0u32)]);
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        let here = distance[node];
        for next in adjacency.get(node).into_iter().flatten() {
            if !distance.contains_key(next) {
                distance.insert(next, here + 1);
                queue.push_back(next);
            }
        }
    }
    distance
}

#[cfg(test)]
#[path = "bidirectional_path_search_tests.rs"]
mod tests;
