//! Public traversal of half-open intervals, without a fabricated starting cursor.
use std::collections::BTreeMap;

use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, DimensionSelection, KmpBundle,
    RelationExplanation, RelationSemanticClass, Role, TemporalAxis, TemporalCursor,
    TemporalDirection, TemporalInterval, TemporalMemoryTraversal, TemporalTraversalRequest,
};

fn bundle() -> KmpBundle {
    let node = |id: &str, kind: &str| {
        BundleNode::new(id, kind, id, id, "ACTIVE", Vec::new(), BTreeMap::new())
    };
    let records = [
        ("before", "2026-09-01T09:00:00Z", "2026-09-01T10:30:00Z"),
        ("start-z", "2026-09-01T10:00:00Z", "2026-09-01T10:00:00Z"),
        ("start-a", "2026-09-01T10:00:00Z", "2026-09-01T10:00:00Z"),
        ("inside", "2026-09-01T11:00:00Z", "2026-09-01T13:00:00Z"),
        ("end", "2026-09-01T12:00:00Z", "2026-09-01T12:00:00Z"),
    ];
    let lanes = [
        ("project", "label:v1:project%3Ap:project:p"),
        ("source", "label:v1:project%3Ap:source:s"),
    ];
    let mut nodes: Vec<_> = lanes
        .iter()
        .map(|(_, id)| node(id, "memory_dimension"))
        .collect();
    nodes.extend(records.iter().map(|(id, _, _)| node(id, "observation")));
    let relationships = records
        .iter()
        .enumerate()
        .flat_map(|(index, (id, occurred, observed))| {
            lanes.iter().map(move |(dimension, scope)| {
                BundleRelationship::new(
                    *scope,
                    *id,
                    "contains_entry",
                    RelationExplanation::new(RelationSemanticClass::Structural)
                        .with_dimension(*dimension)
                        .with_scope_id(*scope)
                        .with_sequence(index as u32 + 1)
                        .with_occurred_at(*occurred)
                        .with_observed_at(*observed)
                        .with_optional_valid_from(
                            matches!(*id, "before" | "inside").then(|| (*occurred).to_string()),
                        )
                        .with_optional_valid_until(
                            matches!(*id, "before" | "inside").then(|| (*observed).to_string()),
                        ),
                )
            })
        })
        .collect();
    KmpBundle::new(
        CaseId::new("project:p").expect("case"),
        Role::new("reader").expect("role"),
        node("project:p", "project"),
        nodes,
        relationships,
        Vec::new(),
        BundleMetadata::initial("test"),
    )
    .expect("source bundle")
}

fn interval() -> TemporalInterval {
    TemporalInterval::new(
        Some("2026-09-01T10:00:00Z".into()),
        Some("2026-09-01T12:00:00Z".into()),
    )
    .expect("half-open interval")
}

#[test]
fn both_directions_include_start_ties_exclude_end_and_preserve_all_memberships() {
    let bundle = bundle();
    for (direction, expected) in [
        (
            TemporalDirection::Forward,
            vec!["start-z", "start-a", "inside"],
        ),
        (
            TemporalDirection::Rewind,
            vec!["inside", "start-a", "start-z"],
        ),
    ] {
        for size in [1, 2, 20] {
            let mut cursor = None;
            let mut seen = Vec::new();
            loop {
                let request = TemporalTraversalRequest::new(direction, cursor.clone())
                    .with_interval(interval())
                    .with_axis(TemporalAxis::Occurred)
                    .with_limit_entries(size)
                    .expect("limit");
                let page =
                    TemporalMemoryTraversal::traverse(&bundle, &request).expect("interval page");
                assert_eq!(page.interval(), Some(&interval()));
                assert_eq!(page.resolved_cursor().is_some(), cursor.is_some());
                for entry in page.entries() {
                    assert_eq!(entry.coordinates().len(), 2);
                    seen.push(entry.ref_id().to_string());
                }
                let Some(next) = page.page().next_cursor() else {
                    break;
                };
                assert!(seen.len() < 4, "must make bounded progress");
                cursor = Some(TemporalCursor::ref_id(next).expect("returned ref"));
            }
            assert_eq!(seen, expected, "direction {direction:?}, page size {size}");
        }
    }
}

#[test]
fn clocks_and_validity_overlap_select_different_source_histories() {
    let bundle = bundle();
    for (axis, expected) in [
        (TemporalAxis::Occurred, vec!["start-z", "start-a", "inside"]),
        (TemporalAxis::Observed, vec!["start-z", "start-a", "before"]),
        (TemporalAxis::Validity, vec!["before", "inside"]),
        (TemporalAxis::Ingested, vec![]),
    ] {
        let request = TemporalTraversalRequest::new(TemporalDirection::Forward, None)
            .with_interval(interval())
            .with_axis(axis)
            .with_dimensions(DimensionSelection::only(["project"]));
        let result = TemporalMemoryTraversal::traverse(&bundle, &request).expect("clock read");
        assert_eq!(
            result
                .entries()
                .iter()
                .map(|e| e.ref_id())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(result.entries().iter().all(|e| e.coordinates().len() == 1));
    }
}

#[test]
fn open_and_empty_intervals_are_distinct_from_missing_clocks_or_an_invalid_start() {
    let bundle = bundle();
    let after = TemporalInterval::new(Some("2026-09-01T13:00:00Z".into()), None).expect("open end");
    let request = TemporalTraversalRequest::new(TemporalDirection::Forward, None)
        .with_interval(after)
        .with_axis(TemporalAxis::Occurred);
    let empty = TemporalMemoryTraversal::traverse(&bundle, &request).expect("empty interval");
    assert!(empty.entries().is_empty() && empty.missing().is_empty());
    assert!(!empty.page().has_more());
    for request in [
        TemporalTraversalRequest::new(TemporalDirection::Forward, None),
        TemporalTraversalRequest::new(TemporalDirection::Near, None).with_interval(interval()),
        TemporalTraversalRequest::new(
            TemporalDirection::Forward,
            TemporalCursor::sequence(1).expect("sequence"),
        )
        .with_interval(interval()),
    ] {
        assert!(TemporalMemoryTraversal::traverse(&bundle, &request).is_err());
    }
}
