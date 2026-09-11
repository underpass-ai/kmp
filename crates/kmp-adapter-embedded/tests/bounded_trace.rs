use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    GraphNeighborhoodReader, NodeProjection, NodeRelationProjection, ProjectionMutation,
    ProjectionWriter, RelationDirection, RelationExplanation, RelationSemanticClass,
    TraceSearchLimits, TraceSearchRequest, TraceSearchStop,
};
use std::collections::BTreeMap;

fn node(id: &str, about: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: "observation".into(),
        title: id.into(),
        summary: format!("Fact {id}"),
        status: "ACTIVE".into(),
        labels: vec!["entry".into()],
        properties: BTreeMap::from([("memory_about".into(), about.into())]),
        provenance: None,
    })
}

fn edge(from: &str, to: &str, rel: &str, class: RelationSemanticClass) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: from.into(),
        target_node_id: to.into(),
        relation_type: rel.into(),
        explanation: RelationExplanation::new(class)
            .with_optional_rationale(Some(format!("{from} explicitly requires {to}.")))
            .with_optional_evidence(Some(format!("Source records {from} / {to}; exact proof."))),
    }))
}

fn query(from: &str, targets: &[&str]) -> TraceSearchRequest {
    TraceSearchRequest {
        about: "project:test".into(),
        from: from.into(),
        targets: targets.iter().map(|s| (*s).into()).collect(),
        direction: RelationDirection::Outgoing,
        relations: Default::default(),
        limits: TraceSearchLimits::default(),
        temporal: Default::default(),
    }
}

#[tokio::test]
async fn three_destinations_share_a_hundred_hop_search_and_keep_exact_link_proof() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open fixture store");
    let mut changes = Vec::new();
    for n in 0..=100 {
        changes.push(node(&format!("n{n:03}"), "project:test"));
        if n < 100 {
            changes.push(edge(
                &format!("n{n:03}"),
                &format!("n{:03}", n + 1),
                "depends_on",
                RelationSemanticClass::Causal,
            ));
        }
    }
    for target in ["rule", "verification"] {
        changes.push(node(target, "project:test"));
        changes.push(edge(
            "n000",
            target,
            "verified_by",
            RelationSemanticClass::Evidential,
        ));
    }
    changes.push(edge(
        "n050",
        "n000",
        "depends_on",
        RelationSemanticClass::Causal,
    ));
    store
        .apply_mutations(changes)
        .await
        .expect("fixture write or bounded read");
    let mut request = query("n000", &["n100", "rule", "verification"]);
    request.limits.nodes = 128;
    request.limits.depth = 100;
    let result = store
        .load_bounded_trace(&request)
        .await
        .expect("fixture write or bounded read");
    assert_eq!(result.stop, TraceSearchStop::TargetsReached);
    assert_eq!(result.routes.len(), 3);
    assert_eq!(result.discovered_nodes, 103);
    assert_eq!(result.relations.len(), 102);
    assert_eq!(result.routes[0].edge_indexes.len(), 100);
    assert_eq!(
        result.relations[0].explanation.evidence(),
        Some("Source records n000 / n001; exact proof.")
    );
    assert!(result.unreached.is_empty());

    request.limits.nodes = 64;
    let cut = store
        .load_bounded_trace(&request)
        .await
        .expect("fixture write or bounded read");
    assert_eq!(cut.stop, TraceSearchStop::NodeBudget);
    assert_eq!(cut.discovered_nodes, 64);
    assert_eq!(cut.unreached, ["n100"]);
    assert_eq!(cut.routes.len(), 2);
}

#[tokio::test]
async fn incoming_routes_preserve_arrows_and_shared_prefixes_are_not_duplicated() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open fixture store");
    store
        .apply_mutations(vec![
            node("a", "project:test"),
            node("b", "project:test"),
            node("c", "project:test"),
            edge("a", "b", "depends_on", RelationSemanticClass::Causal),
            edge("b", "c", "depends_on", RelationSemanticClass::Causal),
        ])
        .await
        .expect("fixture write or bounded read");
    let mut request = query("c", &["a", "b"]);
    request.direction = RelationDirection::Incoming;
    let result = store
        .load_bounded_trace(&request)
        .await
        .expect("fixture write or bounded read");
    assert_eq!(result.stop, TraceSearchStop::TargetsReached);
    assert_eq!(result.relations.len(), 2);
    assert_eq!(result.relations[0].source_node_id, "b");
    assert_eq!(result.relations[0].target_node_id, "c");
    assert_eq!(result.routes[0].edge_indexes, [0, 1]);
    assert_eq!(result.routes[1].edge_indexes, [0]);
}

