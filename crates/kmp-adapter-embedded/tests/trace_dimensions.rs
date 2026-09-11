use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    DimensionSelection, GraphNeighborhoodReader, LabelSelector, LabelSelectorOperator,
    MemoryDimensionIdentity, NodeProjection, NodeRelationProjection, ProjectionMutation,
    ProjectionWriter, RelationDirection, RelationExplanation, RelationSemanticClass, TemporalAxis,
    TemporalCursor, TemporalSelection, TraceDimensionPolicy, TraceSearchLimits, TraceSearchRequest,
    TraceSearchStop,
};

const ABOUT: &str = "project:dimension-trace";
const EARLY: &str = "2026-09-11T10:00:00Z";
const CUT: &str = "2026-09-11T12:00:00Z";
const LATE: &str = "2026-09-11T14:00:00Z";

fn node(id: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: "observation".into(),
        title: id.into(),
        summary: id.into(),
        status: "ACTIVE".into(),
        labels: vec!["entry".into()],
        properties: [("memory_about".into(), ABOUT.into())].into(),
        provenance: None,
    })
}
fn edge(a: &str, b: &str) -> ProjectionMutation {
    relation(
        a,
        b,
        "depends_on",
        RelationExplanation::new(RelationSemanticClass::Causal)
            .with_rationale("The source register explicitly declares this dependency.")
            .with_evidence(format!("Register: {a} requires {b}."))
            .with_observed_at(EARLY),
    )
}
fn relation(a: &str, b: &str, kind: &str, explanation: RelationExplanation) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: a.into(),
        target_node_id: b.into(),
        relation_type: kind.into(),
        explanation,
    }))
}
fn label(id: &str, key: &str, value: &str, at: &str) -> ProjectionMutation {
    let ref_id = MemoryDimensionIdentity::new(ABOUT, key, value)
        .expect("label")
        .node_id();
    relation(
        &ref_id,
        id,
        "contains_entry",
        RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension(key)
            .with_scope_id(ref_id.clone())
            .with_observed_at(at),
    )
}
fn selector(key: &str, op: LabelSelectorOperator, values: &[&str]) -> DimensionSelection {
    DimensionSelection::all()
        .with_selectors([LabelSelector::new(key, op, values.iter().copied()).expect("selector")])
}
fn prod() -> DimensionSelection {
    selector("env", LabelSelectorOperator::In, &["prod"])
}
fn query() -> TraceSearchRequest {
    TraceSearchRequest {
        about: ABOUT.into(),
        from: "s".into(),
        targets: ["t".into()].into(),
        direction: RelationDirection::Outgoing,
        relations: Default::default(),
        follow: vec![],
        paths_per_target: 1,
        select: None,
        dimensions: Default::default(),
        limits: TraceSearchLimits::default(),
        temporal: Default::default(),
    }
}
async fn open(mutations: Vec<ProjectionMutation>) -> (tempfile::TempDir, EmbeddedKernelStore) {
    let dir = tempfile::tempdir().expect("dir");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    store.apply_mutations(mutations).await.expect("write");
    (dir, store)
}

#[tokio::test]
async fn focused_order_reaches_a_useful_branch_before_breadth_first_spends_its_node_budget() {
    let mut mutations = vec![];
    for id in ["s", "z", "p", "t", "a"] {
        mutations.push(node(id));
        mutations.push(label(id, "work", "register", EARLY));
    }
    for id in ["s", "z", "p", "t"] {
        mutations.push(label(id, "env", "prod", EARLY));
    }
    mutations.extend([
        edge("s", "a"),
        edge("s", "z"),
        edge("z", "p"),
        edge("p", "t"),
    ]);
    for i in 0..6 {
        let child = format!("a{i}");
        mutations.extend([
            node(&child),
            label(&child, "work", "register", EARLY),
            edge("a", &child),
        ]);
        for j in 0..6 {
            let leaf = format!("a{i}-{j}");
            mutations.extend([
                node(&leaf),
                label(&leaf, "work", "register", EARLY),
                edge(&child, &leaf),
            ]);
        }
    }
    let (_dir, store) = open(mutations).await;
    let mut q = query();
    q.limits.nodes = 18;
    let raw = store.load_bounded_trace(&q).await.expect("raw");
    assert!(raw.routes.is_empty());
    assert_eq!(raw.stop, TraceSearchStop::NodeBudget);
    q.dimensions.preferred = Some(prod());
    let focused = store.load_bounded_trace(&q).await.expect("focused");
    assert_eq!(focused.routes.len(), 1);
    assert_eq!(focused.routes[0].edge_indexes.len(), 3);
    assert!(focused.discovered_nodes <= 18);
    assert!(focused.scanned_edges <= q.limits.edges);
    let stats = focused.routing.as_ref().expect("routing");
    assert_eq!(stats.preferred_route_entries, [4]);
    assert!(stats.preferred_entries >= 4);
    assert_eq!(store.load_bounded_trace(&q).await.expect("repeat"), focused);
}

