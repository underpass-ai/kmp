//! The lifecycle chain reads the typed relation index of the embedded store.
use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    LifecycleChainReader, LifecycleRelation, LifecycleSide, NodeProjection, NodeRelationProjection,
    ProjectionMutation, ProjectionWriter, RelationExplanation, RelationSemanticClass,
};

fn node(id: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: "memory_entry".into(),
        title: id.into(),
        summary: "Stored summary".into(),
        status: "ACTIVE".into(),
        labels: vec![],
        properties: Default::default(),
        provenance: None,
    })
}

fn edge(source: &str, target: &str, rel: &str, occurred_at: Option<&str>) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: source.into(),
        target_node_id: target.into(),
        relation_type: rel.into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
            .with_optional_rationale(Some("The newer statement replaces the older one.".into()))
            .with_optional_evidence(Some("Dated statements.".into()))
            .with_optional_occurred_at(occurred_at.map(str::to_string)),
    }))
}

#[tokio::test]
async fn the_chain_follows_every_lifecycle_relation_through_the_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let mut mutations = ["v1", "v2", "v3", "v4", "fork-a", "fork-b"]
        .into_iter()
        .map(node)
        .collect::<Vec<_>>();
    // A busy memory: its other relations must not be read to find its
    // successors, and must not be mistaken for them.
    for index in 0..200 {
        let other = format!("note-{index:03}");
        mutations.push(node(&other));
        mutations.push(edge(&other, "v2", "supports", None));
        mutations.push(edge("v2", &other, "depends_on", None));
    }
    mutations.extend([
        edge("v2", "v1", "supersedes", Some("2026-09-01T00:00:00Z")),
        edge("v3", "v2", "corrects", Some("2026-09-02T00:00:00Z")),
        edge("v4", "v3", "updates_state", Some("2026-09-03T00:00:00Z")),
        edge("fork-b", "v4", "supersedes", Some("2026-09-05T00:00:00Z")),
        edge("fork-a", "v4", "supersedes", Some("2026-09-06T00:00:00Z")),
    ]);
    store.apply_mutations(mutations).await.expect("seed");

    let chain = store.read_lifecycle_chain("v2").await.expect("chain");

    let newer = chain
        .newer()
        .iter()
        .map(|step| (step.node.as_str(), step.via))
        .collect::<Vec<_>>();
    assert_eq!(
        newer,
        [
            ("v3", LifecycleRelation::Corrects),
            ("v4", LifecycleRelation::UpdatesState),
            ("fork-b", LifecycleRelation::Supersedes),
            ("fork-a", LifecycleRelation::Supersedes),
        ]
    );
    assert_eq!(chain.older()[0].node, "v1");
    assert_eq!(chain.heads(), ["fork-b", "fork-a"], "(occurred, id) order");
    let fork = &chain.forks()[0];
    assert_eq!((fork.at.as_str(), fork.side), ("v4", LifecycleSide::Newer));
    assert!(!chain.truncated() && !chain.cycle_cut());
}

#[tokio::test]
async fn a_memory_with_no_lifecycle_is_its_own_head() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    store
        .apply_mutations(vec![
            node("alone"),
            node("friend"),
            edge("friend", "alone", "supports", None),
        ])
        .await
        .expect("seed");

    let chain = store.read_lifecycle_chain("alone").await.expect("chain");

    assert!(chain.is_empty());
    assert_eq!(chain.heads(), ["alone"]);
}
