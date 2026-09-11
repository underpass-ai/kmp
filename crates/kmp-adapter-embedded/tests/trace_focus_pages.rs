use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    DimensionSelection, GraphNeighborhoodReader, LabelSelector, LabelSelectorOperator,
    MemoryDimensionIdentity, MemoryRelationType, NodeProjection, NodeRelationProjection,
    ProjectionMutation, ProjectionWriter, RelationDirection, RelationExplanation,
    RelationSemanticClass, TemporalAxis, TemporalCursor, TemporalSelection, TraceDimensionPolicy,
    TraceRelationStep, TraceSearchLimits, TraceSearchRequest, TraceSearchStop,
};
use std::collections::BTreeSet;

const ABOUT: &str = "project:focused-pages";
const EARLY: &str = "2026-09-01T10:00:00Z";

fn edge(a: &str, b: &str, rel: &str, at: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: a.into(),
        target_node_id: b.into(),
        relation_type: rel.into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
            .with_rationale(format!("The register explicitly links {a} to {b}."))
            .with_evidence(format!("Source: {a} {rel} {b}."))
            .with_observed_at(at),
    }))
}

async fn fixture(arrows: &[(String, String, &str)]) -> (tempfile::TempDir, EmbeddedKernelStore) {
    let dir = tempfile::tempdir().expect("dir");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let scope = MemoryDimensionIdentity::new(ABOUT, "env", "prod")
        .expect("scope")
        .node_id();
    let ids = arrows
        .iter()
        .flat_map(|(a, b, _)| [a, b])
        .collect::<BTreeSet<_>>();
    let mut mutations = vec![];
    for id in ids {
        mutations.push(ProjectionMutation::UpsertNode(NodeProjection {
            node_id: id.clone(),
            node_kind: "observation".into(),
            title: id.clone(),
            summary: id.clone(),
            status: "ACTIVE".into(),
            labels: vec!["entry".into()],
            properties: [("memory_about".into(), ABOUT.into())].into(),
            provenance: None,
        }));
        mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
            NodeRelationProjection {
                source_node_id: scope.clone(),
                target_node_id: id.clone(),
                relation_type: "contains_entry".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("env")
                    .with_scope_id(scope.clone())
                    .with_observed_at(EARLY),
            },
        )));
    }
    mutations.extend(arrows.iter().map(|(a, b, rel)| edge(a, b, rel, EARLY)));
    store.apply_mutations(mutations).await.expect("ingest");
    (dir, store)
}

fn query(target: &str) -> TraceSearchRequest {
    TraceSearchRequest {
        about: ABOUT.into(),
        from: "s".into(),
        targets: [target.into()].into(),
        direction: RelationDirection::Outgoing,
        relations: Default::default(),
        follow: vec![],
        paths_per_target: 1,
        select: None,
        temporal: Default::default(),
        limits: TraceSearchLimits::default(),
        dimensions: TraceDimensionPolicy {
            required: None,
            preferred: Some(DimensionSelection::all().with_selectors([
                LabelSelector::new("env", LabelSelectorOperator::In, ["prod"]).expect("selector"),
            ])),
        },
    }
}

#[tokio::test]
async fn resumed_positions_do_not_skip_late_edges_and_only_exhausted_nodes_are_leaves() {
    let mut arrows = (0..12)
        .map(|i| ("s".into(), format!("n{i:02}"), "depends_on"))
        .collect::<Vec<_>>();
    arrows.push(("n11".into(), "t".into(), "depends_on"));
    let (_dir, store) = fixture(&arrows).await;
    let result = store
        .load_bounded_trace(&query("t"))
        .await
        .expect("late edge");
    assert_eq!(result.stop, TraceSearchStop::TargetsReached);
    assert_eq!(result.routes[0].edge_indexes.len(), 2);
    assert_eq!(result.relations[0].target_node_id, "n11");
    assert!(result.routing.as_ref().expect("routing").resumed_states >= 2);
    assert_eq!(
        result,
        store
            .load_bounded_trace(&query("t"))
            .await
            .expect("deterministic")
    );

    let result = store
        .load_bounded_trace(&query("missing"))
        .await
        .expect("exhaust all");
    assert_eq!(result.stop, TraceSearchStop::FrontierExhausted);
    assert_eq!(result.leaves, 12);
    assert_eq!(result.expanded_nodes, 14);
    assert_eq!(result.scanned_edges, 27); // 13 graph rows and 14 coordinate rows, once each.
    assert_eq!(result.considered_states, 14);
    let routing = result.routing.expect("routing");
    assert_eq!(routing.adjacency_pages, 17); // Exact final full page needs an empty end probe.
    assert_eq!(routing.coordinate_pages, 14);
    assert_eq!(routing.resumed_states, 3);

    let mut q = query("missing");
    q.limits.depth = 1;
    let result = store.load_bounded_trace(&q).await.expect("depth cut");
    assert_eq!(result.stop, TraceSearchStop::DepthBudget);
    assert_eq!(result.leaves, 0);
    q.limits = TraceSearchLimits {
        states: 3,
        ..Default::default()
    };
    let result = store.load_bounded_trace(&q).await.expect("state cut");
    assert_eq!(result.stop, TraceSearchStop::StateBudget);
    assert_eq!(result.considered_states, 3);
    assert_eq!(result.leaves, 0);
}

