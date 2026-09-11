use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    GraphNeighborhoodReader, MemoryRelationType, NodeProjection, NodeRelationProjection,
    ProjectionMutation, ProjectionWriter, RelationDirection, RelationExplanation,
    RelationSemanticClass, TraceRelationStep, TraceSearchLimits, TraceSearchRequest,
    TraceSearchStop,
};
use std::collections::BTreeMap;

fn node(id: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: "observation".into(),
        title: id.into(),
        summary: id.into(),
        status: "ACTIVE".into(),
        labels: vec!["entry".into()],
        properties: BTreeMap::from([("memory_about".into(), "project:test".into())]),
        provenance: None,
    })
}
fn edge(a: &str, b: &str, rel: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: a.into(),
        target_node_id: b.into(),
        relation_type: rel.into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
            .with_optional_rationale(Some(format!("Register explicitly links {a} to {b}.")))
            .with_optional_evidence(Some(format!("Exact source: {a} {rel} {b}."))),
    }))
}
fn query(targets: &[&str]) -> TraceSearchRequest {
    TraceSearchRequest {
        proof: false,
        about: "project:test".into(),
        from: "s".into(),
        targets: targets.iter().map(|s| (*s).into()).collect(),
        direction: RelationDirection::Outgoing,
        relations: Default::default(),
        dimensions: Default::default(),
        select: None,
        follow: vec![],
        paths_per_target: 2,
        limits: TraceSearchLimits::default(),
        temporal: Default::default(),
    }
}
fn step(rel: &str, direction: RelationDirection) -> TraceRelationStep {
    TraceRelationStep {
        relation: MemoryRelationType::new(rel).expect("type"),
        direction,
    }
}

#[tokio::test]
async fn alternatives_survive_reconvergence_without_reading_adjacency_twice() {
    let dir = tempfile::tempdir().expect("store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open");
    let mut mutations: Vec<_> = ["s", "a", "b", "hub", "t"].map(node).into();
    mutations.extend(
        [
            ("s", "a"),
            ("s", "b"),
            ("a", "hub"),
            ("b", "hub"),
            ("hub", "t"),
        ]
        .map(|(a, b)| edge(a, b, "depends_on")),
    );
    store.apply_mutations(mutations).await.expect("write");
    let result = store
        .load_bounded_trace(&query(&["t"]))
        .await
        .expect("read");
    assert_eq!(result.stop, TraceSearchStop::TargetsReached);
    assert_eq!(result.routes.len(), 2);
    assert_eq!(
        (
            result.discovered_nodes,
            result.scanned_edges,
            result.expanded_nodes
        ),
        (5, 5, 4)
    );
    assert_eq!(result.considered_states, 7);
    assert_eq!(result.relations.len(), 5);
    assert_eq!(result.routes[0].edge_indexes, [0, 1, 2]);
    assert_eq!(result.routes[1].edge_indexes, [3, 4, 2]);
    assert!(result.incomplete_targets.is_empty());
    let mut single = query(&["t"]);
    single.paths_per_target = 1;
    let single = store.load_bounded_trace(&single).await.expect("baseline");
    assert_eq!(single.routes[0], result.routes[0]);
    assert_eq!(single.relations, result.relations[..3]);
    let mut selection = query(&["t"]);
    selection.select = Some(kmp_domain::TraceMaterialSelection {
        max_nodes: 4,
        max_paths: 4,
        groups: vec![],
    });
    let mut selected = store.load_bounded_trace(&selection).await.expect("select");
    let material = selected.material.take().expect("material");
    assert_eq!(material.selected_candidates, [0]);
    assert_eq!(material.material_refs.len(), 4);
    assert_eq!(material.benefit, 1);
    assert_eq!(
        selected, result,
        "selection must not change discovery, clocks, paths or work counters"
    );
}

#[tokio::test]
async fn per_relation_directions_compose_without_reversing_assertions_or_reading_noise() {
    let dir = tempfile::tempdir().expect("store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open");
    let mut mutations: Vec<_> = ["s", "correction", "verification"].map(node).into();
    mutations.extend([
        edge("correction", "s", "corrects"),
        edge("correction", "verification", "verified_by"),
    ]);
    for n in 0..10_000 {
        mutations.push(edge("s", &format!("noise{n:05}"), "depends_on"));
    }
    store.apply_mutations(mutations).await.expect("write");
    let mut request = query(&["verification"]);
    request.paths_per_target = 1;
    request.follow = vec![
        step("verified_by", RelationDirection::Outgoing),
        step("corrects", RelationDirection::Incoming),
    ];
    request.limits.nodes = 8;
    let result = store.load_bounded_trace(&request).await.expect("read");
    assert_eq!(result.stop, TraceSearchStop::TargetsReached);
    assert_eq!((result.discovered_nodes, result.scanned_edges), (3, 2));
    assert_eq!(result.relations[0].source_node_id, "correction");
    assert_eq!(result.relations[0].target_node_id, "s");
    assert_eq!(result.relations[1].source_node_id, "correction");
    assert_eq!(
        result.relations[1].explanation.evidence(),
        Some("Exact source: correction verified_by verification.")
    );
    request.follow.reverse();
    assert_eq!(
        result.routes,
        store
            .load_bounded_trace(&request)
            .await
            .expect("reorder")
            .routes
    );
    request.follow[0].direction = RelationDirection::Outgoing;
    assert!(
        store
            .load_bounded_trace(&request)
            .await
            .expect("wrong direction")
            .routes
            .is_empty()
    );
}

