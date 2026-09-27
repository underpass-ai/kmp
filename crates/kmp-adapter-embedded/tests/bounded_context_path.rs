//! The single-destination context path under work limits (DESIGN L7).
use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    GraphNeighborhoodReader, NodeProjection, NodeRelationProjection, ProjectionMutation,
    ProjectionWriter, RelationExplanation, RelationSemanticClass, TraceSearchLimits,
    TraceSearchStop,
};
use std::collections::BTreeMap;

fn node(id: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: "observation".into(),
        title: id.into(),
        summary: format!("Fact {id}"),
        status: "ACTIVE".into(),
        labels: vec!["entry".into()],
        properties: BTreeMap::from([("memory_about".into(), "project:test".into())]),
        provenance: None,
    })
}

/// A structural link too: the unbounded walk follows every stored relation.
fn edge(from: &str, to: &str, class: RelationSemanticClass) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: from.into(),
        target_node_id: to.into(),
        relation_type: "depends_on".into(),
        explanation: RelationExplanation::new(class),
    }))
}

async fn store(dir: &std::path::Path) -> EmbeddedKernelStore {
    let store = EmbeddedKernelStore::open(dir).expect("store");
    let mut changes = Vec::new();
    for n in 0..200 {
        changes.push(node(&format!("n{n:03}")));
        if n > 0 {
            let class = if n % 5 == 0 {
                RelationSemanticClass::Structural
            } else {
                RelationSemanticClass::Causal
            };
            changes.push(edge(&format!("n{n:03}"), &format!("n{:03}", n - 1), class));
        }
        if n >= 3 {
            changes.push(edge(
                &format!("n{n:03}"),
                &format!("n{:03}", n - 3),
                RelationSemanticClass::Causal,
            ));
        }
    }
    store.apply_mutations(changes).await.expect("seed");
    store
}

#[tokio::test]
async fn the_bounded_path_is_the_unbounded_one_until_a_limit_cuts_it() {
    let dir = tempfile::tempdir().expect("dir");
    let store = store(dir.path()).await;
    for target in ["n180", "n150", "n101"] {
        let unbounded = store
            .load_context_path("n199", target, 0)
            .await
            .expect("read")
            .expect("reachable");
        let (bounded, search) = store
            .load_bounded_context_path("n199", target, 0, TraceSearchLimits::default())
            .await
            .expect("read");
        let search = search.expect("the embedded store reports its search");
        assert_eq!(search.stop, TraceSearchStop::TargetsReached, "{target}");
        let bounded = bounded.expect("reached");
        assert_eq!(bounded.path_node_ids, unbounded.path_node_ids, "{target}");
        assert_eq!(search.path.as_ref(), Some(&bounded.path_node_ids));
    }

    let tight = TraceSearchLimits {
        nodes: 16,
        ..TraceSearchLimits::default()
    };
    let (none, search) = store
        .load_bounded_context_path("n199", "n000", 0, tight)
        .await
        .expect("read");
    assert!(none.is_none());
    let search = search.expect("reported");
    assert!(search.is_partial(), "{search:?}");
    assert_eq!(search.stop, TraceSearchStop::NodeBudget);
    assert!(search.discovered_nodes <= 16);

    let (none, search) = store
        .load_bounded_context_path("n000", "n199", 0, TraceSearchLimits::default())
        .await
        .expect("read");
    assert!(none.is_none());
    assert_eq!(
        search.expect("reported").stop,
        TraceSearchStop::FrontierExhausted
    );
    let (missing, search) = store
        .load_bounded_context_path("n199", "absent", 0, TraceSearchLimits::default())
        .await
        .expect("read");
    assert!(
        missing.is_none() && search.is_none(),
        "a missing end is no search"
    );
}
