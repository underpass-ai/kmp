use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    GraphNeighborhoodReader, MemoryDimensionIdentity, NodeProjection, NodeRelationProjection,
    ProjectionMutation, ProjectionWriter, RelationDirection, RelationExplanation,
    RelationSemanticClass, TemporalAxis, TemporalCursor, TemporalInterval, TemporalSelection,
    TraceSearchLimits, TraceSearchRequest, TraceSearchStop,
};

const ABOUT: &str = "project:temporal-trace";
const EARLY: &str = "2026-09-11T10:00:00.100Z";
const CUT: &str = "2026-09-11T10:00:00.500Z";
const LATE: &str = "2026-09-11T10:00:00.900Z";

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

fn link(from: &str, to: &str, kind: &str, explanation: RelationExplanation) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: from.into(),
        target_node_id: to.into(),
        relation_type: kind.into(),
        explanation,
    }))
}

fn coordinate(id: &str, lane: &str, at: &str) -> ProjectionMutation {
    let label = MemoryDimensionIdentity::new(ABOUT, "work", lane)
        .expect("label")
        .node_id();
    link(
        &label,
        id,
        "contains_entry",
        RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension("work")
            .with_scope_id(label.clone())
            .with_occurred_at(at)
            .with_observed_at(at)
            .with_ingested_at(at)
            .with_valid_from(at),
    )
}

fn proof(at: Option<&str>) -> RelationExplanation {
    let e = RelationExplanation::new(RelationSemanticClass::Evidential)
        .with_rationale("The review verifies the claim.")
        .with_evidence("Review receipt R-17.");
    if let Some(at) = at {
        e.with_occurred_at(at)
            .with_observed_at(at)
            .with_ingested_at(at)
            .with_valid_from(at)
    } else {
        e
    }
}

fn query(axis: TemporalAxis) -> TraceSearchRequest {
    TraceSearchRequest {
        about: ABOUT.into(),
        from: "a".into(),
        targets: ["b".into()].into(),
        direction: RelationDirection::Outgoing,
        select: None,
        follow: vec![],
        paths_per_target: 1,
        relations: Default::default(),
        limits: TraceSearchLimits::default(),
        temporal: TemporalSelection::as_of(TemporalCursor::time(CUT).expect("cut"), axis)
            .expect("selection"),
    }
}

async fn store() -> (tempfile::TempDir, EmbeddedKernelStore) {
    let dir = tempfile::tempdir().expect("directory");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    store
        .apply_mutations(vec![
            node("a"),
            node("b"),
            coordinate("a", "one", EARLY),
            coordinate("b", "one", EARLY),
        ])
        .await
        .expect("entries");
    (dir, store)
}

#[tokio::test]
async fn all_clocks_filter_link_arrival_between_old_endpoints_at_subsecond_boundaries() {
    let (_dir, store) = store().await;
    for axis in [
        TemporalAxis::Occurred,
        TemporalAxis::Observed,
        TemporalAxis::Ingested,
        TemporalAxis::Validity,
    ] {
        for (at, expected_instant, expected_span) in [
            (EARLY, true, true),
            (CUT, true, false),
            (LATE, false, false),
            ("2026-09-11T12:00:00.900+02:00", false, false),
        ] {
            store
                .apply_mutations(vec![link("a", "b", "verified_by", proof(Some(at)))])
                .await
                .expect("link");
            let mut request = query(axis);
            for expected in [expected_instant, expected_span] {
                let r = store.load_bounded_trace(&request).await.expect("trace");
                assert_eq!(
                    !r.routes.is_empty(),
                    expected,
                    "{at}; {:?}",
                    request.temporal
                );
                assert!(r.clock_unknown_edges.is_empty());
                assert!(r.temporal_selection_resolved);
                request.temporal = TemporalSelection::within(
                    TemporalInterval::new(None, Some(CUT.into())).expect("interval"),
                    axis,
                );
            }
        }
    }
}

#[tokio::test]
async fn canonical_coordinate_updates_control_admission_and_ref_cuts_spend_the_budget() {
    let (_dir, store) = store().await;
    store
        .apply_mutations(vec![
            link("a", "b", "verified_by", proof(Some(EARLY))),
            coordinate("b", "one", LATE),
            coordinate("b", "two", CUT),
        ])
        .await
        .expect("update coordinates without node payload");
    let r = store
        .load_bounded_trace(&query(TemporalAxis::Observed))
        .await
        .expect("trace");
    assert_eq!(
        r.stop,
        TraceSearchStop::TargetsReached,
        "any canonical coordinate can admit the entry"
    );
    let mut q = query(TemporalAxis::Observed);
    q.temporal = TemporalSelection::as_of(TemporalCursor::Ref("b".into()), TemporalAxis::Observed)
        .expect("ref");
    let r = store.load_bounded_trace(&q).await.expect("ref trace");
    assert_eq!(
        r.resolved_as_of.as_deref(),
        Some(CUT),
        "resolve earliest coordinate, not first label or stale node payload"
    );
    q.limits.nodes = 2;
    let r = store.load_bounded_trace(&q).await.expect("budget cut");
    assert_eq!(r.stop, TraceSearchStop::NodeBudget);
    assert!(!r.temporal_selection_resolved);
    assert!(r.resolved_as_of.is_none() && r.routes.is_empty());
    assert_eq!(r.discovered_nodes, 2);
    store
        .apply_mutations(vec![coordinate("b", "two", LATE)])
        .await
        .expect("move coordinate");
    let r = store
        .load_bounded_trace(&query(TemporalAxis::Observed))
        .await
        .expect("future entry");
    assert!(r.routes.is_empty());
}

