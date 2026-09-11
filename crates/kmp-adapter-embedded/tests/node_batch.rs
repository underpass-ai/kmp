use std::sync::Arc;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{GraphNeighborhoodReader, NodeProjection, ProjectionMutation, ProjectionWriter};

fn node(id: &str, summary: &str) -> NodeProjection {
    NodeProjection {
        node_id: id.into(),
        node_kind: "evidence".into(),
        title: "Source title".into(),
        summary: summary.into(),
        status: "ACTIVE".into(),
        labels: vec!["source".into()],
        properties: [("source".into(), "signed-document".into())].into(),
        provenance: None,
    }
}

#[tokio::test]
async fn node_batch_preserves_slots_and_uses_the_held_view_through_arc_and_reference() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open");
    let a = node("a", "original a");
    let b = node("b", "original b");
    store
        .apply_mutations(vec![
            ProjectionMutation::UpsertNode(a.clone()),
            ProjectionMutation::UpsertNode(b.clone()),
        ])
        .await
        .expect("seed");
    let held = Arc::new(store.read_snapshot().await.expect("pin"));
    let peer = EmbeddedKernelStore::open(dir.path()).expect("peer");
    peer.apply_mutations(vec![ProjectionMutation::UpsertNode(node("a", "new a"))])
        .await
        .expect("update");
    let ids = vec!["b".into(), "missing".into(), "a".into(), "b".into()];
    let expected = vec![Some(b.clone()), None, Some(a), Some(b)];
    assert_eq!(
        held.load_nodes_batch(ids.clone()).await.expect("arc batch"),
        expected
    );
    assert_eq!(
        GraphNeighborhoodReader::load_nodes_batch(&held.as_ref(), ids)
            .await
            .expect("reference batch"),
        expected
    );
    assert_eq!(
        store
            .load_nodes_batch(vec!["a".into()])
            .await
            .expect("live"),
        [Some(node("a", "new a"))]
    );
}
