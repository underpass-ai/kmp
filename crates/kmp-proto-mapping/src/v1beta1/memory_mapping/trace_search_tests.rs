use super::*;
use kmp_domain::{
    NodeRelationProjection, RelationExplanation, RelationSemanticClass, TraceSearchStop,
};

fn result() -> TraceSearchResult {
    TraceSearchResult {
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
