use super::*;
use kmp_domain::{
    NodeRelationProjection, RelationExplanation, RelationSemanticClass, TraceSearchStop,
};

fn result() -> TraceSearchResult {
    TraceSearchResult {
        from: "a".into(),
        follow: vec![],
        paths_per_target: 1,
        considered_states: 3,
        incomplete_targets: vec![],
        routes: vec![kmp_domain::TraceRoute {
            target: "c".into(),
            edge_indexes: vec![0, 1],
        }],
        relations: [("a", "b"), ("b", "c")]
            .into_iter()
            .map(|(a, b)| NodeRelationProjection {
                source_node_id: a.into(),
                target_node_id: b.into(),
                relation_type: "depends_on".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Causal)
                    .with_optional_rationale(Some(
                        "The register explicitly declares this dependency.".into(),
                    ))
                    .with_optional_evidence(Some(format!("Source {a} needs {b}."))),
            })
            .collect(),
        unreached: vec![],
        stop: TraceSearchStop::TargetsReached,
        discovered_nodes: 3,
        scanned_edges: 2,
        expanded_nodes: 2,
        leaves: 0,
        coordinate_rows: 0,
        clock_unknown_edges: vec![],
        temporal_axis: Default::default(),
        resolved_as_of: None,
        temporal_selection_resolved: true,
    }
}

#[test]
fn paging_keeps_global_route_indexes_and_hashes_unreturned_explanations_and_search_status() {
    let page = TracePageRequest {
        entries: Some(1),
        cursor: None,
    };
    let first =
        trace_search_response_from_result(result(), RelationDirection::Outgoing, page.clone());
    assert_eq!(first.trace.len(), 1);
    assert_eq!(first.routes[0].edge_indexes, [0, 1]);
    let second = trace_search_response_from_result(
        result(),
        RelationDirection::Outgoing,
        TracePageRequest {
            entries: Some(1),
            cursor: Some(1),
        },
    );
    assert_eq!(first.selection_fingerprint, second.selection_fingerprint);
    assert_eq!(second.trace[0].source_ref, "b");
    let mut changed = result();
    changed.relations[1].explanation = changed.relations[1]
        .explanation
        .clone()
        .with_optional_evidence(Some("A later source corrected the explanation.".into()));
    let changed =
        trace_search_response_from_result(changed, RelationDirection::Outgoing, page.clone());
    assert_eq!(first.trace, changed.trace);
    assert_ne!(first.selection_fingerprint, changed.selection_fingerprint);
    let mut cut = result();
    cut.stop = TraceSearchStop::EdgeBudget;
    let cut = trace_search_response_from_result(cut, RelationDirection::Outgoing, page);
    assert_ne!(first.selection_fingerprint, cut.selection_fingerprint);
}

#[test]
fn proto_mode_selection_and_direction_are_explicit() {
    let mut request = TraceRequest {
        about: "a".into(),
        from: "b".into(),
        to: "c".into(),
        ..Default::default()
    };
    assert!(
        trace_search_request_from_proto(&request)
            .expect("ordinary trace")
            .is_none()
    );
    request.targets = vec!["c".into(), "d".into()];
    let mapped = trace_search_request_from_proto(&request)
        .expect("targets")
        .expect("bounded");
    assert_eq!(mapped.targets.len(), 2);
    assert_eq!(mapped.limits.nodes, 256);
    request.search = Some(kmp_proto::v1beta1::TraceSearchOptions {
        direction: "both".into(),
        ..Default::default()
    });
    assert!(trace_search_request_from_proto(&request).is_err());
}

