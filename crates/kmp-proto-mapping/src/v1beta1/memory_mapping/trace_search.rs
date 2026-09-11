use super::scalars::{ProtoMappingResult, invalid_argument};
use super::{
    bundle_views::memory_relation_from_bundle_relationship,
    read_selection_fingerprint::ReadSelectionFingerprint,
};
use kmp_application::TracePageRequest;
use kmp_domain::{
    BundleRelationship, RelationDirection, TraceSearchLimits, TraceSearchRequest, TraceSearchResult,
};
use kmp_proto::v1beta1::{PageInfo, TraceRequest, TraceResponse, TraceRoute, TraceSearchSelection};
use std::collections::BTreeSet;

pub fn trace_search_request_from_proto(
    request: &TraceRequest,
) -> ProtoMappingResult<Option<TraceSearchRequest>> {
    if request.search.is_none() && request.targets.is_empty() {
        return Ok(None);
    }
    let options = request.search.clone().unwrap_or_default();
    let defaults = TraceSearchLimits::default();
    let direction = match options.direction.as_str() {
        "" | "outgoing" => RelationDirection::Outgoing,
        "incoming" => RelationDirection::Incoming,
        _ => {
            return Err(invalid_argument(
                "search.direction must be outgoing or incoming",
            ));
        }
    };
    let query = TraceSearchRequest {
        about: request.about.clone(),
        from: request.from.clone(),
        targets: if request.targets.is_empty() {
            BTreeSet::from([request.to.clone()])
        } else {
            request.targets.iter().cloned().collect()
        },
        direction,
        relations: options.relations.into_iter().collect(),
        limits: TraceSearchLimits {
            nodes: if options.max_nodes == 0 {
                defaults.nodes
            } else {
                options.max_nodes
            },
            edges: if options.max_edges == 0 {
                defaults.edges
            } else {
                options.max_edges
            },
            depth: if options.max_depth == 0 {
                defaults.depth
            } else {
                options.max_depth
            },
        },
    };
    query
        .validate()
        .map_err(|e| invalid_argument(e.to_string()))?;
    Ok(Some(query))
}

pub fn trace_search_response_from_result(
    result: TraceSearchResult,
    direction: RelationDirection,
    page: TracePageRequest,
) -> TraceResponse {
    let mut response = TraceResponse {
        summary: format!("Reached {} of {} explicit destinations; {}. Routes are declared links, not a complete answer proof.",
            result.routes.len(), result.routes.len() + result.unreached.len(), result.stop.as_str()),
        trace: result.relations.iter().map(|edge| memory_relation_from_bundle_relationship(&BundleRelationship::from_projection(edge))).collect(),
        routes: result.routes.into_iter().map(|r| TraceRoute { target: r.target, edge_indexes: r.edge_indexes }).collect(),
        search: Some(TraceSearchSelection {
            stop_reason: result.stop.as_str().into(), discovered_nodes: result.discovered_nodes,
            scanned_edges: result.scanned_edges, expanded_nodes: result.expanded_nodes,
            leaves: result.leaves, unreached_targets: result.unreached,
            direction: match direction { RelationDirection::Outgoing => "outgoing", RelationDirection::Incoming => "incoming" }.into(),
        }),
        warnings: vec!["Bounded trace reads current same-about entries and source-backed non-structural links. It does not apply a historical cut, return entry bodies or infer missing proof requirements.".into()],
        ..Default::default()
    };
    response.selection_fingerprint = ReadSelectionFingerprint::trace_search(&response);
    let total = response.trace.len();
    let offset = page.offset().min(total);
    let end = offset.saturating_add(page.entries_or_default()).min(total);
    response.trace = response.trace[offset..end].to_vec();
    response.page = Some(PageInfo {
        returned: (end - offset) as u32,
        total: total as u32,
        has_more: end < total,
        next_cursor: if end < total {
            end.to_string()
        } else {
            String::new()
        },
    });
    response
}

#[cfg(test)]
#[path = "trace_search_tests.rs"]
mod tests;
