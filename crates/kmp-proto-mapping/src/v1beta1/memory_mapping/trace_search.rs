use super::scalars::{
    ProtoMappingResult, invalid_argument, proto_temporal_axis, timestamp_from_sort_or_rfc3339,
};
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
    if request.search.is_none()
        && request.targets.is_empty()
        && request.as_of.is_none()
        && request.interval.is_none()
        && request.axis == 0
    {
        return Ok(None);
    }
    let options = request.search.clone().unwrap_or_default();
    let defaults = TraceSearchLimits::default();
    if !options.follow.is_empty()
        && (!options.direction.is_empty() || !options.relations.is_empty())
    {
        return Err(invalid_argument(
            "search.follow replaces direction and relations",
        ));
    }
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
        dimensions: kmp_domain::TraceDimensionPolicy {
            required: options
                .dimensions
                .map(|v| super::dimensions::domain_dimension_selection(Some(v)))
                .transpose()?,
            preferred: options
                .prefer_dimensions
                .map(|v| super::dimensions::domain_dimension_selection(Some(v)))
                .transpose()?,
        },
        select: options
            .select
            .map(super::trace_material::request)
            .transpose()?,
        follow: options
            .follow
            .into_iter()
            .map(|step| {
                Ok(kmp_domain::TraceRelationStep {
                    relation: kmp_domain::MemoryRelationType::new(step.rel)
                        .map_err(|e| invalid_argument(e.to_string()))?,
                    direction: match step.direction.as_str() {
                        "outgoing" => RelationDirection::Outgoing,
                        "incoming" => RelationDirection::Incoming,
                        _ => {
                            return Err(invalid_argument(
                                "search.follow direction must be outgoing or incoming",
                            ));
                        }
                    },
                })
            })
            .collect::<ProtoMappingResult<Vec<_>>>()?,
        paths_per_target: if options.paths_per_target == 0 {
            1
        } else {
            options.paths_per_target
        },
        direction,
        relations: options.relations.into_iter().collect(),
        temporal: super::queries::temporal_selection_from_proto(
            request.as_of.clone(),
            request.interval,
            request.axis,
        )?,
        limits: TraceSearchLimits {
            states: if options.max_states == 0 {
                defaults.states
            } else {
                options.max_states
            },
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
    let reached = result
        .routes
        .iter()
        .map(|r| &r.target)
        .collect::<BTreeSet<_>>()
        .len();
    let mut response = TraceResponse {
        summary: format!("Reached {} of {} explicit destinations; {}. Routes are declared links, not a complete answer proof.",
            reached, reached + result.unreached.len(), result.stop.as_str()),
        trace: result.relations.iter().map(|edge| memory_relation_from_bundle_relationship(&BundleRelationship::from_projection(edge))).collect(),
        routes: result.routes.into_iter().map(|r| TraceRoute { target: r.target, edge_indexes: r.edge_indexes }).collect(),
        search: Some(TraceSearchSelection {
            routing: result.routing.map(|s| kmp_proto::v1beta1::TraceRoutingStats { focused: s.focused, evaluated_entries: s.evaluated_entries, preferred_entries: s.preferred_entries, dimensional_rejections: s.dimensional_rejections, priority_pops: s.priority_pops, exploration_pops: s.exploration_pops, preferred_route_entries: s.preferred_route_entries, adjacency_pages: s.adjacency_pages, coordinate_pages: s.coordinate_pages, resumed_states: s.resumed_states }),
        material: None,
            from: result.from, paths_per_target: result.paths_per_target,
            considered_states: result.considered_states, incomplete_targets: result.incomplete_targets,
            follow: result.follow.iter().map(|s| kmp_proto::v1beta1::TraceRelationStep {
                rel: s.relation.as_str().into(), direction: direction_name(s.direction).into(),
            }).collect(),
            stop_reason: result.stop.as_str().into(), discovered_nodes: result.discovered_nodes,
            scanned_edges: result.scanned_edges, expanded_nodes: result.expanded_nodes,
            leaves: result.leaves, unreached_targets: result.unreached,
            axis: proto_temporal_axis(result.temporal_axis) as i32,
            resolved_as_of: timestamp_from_sort_or_rfc3339(result.resolved_as_of.as_deref()),
            temporal_selection_resolved: result.temporal_selection_resolved,
            coordinate_rows: result.coordinate_rows, clock_unknown_edges: result.clock_unknown_edges,
            direction: if result.follow.is_empty() { direction_name(direction) } else { "per_relation" }.into(),
        }),
        warnings: vec!["Bounded trace reads same-about entries and source-backed non-structural links on the selected clock. Missing link clocks remain unknown, not proof of their historical presence; clock_unknown_edges identifies those selected rows. Entry/source bodies, lifecycle state and missing proof requirements are not inferred.".into()],
        ..Default::default()
    };
    if let Some(material) = result.material {
        super::trace_material::project(&mut response, material);
    }
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

fn direction_name(direction: RelationDirection) -> &'static str {
    match direction {
        RelationDirection::Outgoing => "outgoing",
        RelationDirection::Incoming => "incoming",
    }
}

#[cfg(test)]
#[path = "trace_search_tests.rs"]
mod tests;