#[tokio::test]
async fn unknown_link_clocks_remain_explicit_and_validity_expiry_excludes_links() {
    let (_dir, store) = store().await;
    store
        .apply_mutations(vec![link("a", "b", "verified_by", proof(None))])
        .await
        .expect("unknown clock");
    let mut q = query(TemporalAxis::Observed);
    let r = store.load_bounded_trace(&q).await.expect("unknown trace");
    assert_eq!(r.clock_unknown_edges, [0]);
    assert_eq!(
        r.relations[0].explanation.evidence(),
        Some("Review receipt R-17.")
    );
    store
        .apply_mutations(vec![link(
            "a",
            "b",
            "verified_by",
            proof(Some(EARLY)).with_valid_until(CUT),
        )])
        .await
        .expect("expiry");
    q.temporal = TemporalSelection::as_of(
        TemporalCursor::time(CUT).expect("cut"),
        TemporalAxis::Validity,
    )
    .expect("validity");
    assert!(
        store
            .load_bounded_trace(&q)
            .await
            .expect("expired")
            .routes
            .is_empty()
    );
    q.temporal = TemporalSelection::within(
        TemporalInterval::new(Some(CUT.into()), None).expect("interval"),
        TemporalAxis::Validity,
    );
    assert!(
        store
            .load_bounded_trace(&q)
            .await
            .expect("expired span")
            .routes
            .is_empty()
    );
}

#[tokio::test]
async fn incoming_temporal_walk_keeps_arrows_and_cuts_are_not_leaves() {
    let (_dir, store) = store().await;
    store
        .apply_mutations(vec![link("a", "b", "verified_by", proof(Some(EARLY)))])
        .await
        .expect("link");
    let mut q = query(TemporalAxis::Observed);
    q.from = "b".into();
    q.targets = ["a".into()].into();
    q.direction = RelationDirection::Incoming;
    let r = store.load_bounded_trace(&q).await.expect("incoming");
    assert_eq!(r.stop, TraceSearchStop::TargetsReached);
    assert_eq!(r.relations[0].source_node_id, "a");
    q.limits.edges = 1;
    let r = store.load_bounded_trace(&q).await.expect("cut");
    assert_eq!(r.stop, TraceSearchStop::EdgeBudget);
    assert_eq!(r.coordinate_rows, 1);
    assert_eq!(r.leaves, 0);
    q.limits = Default::default();
    store
        .apply_mutations(vec![coordinate("b", "one", LATE)])
        .await
        .expect("future source");
    assert_eq!(
        store
            .load_bounded_trace(&q)
            .await
            .expect("excluded source")
            .stop,
        TraceSearchStop::SourceOutsideSelection
    );
}

#[tokio::test]
async fn explicit_clock_never_falls_back_and_invalid_direct_selections_fail() {
    let (_dir, store) = store().await;
    let label = MemoryDimensionIdentity::new(ABOUT, "work", "one")
        .expect("label")
        .node_id();
    store
        .apply_mutations(vec![link(
            &label,
            "a",
            "contains_entry",
            RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension("work")
                .with_scope_id(label.clone())
                .with_observed_at(EARLY),
        )])
        .await
        .expect("observed only");
    let mut q = query(TemporalAxis::Occurred);
    assert_eq!(
        store
            .load_bounded_trace(&q)
            .await
            .expect("missing occurred")
            .stop,
        TraceSearchStop::SourceOutsideSelection
    );
    for cursor in [
        TemporalCursor::Sequence(1),
        TemporalCursor::Time("nonsense".into()),
        TemporalCursor::Ref("".into()),
    ] {
        q.temporal = TemporalSelection::AsOf {
            cursor,
            axis: TemporalAxis::Observed,
        };
        assert!(store.load_bounded_trace(&q).await.is_err());
    }
}

#[tokio::test]
async fn alternatives_and_reverse_moves_still_admit_each_node_and_link_on_the_selected_clock() {
    let (_dir, store) = store().await;
    store
        .apply_mutations(vec![
            node("old"),
            coordinate("old", "one", EARLY),
            node("late_node"),
            coordinate("late_node", "one", LATE),
            node("late_link"),
            coordinate("late_link", "one", EARLY),
            link("old", "a", "corrects", proof(Some(EARLY))),
            link("old", "b", "verified_by", proof(Some(EARLY))),
            link("late_node", "a", "corrects", proof(Some(EARLY))),
            link("late_node", "b", "verified_by", proof(Some(EARLY))),
            link("late_link", "a", "corrects", proof(Some(LATE))),
            link("late_link", "b", "verified_by", proof(Some(EARLY))),
        ])
        .await
        .expect("alternatives");
    let mut request = query(TemporalAxis::Observed);
    request.paths_per_target = 2;
    request.follow = [
        ("corrects", RelationDirection::Incoming),
        ("verified_by", RelationDirection::Outgoing),
    ]
    .into_iter()
    .map(|(rel, direction)| kmp_domain::TraceRelationStep {
        relation: kmp_domain::MemoryRelationType::new(rel).expect("rel"),
        direction,
    })
    .collect();
    let result = store.load_bounded_trace(&request).await.expect("as of");
    assert_eq!(result.routes.len(), 1);
    assert_eq!(result.incomplete_targets, ["b"]);
    assert_eq!(result.stop, TraceSearchStop::FrontierExhausted);
    assert_eq!(result.relations.len(), 2);
    assert!(result.relations.iter().all(|e| e.source_node_id == "old"));
    assert!(result.clock_unknown_edges.is_empty());
    request.temporal = Default::default();
    assert_eq!(
        store
            .load_bounded_trace(&request)
            .await
            .expect("frontier")
            .routes
            .len(),
        2
    );
}
