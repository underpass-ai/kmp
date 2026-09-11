use super::trace_temporal_admission::TraceTemporalAdmission;
use crate::{
    NodeRelationProjection, PortError, RelationDirection, RelationSemanticClass, TraceRoute,
    TraceSearchRequest, TraceSearchResult, TraceSearchStop, TraceSnapshotReader,
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
    let mut admission = TraceTemporalAdmission::new(reader, request);
    if !admission.is_owned(&request.from)? {
        return Err(PortError::InvalidState(
            "trace search source must be an existing entry owned by about".into(),
        ));
    }
    let resolved = admission.resolve_cut()?;
    let root_admitted = resolved && admission.admits(&request.from)?;
    let mut result = TraceSearchResult {
        routes: vec![],
        relations: vec![],
        unreached: vec![],
        stop: TraceSearchStop::FrontierExhausted,
        discovered_nodes: 0,
        scanned_edges: 0,
        expanded_nodes: 0,
        leaves: 0,
        coordinate_rows: 0,
        clock_unknown_edges: vec![],
        temporal_axis: request.temporal.axis().unwrap_or_default(),
        resolved_as_of: admission.resolved_as_of.clone(),
        temporal_selection_resolved: resolved,
    };
    let mut predecessors = BTreeMap::<String, (String, NodeRelationProjection)>::new();
    let mut reached = BTreeSet::new();
    if root_admitted && request.targets.contains(&request.from) {
        reached.insert(request.from.clone());
    }
    let mut frontier = if root_admitted {
        VecDeque::from([(request.from.clone(), 0)])
    } else {
        VecDeque::new()
    };
    if !root_admitted {
        result.stop = crate::TraceSearchStop::SourceOutsideSelection;
    }
    let mut depth_cut = false;

    'search: while root_admitted
        && admission.budget.stop.is_none()
        && reached.len() < request.targets.len()
    {
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
            let Some(page) = admission
                .budget
                .page(&node, request.direction, after, None)?
            else {
                break 'search;
            };
            for edge in page.edges {
                let neighbor = match request.direction {
                    RelationDirection::Outgoing => &edge.target_node_id,
                    RelationDirection::Incoming => &edge.source_node_id,
                }
                .clone();
                if *edge.explanation.semantic_class() == RelationSemanticClass::Structural
                    || edge
                        .explanation
                        .rationale()
                        .is_none_or(|s| s.trim().is_empty())
                    || edge
                        .explanation
                        .evidence()
                        .is_none_or(|s| s.trim().is_empty())
                    || !admission
                        .window()
                        .admits_dependency_relation(&edge.explanation)
                    || (!request.relations.is_empty()
                        && !request.relations.contains(&edge.relation_type))
                {
                    continue;
                }
                if !admission.admits(&neighbor)? {
                    if admission.budget.stop.is_some() {
                        break 'search;
                    }
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
                    if reached.len() == request.targets.len() {
                        break 'search;
                    }
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
    result.discovered_nodes = admission.budget.refs.len() as u32;
    result.scanned_edges = admission.budget.scanned;
    result.coordinate_rows = admission.budget.coordinate_rows;
    if let Some(stop) = admission.budget.stop {
        result.stop = stop;
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
    if !request.temporal.is_frontier() {
        result.clock_unknown_edges = result
            .relations
            .iter()
            .enumerate()
            .filter(|(_, edge)| !admission.window().relation_clock_known(&edge.explanation))
            .map(|(index, _)| index as u32)
            .collect();
    }
    Ok(result)
}
