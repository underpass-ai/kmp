use super::responses::temporal_response_from_result;
use kmp_application::{TemporalIncludeOptions, TemporalMemoryResult};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId, KmpBundle,
    RelationExplanation, RelationSemanticClass, Role, TemporalAxis, TemporalCursor,
    TemporalDirection, TemporalMemoryTraversal, TemporalTraversalRequest,
};
use kmp_proto::v1beta1::{TemporalCursor as ProtoCursor, TemporalMoveResponse};
use std::collections::BTreeMap;

const EARLY: &str = "2026-09-01T09:00:00Z";
const LATE: &str = "2026-09-08T11:00:00Z";
const EVENT: &str = "2026-09-10T10:00:00Z";

fn bundle(link_time: &str) -> KmpBundle {
    let node = |id: &str, kind: &str, text: &str| {
        BundleNode::new(id, kind, id, text, "ACTIVE", vec![], BTreeMap::new())
    };
    let mut links = vec![
        BundleRelationship::new(
            "lane",
            "alias",
            "contains_entry",
            RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension("topic")
                .with_scope_id("lane")
                .with_observed_at(EARLY)
                .with_ingested_at(EARLY)
                .with_valid_from(EARLY)
                .with_valid_until("2026-09-10T10:05:00Z"),
        ),
        BundleRelationship::new(
            "lane",
            "role",
            "contains_entry",
            RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension("topic")
                .with_scope_id("lane")
                .with_observed_at(LATE)
                .with_ingested_at(LATE)
                .with_occurred_at(EVENT)
                .with_valid_from(EVENT),
        ),
        BundleRelationship::new(
            "alias",
            "role",
            "same_entity_as",
            RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_observed_at(link_time)
                .with_ingested_at(link_time)
                .with_rationale("Same scoped person in two source records")
                .with_evidence("Explicit workshop equivalence"),
        ),
    ];
    let mut nodes = vec![
        node("lane", "memory_dimension", "topic"),
        node("alias", "observation", "Elena uses Nora in Alba"),
        node("role", "constraint", "Nora in Alba is accountable"),
    ];
    let mut details = Vec::new();
    for (id, time, text) in [
        ("alias", EARLY, "Workshop register: Elena uses Nora"),
        (
            "role",
            LATE,
            "Responsibility notice: Nora in Alba is accountable",
        ),
    ] {
        let proof_id = format!("evidence:{id}");
        nodes.push(BundleNode::new(
            &proof_id,
            "memory_evidence",
            id,
            text,
            "ACTIVE",
            vec![],
            BTreeMap::from([("payload_time".into(), time.into())]),
        ));
        details.push(BundleNodeDetail::new(&proof_id, text, "hash", 1));
        links.push(BundleRelationship::new(
            &proof_id,
            id,
            "supports",
            RelationExplanation::new(RelationSemanticClass::Evidential),
        ));
    }
    KmpBundle::new(
        CaseId::new("project:test").expect("valid temporal fixture"),
        Role::new("reader").expect("valid temporal fixture"),
        node("project:test", "memory_anchor", "test"),
        nodes,
        links,
        details,
        BundleMetadata::initial("test"),
    )
    .expect("valid temporal fixture")
}

fn read(axis: TemporalAxis, at: &str, dependencies: bool, link_time: &str) -> TemporalMoveResponse {
    read_bundle(bundle(link_time), axis, at, dependencies)
}

fn read_bundle(
    source_bundle: KmpBundle,
    axis: TemporalAxis,
    at: &str,
    dependencies: bool,
) -> TemporalMoveResponse {
    let request = TemporalTraversalRequest::new(
        TemporalDirection::Goto,
        TemporalCursor::time(at).expect("valid temporal fixture"),
    )
    .with_axis(axis)
    .with_limit_entries(1)
    .expect("valid temporal fixture");
    let traversal = TemporalMemoryTraversal::traverse(&source_bundle, &request)
        .expect("valid temporal fixture");
    temporal_response_from_result(
        ProtoCursor::default(),
        TemporalDirection::Goto,
        TemporalMemoryResult {
            traversal,
            source_bundle,
            include: TemporalIncludeOptions {
                evidence: true,
                relations: true,
                raw_refs: false,
                dependencies,
            },
        },
    )
}