#[tokio::test]
async fn reconverging_paths_share_partial_pages_without_replaying_a_prefix_twice() {
    let mut arrows = vec![
        ("s".into(), "a".into(), "depends_on"),
        ("s".into(), "b".into(), "depends_on"),
        ("a".into(), "hub".into(), "depends_on"),
        ("b".into(), "hub".into(), "depends_on"),
    ];
    for i in 0..9 {
        arrows.push(("hub".into(), format!("n{i}"), "depends_on"));
        arrows.push((format!("n{i}"), "t".into(), "depends_on"));
    }
    let (_dir, store) = fixture(&arrows).await;
    let mut q = query("missing");
    q.paths_per_target = 2;
    let result = store
        .load_bounded_trace(&q)
        .await
        .expect("all alternative states");
    assert_eq!(result.stop, TraceSearchStop::FrontierExhausted);
    assert_eq!(result.expanded_nodes, 14);
    assert_eq!(result.leaves, 1);
    assert_eq!(result.considered_states, 41);
    assert_eq!(result.scanned_edges, 36); // 22 graph rows + 14 coordinate rows despite two paths.
    assert_eq!(
        result.routing.as_ref().expect("routing").adjacency_pages,
        16
    );
    q.targets = ["t".into()].into();
    let result = store.load_bounded_trace(&q).await.expect("two routes");
    assert_eq!(result.routes.len(), 2);
    assert_ne!(result.routes[0].edge_indexes, result.routes[1].edge_indexes);
    assert_eq!(result.routes[0].edge_indexes.len(), 4);
    assert_eq!(result.routes[1].edge_indexes.len(), 4);
}

#[tokio::test]
async fn page_positions_remain_bound_to_direction_type_and_relation_clock() {
    let mut arrows = (0..8)
        .map(|i| (format!("c{i}"), "s".into(), "corrects"))
        .collect::<Vec<_>>();
    arrows.push(("c7".into(), "t".into(), "verified_by"));
    let (_dir, store) = fixture(&arrows).await;
    let mut q = query("t");
    q.follow = vec![
        TraceRelationStep {
            relation: MemoryRelationType::new("corrects").expect("rel"),
            direction: RelationDirection::Incoming,
        },
        TraceRelationStep {
            relation: MemoryRelationType::new("verified_by").expect("rel"),
            direction: RelationDirection::Outgoing,
        },
    ];
    let result = store.load_bounded_trace(&q).await.expect("mixed moves");
    assert_eq!(result.routes[0].edge_indexes.len(), 2);
    assert_eq!(result.relations[0].source_node_id, "c7");
    assert_eq!(result.relations[0].target_node_id, "s");
    assert_eq!(result.relations[1].source_node_id, "c7");
    assert_eq!(result.relations[1].target_node_id, "t");
    store
        .apply_mutations(vec![edge("c7", "t", "verified_by", "2026-09-03T10:00:00Z")])
        .await
        .expect("later relation");
    q.temporal = TemporalSelection::as_of(
        TemporalCursor::time("2026-09-02T10:00:00Z").expect("cut"),
        TemporalAxis::Observed,
    )
    .expect("selection");
    let result = store
        .load_bounded_trace(&q)
        .await
        .expect("historical relation admission");
    assert_eq!(result.stop, TraceSearchStop::FrontierExhausted);
    assert!(result.routes.is_empty());
}
