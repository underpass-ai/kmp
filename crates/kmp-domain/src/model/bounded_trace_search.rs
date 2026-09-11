use super::{
    trace_candidate_frontier::TraceCandidateFrontier, trace_node_expansion::TraceNodeExpansion,
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
        routing: None,
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
    let mut frontier =
        TraceCandidateFrontier::new(request, root_admitted, admission.preferred(&request.from));
    let focused = request.dimensions.preferred.is_some();
    let alternatives = request.paths_per_target > 1;
    let rows = if focused { 4 } else { 32 };
    let mut cache = BTreeMap::<String, TraceNodeExpansion>::new();
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
        let (mut offset, resumed) = frontier.begin_expansion(state);
        admission.routing.resumed_states += u32::from(resumed);
        let mut expansion = cache.remove(&node).unwrap_or_else(|| {
            result.expanded_nodes += 1;
            TraceNodeExpansion::default()
        });
        loop {
            let end = if focused {
                offset
                    .saturating_add(rows as usize)
                    .min(expansion.eligible.len())
            } else {
                expansion.eligible.len()
            };
            let replayed = end > offset;
            for edge in &expansion.eligible[offset..end] {
                if !frontier.extend(
                    state,
                    neighbor(edge, &node),
                    edge,
                    admission.preferred(neighbor(edge, &node)),
                ) {
                    break 'search;
                }
            }
            offset = end;
            if (focused && replayed) || expansion.complete {
                break;
            }
            let Some(edges) = expansion.page(&node, &moves, &mut admission.budget, rows)? else {
                break 'search;
            };
            for edge in edges {
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
                expansion.has_eligible = true;
                if !frontier.extend(state, next, &edge, admission.preferred(next)) {
                    break 'search;
                }
                if alternatives {
                    expansion.eligible.push(edge);
                    offset += 1;
                }
            }
            if expansion.complete && !expansion.has_eligible {
                result.leaves += 1;
            }
            if focused || expansion.complete {
                break;
            }
        }
        if !expansion.complete || offset < expansion.eligible.len() {
            frontier.resume(state, offset, admission.preferred(&node));
        }
        if alternatives || !expansion.complete {
            cache.insert(node, expansion);
        }
    }
    result.discovered_nodes = admission.budget.refs.len() as u32;
    result.scanned_edges = admission.budget.scanned;
    result.coordinate_rows = admission.budget.coordinate_rows;
    if let Some(stop) = frontier.stop.or(admission.budget.stop) {
        result.stop = stop;
    }
    if request.dimensions.is_active() {
        admission.routing.adjacency_pages = admission.budget.adjacency_pages;
        admission.routing.coordinate_pages = admission.budget.coordinate_pages;
        result.routing = Some(admission.routing.clone());
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