#[test]
fn expired_relationship_does_not_expand_two_still_valid_endpoints() {
    let original = bundle(LATE);
    let links = original
        .relationships()
        .iter()
        .map(|edge| {
            if edge.relationship_type() == "same_entity_as" {
                BundleRelationship::new(
                    edge.source_node_id(),
                    edge.target_node_id(),
                    edge.relationship_type(),
                    edge.explanation()
                        .clone()
                        .with_valid_from(EARLY)
                        .with_valid_until(EVENT),
                )
            } else {
                edge.clone()
            }
        })
        .collect();
    let source = KmpBundle::new(
        original.root_node_id().clone(),
        original.role().clone(),
        original.root_node().clone(),
        original.neighbor_nodes().to_vec(),
        links,
        original.node_details().to_vec(),
        original.metadata().clone(),
    )
    .expect("valid temporal fixture");
    let response = read_bundle(source, TemporalAxis::Validity, EVENT, true);
    assert_eq!(response.entries[0].r#ref, "role");
    assert_eq!(response.dependency_groups[0].member_refs, ["role"]);
    assert!(response.dependency_entries.is_empty());
}

#[test]
fn observed_dependency_keeps_old_body_coordinates_and_source_with_one_selected_entry() {
    let baseline = read(TemporalAxis::Observed, EVENT, false, LATE);
    let grouped = read(TemporalAxis::Observed, EVENT, true, LATE);
    assert_eq!(grouped.entries, baseline.entries);
    assert_eq!(grouped.page, baseline.page);
    assert_eq!(grouped.entries[0].r#ref, "role");
    assert!(baseline.dependency_groups.is_empty());
    assert_eq!(grouped.dependency_entries[0].r#ref, "alias");
    assert_eq!(
        grouped.dependency_entries[0].text,
        "Elena uses Nora in Alba"
    );
    assert!(
        grouped.dependency_entries[0].coordinates[0]
            .observed_at
            .is_some()
    );
    assert!(
        grouped
            .proof
            .as_ref()
            .expect("valid temporal fixture")
            .evidence
            .iter()
            .any(|e| e.text == "Workshop register: Elena uses Nora")
    );
    assert_eq!(grouped.dependency_groups[0].member_refs, ["role", "alias"]);
}

#[test]
fn occurred_does_not_replace_missing_event_time_with_observation() {
    let result = read(TemporalAxis::Occurred, EVENT, true, LATE);
    assert_eq!(result.dependency_groups[0].member_refs, ["role"]);
    assert_eq!(result.dependency_groups[0].unavailable_in_selection, 1);
    assert!(result.dependency_entries.is_empty());
    assert!(
        !result
            .proof
            .expect("valid temporal fixture")
            .evidence
            .iter()
            .any(|e| e.text.contains("Workshop register"))
    );
}

#[test]
fn future_relation_is_not_followed_even_when_both_memories_are_old() {
    let result = read(TemporalAxis::Observed, EVENT, true, "2026-09-11T10:00:00Z");
    assert_eq!(result.dependency_groups[0].member_refs, ["role"]);
    assert_eq!(result.dependency_groups[0].unavailable_in_selection, 0);
    assert!(result.dependency_entries.is_empty());
}

#[test]
fn validity_end_is_exclusive_and_ingested_cutoff_is_independent() {
    let before = read(TemporalAxis::Validity, "2026-09-10T10:04:00Z", true, LATE);
    let boundary = read(TemporalAxis::Validity, "2026-09-10T10:05:00Z", true, LATE);
    assert_eq!(before.dependency_groups[0].member_refs, ["role", "alias"]);
    assert_eq!(boundary.dependency_groups[0].member_refs, ["role"]);
    assert!(boundary.dependency_entries.is_empty());
    let early = read(TemporalAxis::Ingested, "2026-09-02T00:00:00Z", true, LATE);
    assert_eq!(early.entries[0].r#ref, "alias");
    assert!(early.dependency_entries.is_empty());
    let later = read(TemporalAxis::Ingested, EVENT, true, LATE);
    assert_eq!(later.dependency_groups[0].member_refs, ["role", "alias"]);
}
