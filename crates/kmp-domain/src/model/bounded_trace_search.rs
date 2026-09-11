use super::{
    trace_candidate_frontier::TraceCandidateFrontier,
    trace_temporal_admission::TraceTemporalAdmission,
};
use crate::{
    NodeRelationProjection, PortError, RelationDirection, RelationSemanticClass,
    TraceSearchRequest, TraceSearchResult, TraceSearchStop, TraceSnapshotReader,
};
use std::collections::BTreeMap;

/// Unit-cost, bounded candidate discovery. No semantic sufficiency or optimality claim.
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
    let mut moves = request
        .follow
        .iter()
        .map(|s| (s.direction, Some(s.relation.as_str())))
        .collect::<Vec<_>>();
    moves.sort_by_key(|(direction, rel)| (*rel, *direction == RelationDirection::Incoming));
    if moves.is_empty() {
        moves.push((request.direction, None));
    }
    let mut result = TraceSearchResult {
        material: None,
        from: request.from.clone(),
        follow: request.follow.clone(),
        paths_per_target: request.paths_per_target,
        considered_states: 0,
        incomplete_targets: vec![],
        routes: vec![],
        relations: vec![],
        unreached: vec![],
        stop: if root_admitted {
            TraceSearchStop::FrontierExhausted
        } else {
            TraceSearchStop::SourceOutsideSelection
        },
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
    let mut frontier = TraceCandidateFrontier::new(request, root_admitted);
    let mut cache = BTreeMap::<String, Vec<NodeRelationProjection>>::new();
    let mut depth_cut = false;
    'search: while admission.budget.stop.is_none() && frontier.stop.is_none() {
        let Some((state, node, depth)) = frontier.pop() else {
            if depth_cut {
                result.stop = TraceSearchStop::DepthBudget;
            }
            break;
        };
        if depth == request.limits.depth {
            // No extra probe: an unexpanded depth boundary is not a known leaf.
            depth_cut = true;
            continue;
        }
        if let Some(edges) = cache.get(&node) {
            for edge in edges {
                if !frontier.extend(state, neighbor(edge, &node), edge) {
                    break 'search;
                }
            }
            continue;
        }
        result.expanded_nodes += 1;
        let mut eligible = Vec::new();
        let mut has_eligible = false;
        for &(direction, relation_type) in &moves {
            let mut after = None;
            loop {
                let Some(page) = admission
                    .budget
                    .page(&node, direction, after, relation_type)?
                else {
                    break 'search;
                };
                for edge in page.edges {
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
                    let next = neighbor(&edge, &node);
                    if !admission.admits(next)? {
                        if admission.budget.stop.is_some() {
                            break 'search;
                        }
                        continue;
                    }
                    has_eligible = true;
                    if !frontier.extend(state, next, &edge) {
                        break 'search;
                    }
                    if request.paths_per_target > 1 {
                        eligible.push(edge);
                    }
                }
                if page.exhausted {
                    break;
                }
                after = page.next;
            }
        }
        if !has_eligible {
            result.leaves += 1;
        }
        if request.paths_per_target > 1 {
            cache.insert(node, eligible);
        }
    }
    result.discovered_nodes = admission.budget.refs.len() as u32;
    result.scanned_edges = admission.budget.scanned;
    result.coordinate_rows = admission.budget.coordinate_rows;
    if let Some(stop) = frontier.stop.or(admission.budget.stop) {
        result.stop = stop;
    }
    frontier.finish(&mut result);
    if !request.temporal.is_frontier() {
        result.clock_unknown_edges = result
            .relations
            .iter()
            .enumerate()
            .filter(|(_, edge)| !admission.window().relation_clock_known(&edge.explanation))
            .map(|(index, _)| index as u32)
            .collect();
    }
    if let Some(policy) = &request.select {
        result.material = Some(
            crate::select_trace_material(&result, &request.targets, policy)
                .map_err(|e| PortError::InvalidState(e.to_string()))?,
        );
    }
    Ok(result)
}

fn neighbor<'a>(edge: &'a NodeRelationProjection, node: &str) -> &'a str {
    if edge.source_node_id == node {
        &edge.target_node_id
    } else {
        &edge.source_node_id
    }
}