#[tokio::test]
async fn depth_and_edge_cutoffs_never_masquerade_as_a_leaf_or_exhausted_frontier() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open fixture store");
    store
        .apply_mutations(vec![
            node("a", "project:test"),
            node("b", "project:test"),
            node("c", "project:test"),
            edge("a", "b", "depends_on", RelationSemanticClass::Causal),
            edge("b", "c", "depends_on", RelationSemanticClass::Causal),
        ])
        .await
        .expect("fixture write or bounded read");
    let mut request = query("a", &["c"]);
    request.limits.depth = 1;
    let depth = store
        .load_bounded_trace(&request)
        .await
        .expect("fixture write or bounded read");
    assert_eq!(depth.stop, TraceSearchStop::DepthBudget);
    assert_eq!(depth.leaves, 0);
    request.limits = TraceSearchLimits {
        edges: 1,
        ..Default::default()
    };
    let edges = store
        .load_bounded_trace(&request)
        .await
        .expect("fixture write or bounded read");
    assert_eq!(edges.stop, TraceSearchStop::EdgeBudget);
    assert_eq!(edges.scanned_edges, 1);
    assert_eq!(edges.leaves, 0);
    let leaf = store
        .load_bounded_trace(&query("c", &["a"]))
        .await
        .expect("fixture write or bounded read");
    assert_eq!(leaf.stop, TraceSearchStop::FrontierExhausted);
    assert_eq!(leaf.leaves, 1);
    assert!(
        store
            .load_bounded_trace(&query("missing", &["a"]))
            .await
            .is_err()
    );
    let zero = store
        .load_bounded_trace(&query("a", &["a"]))
        .await
        .expect("fixture write or bounded read");
    assert_eq!(zero.stop, TraceSearchStop::TargetsReached);
    assert_eq!(zero.scanned_edges, 0);
    assert!(zero.routes[0].edge_indexes.is_empty());
}

#[tokio::test]
async fn excluded_rows_spend_the_budget_and_never_leak_foreign_paths() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open fixture store");
    let mut changes = vec![node("root", "project:test")];
    for n in 0..10_000 {
        changes.push(edge(
            "root",
            &format!("noise{n:05}"),
            "contains_entry",
            RelationSemanticClass::Structural,
        ));
    }
    store
        .apply_mutations(changes)
        .await
        .expect("fixture write or bounded read");
    let mut request = query("root", &["unseen"]);
    request.limits.nodes = 8;
    let cut = store
        .load_bounded_trace(&request)
        .await
        .expect("fixture write or bounded read");
    assert_eq!(cut.stop, TraceSearchStop::NodeBudget);
    assert_eq!((cut.discovered_nodes, cut.scanned_edges), (8, 7));
    assert!(cut.relations.is_empty());
    assert_eq!(cut.leaves, 0);

    store
        .apply_mutations(vec![
            node("foreign", "project:elsewhere"),
            node("end", "project:test"),
            edge(
                "end",
                "foreign",
                "same_entity_as",
                RelationSemanticClass::Evidential,
            ),
            edge(
                "foreign",
                "root",
                "same_entity_as",
                RelationSemanticClass::Evidential,
            ),
        ])
        .await
        .expect("fixture write or bounded read");
    let scoped = store
        .load_bounded_trace(&query("end", &["root"]))
        .await
        .expect("fixture write or bounded read");
    assert_eq!(scoped.stop, TraceSearchStop::FrontierExhausted);
    assert_eq!(scoped.discovered_nodes, 2);
    assert!(scoped.relations.is_empty());
    assert!(
        store
            .load_bounded_trace(&query("foreign", &["root"]))
            .await
            .is_err()
    );
}
