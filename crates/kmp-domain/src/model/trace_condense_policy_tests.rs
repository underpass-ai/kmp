use super::*;
use crate::{
    NodeBodyDescriptor, NodeCardPresentation, NodeCardStamp, NodeProjection,
    NodeRelationProjection, RelationExplanation, RelationSemanticClass, TraceBodyState,
    TraceProofObject,
};

fn object(id: &str, bytes: u64, status: NodeCardStatus) -> TraceProofObject {
    TraceProofObject {
        node: NodeProjection {
            node_id: id.into(),
            node_kind: "entry".into(),
            title: id.into(),
            summary: String::new(),
            status: "ACTIVE".into(),
            labels: vec!["entry".into()],
            properties: BTreeMap::new(),
            provenance: None,
        },
        descriptor: Some(NodeBodyDescriptor {
            node_id: id.into(),
            revision: 7,
            content_hash: "public".into(),
            record_bytes: bytes + 90,
            body_bytes: bytes,
            record_digest: format!("sha256:{id}"),
        }),
        body: None,
        body_state: TraceBodyState::NotRequested,
        coordinates: vec![],
        card: Some(NodeCardPresentation {
            status,
            text: None,
            stored: (status != NodeCardStatus::Absent).then(|| NodeCardStamp {
                source_revision: 6,
                source_content_hash: "old-public".into(),
                source_record_digest: "old-digest".into(),
                source_body_bytes: bytes,
                card_revision: 13,
                authored_by: "reader".into(),
                authored_at: "2026-09-01T00:00:00Z".into(),
                text_bytes: 9,
            }),
        }),
    }
}

fn support(source: &str, entry: &str) -> NodeRelationProjection {
    NodeRelationProjection {
        source_node_id: source.into(),
        target_node_id: entry.into(),
        relation_type: "supports".into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Evidential),
    }
}

#[test]
fn shared_sources_count_once_per_route_and_rank_before_larger_singletons() {
    let proof = TraceProofResult {
        objects: vec![
            object("a", 9000, NodeCardStatus::Absent),
            object("b", 8000, NodeCardStatus::Absent),
            object("shared", 2048, NodeCardStatus::Stale),
        ],
        supports: vec![support("shared", "a"), support("shared", "b")],
        ..Default::default()
    };
    let groups = vec![["a".into(), "b".into()].into(), ["b".into()].into()];
    let result = recommend(&proof, &groups);
    assert_eq!(
        result
            .items
            .iter()
            .map(|c| (c.descriptor.node_id.as_str(), c.shared_by))
            .collect::<Vec<_>>(),
        vec![("b", 2), ("shared", 2), ("a", 1)]
    );
    let shared = &result.items[1];
    assert_eq!(shared.expect, NodeCardExpectation::CardRevision(13));
    assert_eq!(
        shared.descriptor,
        proof.objects[2]
            .descriptor
            .clone()
            .expect("valid candidate fixture")
    );
    assert_eq!(result.items[2].expect, NodeCardExpectation::Absent);
}

#[test]
fn counts_exclusions_once_and_never_offers_a_missing_body_or_card_revision() {
    let mut missing = object("missing", 5000, NodeCardStatus::Absent);
    missing.descriptor = None;
    let mut malformed = object("bad-stale", 5000, NodeCardStatus::Stale);
    malformed
        .card
        .as_mut()
        .expect("valid candidate fixture")
        .stored = None;
    let mut no_compact = object("plain", 5000, NodeCardStatus::Absent);
    no_compact.card = None;
    let proof = TraceProofResult {
        objects: vec![
            object("small", 1023, NodeCardStatus::Absent),
            object("valid", 500, NodeCardStatus::Valid),
            object("future", 500, NodeCardStatus::AfterCut),
            object("boundary", 1024, NodeCardStatus::Absent),
            missing,
            malformed,
            no_compact,
        ],
        ..Default::default()
    };
    let result = recommend(&proof, &[]);
    assert_eq!(
        (result.below_floor, result.valid, result.after_cut),
        (1, 1, 1)
    );
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].descriptor.node_id, "boundary");
    assert_eq!(result.items[0].shared_by, 0);
}

#[test]
fn equal_costs_use_ref_order_and_cap_reports_the_full_eligible_tail() {
    let mut proof = TraceProofResult {
        objects: (0..12)
            .rev()
            .map(|i| object(&format!("r{i:02}"), 2048, NodeCardStatus::Absent))
            .collect(),
        ..Default::default()
    };
    let result = recommend(&proof, &[]);
    assert_eq!(result.items.len(), 8);
    assert_eq!(result.omitted_count, 4);
    assert_eq!(
        result
            .items
            .first()
            .expect("valid candidate fixture")
            .descriptor
            .node_id,
        "r00"
    );
    assert_eq!(
        result
            .items
            .last()
            .expect("valid candidate fixture")
            .descriptor
            .node_id,
        "r07"
    );
    proof.objects.reverse();
    assert_eq!(recommend(&proof, &[]), result);
}