#[test]
fn temporal_arguments_enable_bounded_mode_and_reject_ambiguous_selections() {
    use kmp_proto::v1beta1::{TemporalAxis, TemporalCursor, TemporalInterval};
    let at = "2026-09-11T10:00:00.500Z".parse().expect("timestamp");
    let mut request = TraceRequest {
        about: "a".into(),
        from: "b".into(),
        to: "c".into(),
        as_of: Some(TemporalCursor {
            time: Some(at),
            ..Default::default()
        }),
        axis: TemporalAxis::Observed as i32,
        ..Default::default()
    };
    let mapped = trace_search_request_from_proto(&request)
        .expect("valid")
        .expect("bounded");
    assert_eq!(
        mapped.temporal.axis(),
        Some(kmp_domain::TemporalAxis::Observed)
    );
    request.interval = Some(TemporalInterval {
        end: Some(at),
        ..Default::default()
    });
    assert!(trace_search_request_from_proto(&request).is_err());
    request.as_of = None;
    request.interval = None;
    assert!(
        trace_search_request_from_proto(&request).is_err(),
        "axis alone"
    );
    request.as_of = Some(TemporalCursor {
        sequence: Some(1),
        ..Default::default()
    });
    assert!(
        trace_search_request_from_proto(&request).is_err(),
        "relative sequence"
    );
}

#[test]
fn temporal_status_and_undated_edges_are_retained_and_bound_to_the_cursor() {
    let mut selected = result();
    selected.temporal_axis = kmp_domain::TemporalAxis::Observed;
    selected.resolved_as_of = Some("2026-09-11T10:00:00.500Z".into());
    selected.coordinate_rows = 3;
    selected.clock_unknown_edges = vec![1];
    let first = trace_search_response_from_result(
        selected.clone(),
        RelationDirection::Outgoing,
        Default::default(),
    );
    let search = first.search.as_ref().expect("search");
    assert_eq!(search.resolved_as_of.expect("instant").nanos, 500_000_000);
    assert_eq!(search.clock_unknown_edges, [1]);
    assert_eq!(search.coordinate_rows, 3);
    selected.temporal_selection_resolved = false;
    let changed = trace_search_response_from_result(
        selected,
        RelationDirection::Outgoing,
        Default::default(),
    );
    assert_ne!(first.selection_fingerprint, changed.selection_fingerprint);
}

#[test]
fn alternative_contract_preserves_policy_and_rejects_ambiguous_moves() {
    use kmp_proto::v1beta1::{TraceRelationStep, TraceSearchOptions};
    let mut request = TraceRequest {
        about: "p".into(),
        from: "s".into(),
        to: "t".into(),
        search: Some(TraceSearchOptions {
            paths_per_target: 2,
            max_states: 40,
            follow: vec![TraceRelationStep {
                rel: "corrects".into(),
                direction: "incoming".into(),
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let mapped = trace_search_request_from_proto(&request)
        .expect("map")
        .expect("bounded");
    assert_eq!(mapped.paths_per_target, 2);
    assert_eq!(mapped.limits.states, 40);
    assert_eq!(mapped.follow[0].direction, RelationDirection::Incoming);
    let mut selected = result();
    selected.follow = mapped.follow;
    selected.paths_per_target = 2;
    selected.incomplete_targets = vec!["c".into()];
    let response = trace_search_response_from_result(
        selected.clone(),
        RelationDirection::Outgoing,
        Default::default(),
    );
    assert_eq!(
        response.search.as_ref().expect("meta").direction,
        "per_relation"
    );
    assert_eq!(response.search.as_ref().expect("meta").from, "a");
    selected.considered_states += 1;
    let changed = trace_search_response_from_result(
        selected,
        RelationDirection::Outgoing,
        Default::default(),
    );
    assert_ne!(
        response.selection_fingerprint,
        changed.selection_fingerprint
    );
    let options = request.search.as_mut().expect("options");
    options.direction = "outgoing".into();
    assert!(trace_search_request_from_proto(&request).is_err());
    request.search.as_mut().expect("options").direction.clear();
    request.search.as_mut().expect("options").paths_per_target = 9;
    assert!(trace_search_request_from_proto(&request).is_err());
    let options = request.search.as_mut().expect("options");
    options.paths_per_target = 2;
    options.follow.push(options.follow[0].clone());
    assert!(trace_search_request_from_proto(&request).is_err());
}
