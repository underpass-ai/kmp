use super::*;
use kmp_domain::{
    ContextEventStore, NodeDetailProjection, ProjectionMutation, ProjectionWriter,
    RelationExplanation, RelationSemanticClass,
};
use std::collections::BTreeMap;

fn node(id: &str, summary: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: "memory_entry".into(),
        title: id.into(),
        summary: summary.into(),
        status: "active".into(),
        labels: Vec::new(),
        properties: BTreeMap::new(),
        provenance: None,
    })
}

fn edge(source: &str, target: &str, relation_type: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: source.into(),
        target_node_id: target.into(),
        relation_type: relation_type.into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural),
    }))
}

fn event(root: &str) -> ContextUpdatedEvent {
    ContextUpdatedEvent {
        root_node_id: root.into(),
        role: "memory".into(),
        revision: 0,
        content_hash: "hash".into(),
        changes: Vec::new(),
        idempotency_key: None,
        logical_digest: None,
        requested_by: None,
        occurred_at: std::time::SystemTime::UNIX_EPOCH,
    }
}

#[tokio::test]
async fn point_reads_answer_nodes_bodies_edges_and_the_log_from_one_snapshot() {
    let dir = tempfile::tempdir().expect("dir");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let mut mutations = vec![node("about", ""), node("entry", "the valve froze")];
    for index in 0..1500 {
        mutations.push(edge("about", &format!("n{index:04}"), "records"));
    }
    mutations.push(edge("about", "entry", "records"));
    mutations.push(edge("label", "entry", "contains_entry"));
    mutations.push(ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
        node_id: "entry".into(),
        detail: "the reserve valve froze at night".into(),
        content_hash: "h".into(),
        revision: 1,
    }));
    store.apply_mutations(mutations).await.expect("graph");
    store.append(event("about"), 0).await.expect("first event");
    store.append(event("about"), 1).await.expect("second event");

    let read = store
        .read_points(|reads| {
            let last = reads.last_event_sequence()?;
            let event = reads.event(last)?.map(|event| event.revision);
            let missing = reads.event(last + 1)?.is_none();
            let summary = reads.node("entry")?.map(|node| node.summary);
            let detail = reads.detail("entry")?.map(|detail| detail.detail);
            let out = reads.outgoing("about", Some("records"))?.len();
            // Counted in the index, without reading an edge.
            assert_eq!(reads.outgoing_count("about", "records")?, out as u64);
            assert_eq!(reads.outgoing_count("about", "contains_entry")?, 0);
            assert_eq!(reads.outgoing_count("label", "contains_entry")?, 1);
            let into = reads
                .incoming("entry", None)?
                .into_iter()
                .map(|edge| (edge.source_node_id, edge.relation_type))
                .collect::<Vec<_>>();
            assert!(
                reads
                    .relation("label", "entry", "contains_entry")?
                    .is_some()
            );
            assert!(reads.relation("label", "entry", "records")?.is_none());
            Ok((last, event, missing, summary, detail, out, into))
        })
        .await
        .expect("point reads");

    assert_eq!(read.0, 2);
    assert_eq!(read.1, Some(2));
    assert!(read.2);
    assert_eq!(read.3.as_deref(), Some("the valve froze"));
    assert_eq!(read.4.as_deref(), Some("the reserve valve froze at night"));
    assert_eq!(read.5, 1501, "every page of a long adjacency is read");
    assert_eq!(
        read.6,
        [
            ("about".to_string(), "records".to_string()),
            ("label".to_string(), "contains_entry".to_string())
        ]
    );
}

#[tokio::test]
async fn an_empty_log_ends_at_zero() {
    let dir = tempfile::tempdir().expect("dir");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let last = store
        .read_points(|reads| reads.last_event_sequence())
        .await
        .expect("read");
    assert_eq!(last, 0);
}