#[tokio::test]
async fn cycles_spend_state_budget_and_source_target_requires_only_zero_hops() {
    let dir = tempfile::tempdir().expect("store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open");
    store
        .apply_mutations(vec![
            node("s"),
            node("a"),
            edge("s", "a", "depends_on"),
            edge("a", "s", "depends_on"),
        ])
        .await
        .expect("write");
    let mut request = query(&["missing", "s"]);
    request.limits.states = 2;
    let cut = store.load_bounded_trace(&request).await.expect("cut");
    assert_eq!(cut.stop, TraceSearchStop::StateBudget);
    assert_eq!(cut.considered_states, 2);
    assert_eq!(cut.routes.len(), 1);
    assert!(cut.routes[0].edge_indexes.is_empty());
    assert_eq!(cut.incomplete_targets, ["missing"]);
    request.limits.states = 3;
    let exhausted = store.load_bounded_trace(&request).await.expect("exhausted");
    assert_eq!(exhausted.stop, TraceSearchStop::FrontierExhausted);
    assert_eq!(exhausted.considered_states, 3, "rejected cycle counts");
    assert_eq!(exhausted.leaves, 0, "a cycle is not a leaf");
    let zero = store
        .load_bounded_trace(&query(&["s"]))
        .await
        .expect("zero");
    assert_eq!(zero.stop, TraceSearchStop::TargetsReached);
    assert_eq!(zero.scanned_edges, 0);
}

#[tokio::test]
async fn early_target_survives_a_later_hub_cut_and_missing_alternatives_are_explicit() {
    let dir = tempfile::tempdir().expect("store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open");
    let mut mutations = vec![node("s")];
    for n in 0..40 {
        let name = format!("n{n:02}");
        mutations.push(node(&name));
        mutations.push(edge("s", &name, "depends_on"));
    }
    store.apply_mutations(mutations).await.expect("write");
    let mut request = query(&["n00"]);
    request.limits.edges = 32;
    let cut = store.load_bounded_trace(&request).await.expect("cut");
    assert_eq!(cut.stop, TraceSearchStop::EdgeBudget);
    assert_eq!(cut.scanned_edges, 32);
    assert_eq!(
        cut.routes.len(),
        1,
        "do not preload a full hub before retaining its early route"
    );
    assert!(cut.unreached.is_empty());
    assert_eq!(cut.incomplete_targets, ["n00"]);
    assert_eq!(cut.leaves, 0);
}

#[tokio::test]
async fn alternatives_remain_available_at_one_hundred_hops() {
    let dir = tempfile::tempdir().expect("store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open");
    let mut mutations = vec![node("s"), node("t")];
    for branch in ["a", "b"] {
        let mut previous = "s".to_string();
        for depth in 1..100 {
            let next = format!("{branch}{depth:03}");
            mutations.push(node(&next));
            mutations.push(edge(&previous, &next, "depends_on"));
            previous = next;
        }
        mutations.push(edge(&previous, "t", "depends_on"));
    }
    store.apply_mutations(mutations).await.expect("write");
    let mut request = query(&["t"]);
    request.limits.depth = 100;
    let result = store.load_bounded_trace(&request).await.expect("deep");
    assert_eq!(result.stop, TraceSearchStop::TargetsReached);
    assert_eq!(
        result
            .routes
            .iter()
            .map(|r| r.edge_indexes.len())
            .collect::<Vec<_>>(),
        [100, 100]
    );
    assert_eq!(
        (
            result.discovered_nodes,
            result.scanned_edges,
            result.considered_states
        ),
        (200, 200, 201)
    );
    request.limits.depth = 99;
    let cut = store.load_bounded_trace(&request).await.expect("depth");
    assert_eq!(cut.stop, TraceSearchStop::DepthBudget);
    assert!(cut.routes.is_empty());
    assert_eq!(cut.leaves, 0);
}