#[tokio::test]
async fn fifo_reserve_crosses_a_nonpreferred_bridge_that_a_hard_filter_removes() {
    let mut mutations = vec![
        node("s"),
        node("t"),
        node("z-bridge"),
        label("s", "env", "prod", EARLY),
        label("z-bridge", "work", "register", EARLY),
        label("t", "work", "register", EARLY),
        edge("s", "z-bridge"),
        edge("z-bridge", "t"),
    ];
    let mut last = "s".to_string();
    for i in 0..12 {
        let id = format!("a{i:02}");
        mutations.extend([
            node(&id),
            label(&id, "env", "prod", EARLY),
            edge(&last, &id),
        ]);
        last = id;
    }
    let (_dir, store) = open(mutations).await;
    let mut q = query();
    q.dimensions.preferred = Some(prod());
    let r = store.load_bounded_trace(&q).await.expect("focus");
    assert_eq!(r.routes.len(), 1);
    let stats = r.routing.expect("routing");
    assert_eq!(stats.exploration_pops, 1);
    assert_eq!(stats.preferred_route_entries, [1]);
    q.dimensions = TraceDimensionPolicy {
        required: Some(prod()),
        preferred: None,
    };
    let hard = store.load_bounded_trace(&q).await.expect("hard");
    assert!(hard.routes.is_empty());
    assert_eq!(hard.stop, TraceSearchStop::FrontierExhausted);
    assert!(hard.routing.expect("routing").dimensional_rejections > 0);
}

#[tokio::test]
async fn memberships_use_the_selected_clock_whole_label_sets_and_exact_keys() {
    let (_dir, store) = open(vec![
        node("s"),
        node("t"),
        label("s", "env", "prod", EARLY),
        label("t", "env", "staging", EARLY),
        label("t", "env", "prod", LATE),
        label("t", "task", "prod", EARLY),
        edge("s", "t"),
    ])
    .await;
    let mut q = query();
    q.temporal = TemporalSelection::as_of(
        TemporalCursor::time(CUT).expect("cut"),
        TemporalAxis::Observed,
    )
    .expect("time");
    q.dimensions.required = Some(prod());
    let cut = store.load_bounded_trace(&q).await.expect("cut");
    assert!(cut.routes.is_empty());
    assert_eq!(cut.routing.expect("routing").dimensional_rejections, 1);
    q.dimensions.required = None;
    q.dimensions.preferred = Some(prod());
    let soft = store.load_bounded_trace(&q).await.expect("soft");
    assert_eq!(soft.routing.expect("routing").preferred_route_entries, [1]);
    q.from = "t".into();
    q.dimensions = TraceDimensionPolicy {
        required: Some(selector("env", LabelSelectorOperator::NotIn, &["prod"])),
        preferred: None,
    };
    assert_eq!(
        store
            .load_bounded_trace(&q)
            .await
            .expect("notin at cut")
            .routes
            .len(),
        1
    );
    q.temporal = Default::default();
    let current = store.load_bounded_trace(&q).await.expect("current");
    assert_eq!(current.stop, TraceSearchStop::SourceOutsideSelection);
    q.dimensions.required = Some(selector(
        "env",
        LabelSelectorOperator::In,
        &["staging", "prod"],
    ));
    assert_eq!(
        store
            .load_bounded_trace(&q)
            .await
            .expect("multi value")
            .routes
            .len(),
        1
    );
    q.dimensions.required = Some(selector("missing", LabelSelectorOperator::NotExists, &[]));
    assert_eq!(
        store
            .load_bounded_trace(&q)
            .await
            .expect("missing")
            .routes
            .len(),
        1
    );
}

#[tokio::test]
async fn dense_parent_cut_is_explicit_and_preferences_do_not_bypass_global_work_limits() {
    let mut mutations = vec![node("s"), node("t"), label("s", "env", "prod", EARLY)];
    for i in 0..100 {
        let child = format!("n{i:03}");
        mutations.extend([
            node(&child),
            label(&child, "env", "prod", EARLY),
            edge("s", &child),
            edge(&child, "t"),
        ]);
    }
    let (_dir, store) = open(mutations).await;
    let mut q = query();
    q.limits.nodes = 16;
    q.dimensions.preferred = Some(prod());
    let cut = store.load_bounded_trace(&q).await.expect("bounded");
    assert_eq!(cut.stop, TraceSearchStop::NodeBudget);
    assert!(cut.routes.is_empty());
    assert_eq!(cut.expanded_nodes, 1);
    assert!(cut.discovered_nodes <= 16);
    q.limits.nodes = 4096;
    q.limits.edges = 1;
    let cut = store.load_bounded_trace(&q).await.expect("edge cut");
    assert_eq!(cut.stop, TraceSearchStop::EdgeBudget);
    assert!(cut.scanned_edges <= 1);
}

#[test]
fn dimension_policy_rejects_cross_about_scope_and_uninformative_preferences() {
    let mut q = query();
    q.dimensions.preferred = Some(DimensionSelection::all());
    assert!(q.validate().is_err());
    q.dimensions.preferred = Some(prod().with_all_about_scope());
    assert!(q.validate().is_err());
    q.dimensions = TraceDimensionPolicy {
        required: Some(DimensionSelection::only(Vec::<String>::new())),
        preferred: None,
    };
    assert!(q.validate().is_err());
}
