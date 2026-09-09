//! An as-of entry selection must not carry proof learned in its future.
use std::collections::BTreeMap;

use kmp_application::{TemporalIncludeOptions, TemporalMemoryResult};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, BundleQualityMetrics, BundleRelationship, CaseId,
    KmpBundle, RelationExplanation, RelationSemanticClass, Role, TemporalAxis, TemporalCursor,
    TemporalDirection, TemporalMemoryTraversal, TemporalTraversalRequest,
};
use kmp_proto::v1beta1::{TemporalCursor as ProtoCursor, TemporalMoveResponse};

use super::responses::temporal_response_from_result;

const INITIAL: &str = "2026-09-01T09:00:00Z";
const APPROVED: &str = "2026-09-03T15:00:00Z";
const EFFECTIVE: &str = "2026-09-05T00:00:00Z";

fn bundle(old_endpoints: bool) -> KmpBundle {
    let node = |id: &str, kind: &str| {
        BundleNode::new(id, kind, id, id, "ACTIVE", Vec::new(), BTreeMap::new())
    };
    let mut edges: Vec<_> = [
        ("csv", INITIAL, INITIAL),
        (
            "json",
            if old_endpoints { INITIAL } else { APPROVED },
            if old_endpoints { INITIAL } else { EFFECTIVE },
        ),
    ]
    .into_iter()
    .map(|(id, observed, valid)| {
        BundleRelationship::new(
            "lane",
            id,
            "contains_entry",
            RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension("format")
                .with_scope_id("lane")
                .with_observed_at(observed)
                .with_valid_from(valid),
        )
    })
    .collect();
    edges.push(BundleRelationship::new(
        "json",
        "csv",
        "supersedes",
        RelationExplanation::new(RelationSemanticClass::Evidential)
            .with_observed_at(APPROVED)
            .with_valid_from(EFFECTIVE)
            .with_rationale("Approval replaces CSV with JSON from the effective date.")
            .with_evidence("Approval received September 3; effective September 5."),
    ));
    edges.push(BundleRelationship::new(
        "proof:approval",
        "csv",
        "supports",
        RelationExplanation::new(RelationSemanticClass::Evidential),
    ));
    let evidence = BundleNode::new(
        "proof:approval",
        "memory_evidence",
        "approval",
        "approval",
        "ACTIVE",
        Vec::new(),
        BTreeMap::from([("payload_time".into(), APPROVED.into())]),
    );
    KmpBundle::new(
        CaseId::new("project:formats").expect("about"),
        Role::new("reader").expect("role"),
        node("project:formats", "memory_anchor"),
        vec![
            node("lane", "memory_dimension"),
            node("csv", "decision"),
            node("json", "decision"),
            evidence,
        ],
        edges,
        vec![BundleNodeDetail::new(
            "proof:approval",
            "Later approval",
            "hash",
            1,
        )],
        BundleMetadata::initial("test"),
    )
    .expect("bundle")
}

fn goto(
    axis: TemporalAxis,
    cursor: TemporalCursor,
    old_endpoints: bool,
    relations: bool,
) -> TemporalMoveResponse {
    read(
        TemporalTraversalRequest::new(TemporalDirection::Goto, cursor).with_axis(axis),
        old_endpoints,
        relations,
    )
}

fn read(
    request: TemporalTraversalRequest,
    old_endpoints: bool,
    relations: bool,
) -> TemporalMoveResponse {
    let source_bundle = bundle(old_endpoints);
    let traversal = TemporalMemoryTraversal::traverse(&source_bundle, &request).expect("traversal");
    temporal_response_from_result(
        ProtoCursor::default(),
        TemporalDirection::Goto,
        TemporalMemoryResult {
            traversal,
            source_bundle,
            include: TemporalIncludeOptions {
                evidence: true,
                relations,
                raw_refs: false,
            },
            quality: BundleQualityMetrics::new(0, 1.0, 0.0, 0.0, 0.0).expect("quality"),
        },
    )
}

#[test]
fn goto_replacement_starts_on_the_selected_clock_including_the_boundary() {
    for (axis, boundary) in [
        (TemporalAxis::Observed, APPROVED),
        (TemporalAxis::Validity, EFFECTIVE),
    ] {
        for (time, replaced) in [("2026-09-02T12:00:00Z", false), (boundary, true)] {
            for relations in [false, true] {
                let result = goto(
                    axis,
                    TemporalCursor::time(time).expect("time"),
                    false,
                    relations,
                );
                let proof = result.proof.expect("proof");
                assert_eq!(!proof.superseded.is_empty(), replaced, "{axis:?} at {time}");
                assert_eq!(
                    proof.path.iter().any(|edge| edge.rel == "supersedes"),
                    replaced && relations
                );
                assert!(
                    proof.as_of.is_some(),
                    "declare the proof's historical boundary"
                );
                if !replaced {
                    assert_eq!(result.entries.len(), 1);
                }
            }
        }
    }
}

#[test]
fn goto_ref_does_not_attach_a_later_report_to_an_earlier_observation() {
    let result = goto(
        TemporalAxis::Observed,
        TemporalCursor::ref_id("csv").expect("ref"),
        false,
        true,
    );
    let proof = result.proof.expect("proof");
    assert!(proof.superseded.is_empty());
    assert!(
        !proof
            .evidence
            .iter()
            .any(|item| item.text == "Later approval")
    );
    assert!(
        !proof
            .path
            .iter()
            .any(|edge| edge.source_ref == "proof:approval")
    );
    assert!(proof.as_of.is_some());
}

#[test]
fn goto_respects_a_late_relation_even_when_both_endpoints_are_old() {
    let result = goto(
        TemporalAxis::Observed,
        TemporalCursor::time("2026-09-02T12:00:00Z").expect("time"),
        true,
        true,
    );
    assert_eq!(result.entries.len(), 2);
    let proof = result.proof.expect("proof");
    assert!(proof.superseded.is_empty());
    assert!(!proof.path.iter().any(|edge| edge.rel == "supersedes"));
}

#[test]
fn goto_uses_the_stricter_cursor_or_interval_end_for_its_proof() {
    for (time, end, replaced, as_of) in [
        (
            "2026-09-04T12:00:00Z",
            Some("2026-09-08T00:00:00Z"),
            false,
            true,
        ),
        (EFFECTIVE, Some(EFFECTIVE), false, false),
        (EFFECTIVE, Some("2026-09-02T12:00:00Z"), false, false),
        (EFFECTIVE, None, true, true),
    ] {
        let interval =
            kmp_domain::TemporalInterval::new(Some(INITIAL.into()), end.map(str::to_string))
                .expect("interval");
        let request = TemporalTraversalRequest::new(
            TemporalDirection::Goto,
            TemporalCursor::time(time).expect("time"),
        )
        .with_axis(TemporalAxis::Validity)
        .with_interval(interval);
        let proof = read(request, false, true).proof.expect("proof");
        assert_eq!(
            !proof.superseded.is_empty(),
            replaced,
            "at {time}, end {end:?}"
        );
        assert_eq!(proof.as_of.is_some(), as_of);
        assert_eq!(proof.interval.is_some(), !as_of);
        assert!(!proof.missing.iter().any(|item| item == "expiry_boundary"));
    }
}
