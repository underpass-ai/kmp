use kmp_application::InspectMemoryQuery;
use kmp_domain::{
    NodeDetailProjection, NodeProjection, NodeRelationProjection, ProjectionMutation,
    ProjectionWriter, RelationExplanation, RelationSemanticClass,
};
use kmp_embedded::EmbeddedKernel;

const ABOUT: &str = "project:inspect-source-test";
const ROOT: &str = "project:inspect-source-test:entry:observation:root";

fn node(id: &str, kind: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: kind.into(),
        title: "Signed source".into(),
        summary: "Summary when a body is absent".into(),
        status: "ACTIVE".into(),
        labels: vec!["source".into()],
        properties: [
            (
                "payload_supports".into(),
                format!("[\"{ROOT}\",\"another-supported-entry\"]"),
            ),
            ("payload_source".into(), "signed:document".into()),
        ]
        .into(),
        provenance: None,
    })
}
fn link(from: &str, to: &str, kind: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: from.into(),
        target_node_id: to.into(),
        relation_type: kind.into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Evidential),
    }))
}

#[tokio::test]
async fn inspect_batches_only_incoming_typed_supports_and_retains_body_absence() {
    let dir = tempfile::tempdir().expect("temporary memory");
    let kernel = EmbeddedKernel::open(dir.path()).expect("open");
    kernel
        .store()
        .apply_mutations(vec![
            node(ROOT, "observation"),
            node("good", "memory_evidence"),
            node("no-body", "evidence"),
            node("wrong-kind", "observation"),
            node("wrong-relation", "evidence"),
            node("wrong-direction", "evidence"),
            ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
                node_id: "good".into(),
                detail: "Original proof; 17, not 18.".into(),
                content_hash: "stored-hash".into(),
                revision: 9,
            }),
            link("good", ROOT, "supports"),
            link("no-body", ROOT, "supports"),
            link("wrong-kind", ROOT, "supports"),
            link("unmaterialized", ROOT, "supports"),
            link("wrong-relation", ROOT, "mentions"),
            link(ROOT, "wrong-direction", "supports"),
        ])
        .await
        .expect("seed controlled projection states");
    for show_links in [false, true] {
        let result = kernel
            .service()
            .inspect(InspectMemoryQuery {
                about: ABOUT.into(),
                ref_id: ROOT.into(),
                include_details: true,
                include_incoming: show_links,
                include_outgoing: show_links,
                include_raw: false,
            })
            .await
            .expect("inspect");
        assert_eq!(result.evidence.len(), 2);
        assert_eq!(result.evidence[0].detail.node.node_id, "good");
        assert_eq!(
            result.evidence[0].supports,
            [ROOT, "another-supported-entry"]
        );
        assert_eq!(
            result.evidence[0].detail.node.properties["payload_source"],
            "signed:document"
        );
        let body = result.evidence[0]
            .detail
            .detail
            .as_ref()
            .expect("source body");
        assert_eq!(body.detail, "Original proof; 17, not 18.");
        assert_eq!(body.content_hash, "stored-hash");
        assert_eq!(body.revision, 9);
        assert_eq!(result.evidence[1].detail.node.node_id, "no-body");
        assert!(result.evidence[1].detail.detail.is_none());
        assert_eq!(
            result.evidence[1].detail.node.summary,
            "Summary when a body is absent"
        );
        assert_eq!(result.incoming.is_empty(), !show_links);
        assert_eq!(result.outgoing.is_empty(), !show_links);
    }
    assert!(
        kernel
            .service()
            .inspect(InspectMemoryQuery {
                about: "project:outside".into(),
                ref_id: ROOT.into(),
                include_details: true,
                include_incoming: true,
                include_outgoing: true,
                include_raw: false,
            })
            .await
            .is_err(),
        "batching does not broaden the about boundary"
    );
}
