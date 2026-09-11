use crate::{
    AdjacencyRequest, NodeRelationProjection, PortError, RelationDirection, RelationSemanticClass,
    TraceRoute, TraceSearchRequest, TraceSearchResult, TraceSearchStop, TraceSnapshotReader,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Deterministic unit-cost baseline: shared BFS, one shortest discovered path
/// per explicit destination. Not a learned ranker or a proof-completeness oracle.
pub fn bounded_trace_search(
    reader: &impl TraceSnapshotReader,
    request: &TraceSearchRequest,
) -> Result<TraceSearchResult, PortError> {
    request
        .validate()
        .map_err(|e| PortError::InvalidState(e.to_string()))?;
    let owned = |id: &str| -> Result<bool, PortError> {
        Ok(reader.node(id)?.is_some_and(|n| {
            n.properties.get("memory_about") == Some(&request.about)
                && n.labels.iter().any(|l| l == "entry")
        }))
    };
    if !owned(&request.from)? {
        return Err(PortError::InvalidState(
            "trace search source must be an existing entry owned by about".into(),
        ));
    }
    let mut result = TraceSearchResult {
        routes: vec![],
        relations: vec![],
        unreached: vec![],
        stop: TraceSearchStop::FrontierExhausted,
        discovered_nodes: 1,
        scanned_edges: 0,
        expanded_nodes: 0,
        leaves: 0,
    };
    let mut discovered = BTreeSet::from([request.from.clone()]);
    let mut admitted = BTreeMap::from([(request.from.clone(), true)]);
    let mut predecessors = BTreeMap::<String, (String, NodeRelationProjection)>::new();
    let mut reached = BTreeSet::new();
    if request.targets.contains(&request.from) {
        reached.insert(request.from.clone());
    }
    let mut frontier = VecDeque::from([(request.from.clone(), 0)]);
    let mut depth_cut = false;

    'search: while reached.len() < request.targets.len() {
        let Some((node, depth)) = frontier.pop_front() else {
            if depth_cut {
                result.stop = TraceSearchStop::DepthBudget;
            }
            break;
        };
        if depth == request.limits.depth {
            // No probe beyond the cap: this is not a known leaf.
            depth_cut = true;
            continue;
        }
        result.expanded_nodes += 1;
        let mut after = None;
        let mut eligible = 0;
        loop {
            let nodes_left = request.limits.nodes - result.discovered_nodes;
            let edges_left = request.limits.edges - result.scanned_edges;
            if nodes_left == 0 {
                result.stop = TraceSearchStop::NodeBudget;
                break 'search;
            }
            if edges_left == 0 {
                result.stop = TraceSearchStop::EdgeBudget;
                break 'search;
            }
            // Each row can disclose one unknown endpoint. Reserve before I/O;
            // even excluded, foreign and structural endpoints consume N.
            let mut page_request =
                AdjacencyRequest::new(&node, request.direction, nodes_left.min(edges_left).min(32))
                    .map_err(|e| PortError::InvalidState(e.to_string()))?;
            if let Some(position) = after {
                page_request = page_request.with_after(position);
            }
            let page = reader.adjacency(&page_request)?;
            result.scanned_edges += page.edges.len() as u32;
            for edge in page.edges {
                let neighbor = match request.direction {
                    RelationDirection::Outgoing => &edge.target_node_id,
                    RelationDirection::Incoming => &edge.source_node_id,
                }
                .clone();
                discovered.insert(neighbor.clone());
                result.discovered_nodes = discovered.len() as u32;
                if *edge.explanation.semantic_class() == RelationSemanticClass::Structural
                    || edge
                        .explanation
                        .rationale()
                        .is_none_or(|s| s.trim().is_empty())
                    || edge
                        .explanation
                        .evidence()
                        .is_none_or(|s| s.trim().is_empty())
                    || (!request.relations.is_empty()
                        && !request.relations.contains(&edge.relation_type))
                {
                    continue;
                }
                let is_owned = match admitted.get(&neighbor) {
                    Some(value) => *value,
                    None => {
                        let value = owned(&neighbor)?;
                        admitted.insert(neighbor.clone(), value);
                        value
                    }
                };
                if !is_owned {
                    continue;
                }
                eligible += 1;
                if neighbor == request.from || predecessors.contains_key(&neighbor) {
                    continue;
                }
                predecessors.insert(neighbor.clone(), (node.clone(), edge));
                frontier.push_back((neighbor.clone(), depth + 1));
                if request.targets.contains(&neighbor) {
                    reached.insert(neighbor);
                }
            }
            if reached.len() == request.targets.len() {
                break 'search;
            }
            if page.exhausted {
                if eligible == 0 {
                    result.leaves += 1;
                }
                break;
            }
            after = page.next;
        }
    }
    if reached.len() == request.targets.len() {
        result.stop = TraceSearchStop::TargetsReached;
    }
    let mut index = BTreeMap::new();
    for target in &reached {
        let mut path = Vec::new();
        let mut current = target;
        while let Some((previous, edge)) = predecessors.get(current) {
            path.push(edge);
            current = previous;
        }
        path.reverse();
        let mut edge_indexes = Vec::new();
        for edge in path {
            let key = (
                edge.source_node_id.clone(),
                edge.target_node_id.clone(),
                edge.relation_type.clone(),
            );
            let position = *index.entry(key).or_insert_with(|| {
                let position = result.relations.len() as u32;
                result.relations.push(edge.clone());
                position
            });
            edge_indexes.push(position);
        }
        result.routes.push(TraceRoute {
            target: target.clone(),
            edge_indexes,
        });
    }
    result.unreached = request.targets.difference(&reached).cloned().collect();
    Ok(result)
}
