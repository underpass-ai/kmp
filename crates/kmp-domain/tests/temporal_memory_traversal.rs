use std::collections::BTreeMap;

use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, DimensionSelection, DomainError,
    KmpBundle, RelationExplanation, RelationSemanticClass, Role, TemporalCursor, TemporalDirection,
    TemporalMemoryTraversal, TemporalTraversalRequest, TemporalWindow,
};

#[test]
fn goto_time_returns_latest_entry_before_cursor_with_all_selected_coordinates() {
    let bundle = sample_bundle();
    let result = TemporalMemoryTraversal::traverse(
        &bundle,
        &TemporalTraversalRequest::new(
            TemporalDirection::Goto,
            TemporalCursor::time("2026-04-12T15:03:00Z").expect("cursor should be valid"),
        ),
    )
    .expect("goto should succeed");

    assert_eq!(result.entries().len(), 1);
    assert_eq!(result.entries()[0].ref_id(), "claim:rachel-denver");
    assert_eq!(
        result.entries()[0]
            .coordinates()
            .iter()
            .map(|coordinate| coordinate.dimension())
            .collect::<Vec<_>>(),
        vec!["conversation", "entity"]
    );
    assert_eq!(
        result.included_dimensions(),
        ["conversation".to_string(), "entity".to_string()]
    );
    assert!(result.missing().is_empty());
}

#[test]
fn forward_ref_respects_only_dimension_selection() {
    let bundle = sample_bundle();
    let result = TemporalMemoryTraversal::traverse(
        &bundle,
        &TemporalTraversalRequest::new(
            TemporalDirection::Forward,
            TemporalCursor::ref_id("claim:rachel-denver").expect("cursor should be valid"),
        )
        .with_dimensions(DimensionSelection::only(["conversation"]))
        .with_limit_entries(5)
        .expect("limit should be valid"),
    )
    .expect("forward should succeed");

    assert_eq!(result.entries().len(), 1);
    assert_eq!(result.entries()[0].ref_id(), "claim:rachel-austin");
    assert_eq!(result.entries()[0].coordinates().len(), 1);
    assert_eq!(
        result.entries()[0].coordinates()[0].dimension(),
        "conversation"
    );
    assert_eq!(result.missing_dimensions(), Vec::<String>::new());
}

#[test]
fn rewind_sequence_uses_sequence_axis_without_mixing_time_only_coordinates() {
    let bundle = sample_bundle();
    let result = TemporalMemoryTraversal::traverse(
        &bundle,
        &TemporalTraversalRequest::new(
            TemporalDirection::Rewind,
            TemporalCursor::sequence(7).expect("cursor should be valid"),
        )
        .with_limit_entries(5)
        .expect("limit should be valid"),
    )
    .expect("rewind should succeed");

    assert_eq!(
        result
            .entries()
            .iter()
            .map(|entry| entry.ref_id())
            .collect::<Vec<_>>(),
        vec!["claim:rachel-austin", "claim:rachel-denver"]
    );
    assert!(result.entries().iter().all(|entry| {
        entry
            .coordinates()
            .iter()
            .any(|coordinate| coordinate.sequence().is_some())
    }));
}

#[test]
fn near_time_returns_bounded_neighbors_around_cursor() {
    let bundle = sample_bundle();
    let result = TemporalMemoryTraversal::traverse(
        &bundle,
        &TemporalTraversalRequest::new(
            TemporalDirection::Near,
            TemporalCursor::time("2026-04-12T15:03:00Z").expect("cursor should be valid"),
        )
        .with_window(TemporalWindow::new(1, 1)),
    )
    .expect("near should succeed");

    assert_eq!(
        result
            .entries()
            .iter()
            .map(|entry| entry.ref_id())
            .collect::<Vec<_>>(),
        vec!["claim:rachel-denver", "claim:rachel-austin"]
    );
}

#[test]
fn except_dimension_removes_dimension_coordinates_from_output() {
    let bundle = sample_bundle();
    let result = TemporalMemoryTraversal::traverse(
        &bundle,
        &TemporalTraversalRequest::new(
            TemporalDirection::Goto,
            TemporalCursor::time("2026-04-12T15:10:00Z").expect("cursor should be valid"),
        )
        .with_dimensions(DimensionSelection::except(["entity"])),
    )
    .expect("goto should succeed");

    assert!(
        result
            .entries()
            .iter()
            .any(|entry| entry.ref_id() == "claim:rachel-austin")
    );
    assert!(result.entries().iter().all(|entry| {
        entry
            .coordinates()
            .iter()
            .all(|coordinate| coordinate.dimension() != "entity")
    }));
}

#[test]
fn time_cursor_does_not_compare_against_sequence_only_positions() {
    let bundle = sample_bundle();
    let result = TemporalMemoryTraversal::traverse(
        &bundle,
        &TemporalTraversalRequest::new(
            TemporalDirection::Forward,
            TemporalCursor::time("2026-04-12T15:10:00Z").expect("cursor should be valid"),
        )
        .with_dimensions(DimensionSelection::only(["benchmark_record"]))
        .with_limit_entries(5)
        .expect("limit should be valid"),
    )
    .expect("forward should succeed");

    assert!(result.entries().is_empty());
    assert_eq!(result.missing(), ["temporal_positions".to_string()]);
    assert_eq!(
        result.missing_dimensions(),
        ["benchmark_record".to_string()]
    );
}

#[test]
fn missing_ref_cursor_fails_fast() {
    let bundle = sample_bundle();
    let error = TemporalMemoryTraversal::traverse(
        &bundle,
        &TemporalTraversalRequest::new(
            TemporalDirection::Forward,
            TemporalCursor::ref_id("claim:missing").expect("cursor should be valid"),
        ),
    )
    .expect_err("missing ref should fail");

    assert_eq!(
        error,
        DomainError::InvalidState("temporal cursor ref not found: claim:missing".to_string())
    );
}

fn sample_bundle() -> KmpBundle {
    KmpBundle::new(
        CaseId::new("question:830ce83f").expect("root id should be valid"),
        Role::new("memory").expect("role should be valid"),
        node("question:830ce83f", "memory_anchor", "Rachel memory"),
        vec![
            node(
                "conversation:rachel-2026-04-12",
                "memory_dimension",
                "Rachel relocation discussion",
            ),
            node("person:rachel", "memory_dimension", "Rachel"),
            node(
                "longmemeval:item:830ce83f",
                "memory_dimension",
                "Benchmark item",
            ),
            node(
                "claim:rachel-denver",
                "claim",
                "Rachel said she was moving to Denver.",
            ),
            node(
                "claim:rachel-austin",
                "claim",
                "Rachel later corrected the destination to Austin.",
            ),
        ],
        vec![
            contains_entry(
                "conversation:rachel-2026-04-12",
                "claim:rachel-denver",
                "conversation",
                Some(1),
                Some("2026-04-12T15:00:00Z"),
                None,
                None,
            ),
            contains_entry(
                "person:rachel",
                "claim:rachel-denver",
                "entity",
                Some(1),
                None,
                Some("2026-04-12T15:00:00Z"),
                None,
            ),
            contains_entry(
                "conversation:rachel-2026-04-12",
                "claim:rachel-austin",
                "conversation",
                Some(2),
                Some("2026-04-12T15:05:00Z"),
                None,
                None,
            ),
            contains_entry(
                "person:rachel",
                "claim:rachel-austin",
                "entity",
                Some(2),
                None,
                Some("2026-04-12T15:05:00Z"),
                None,
            ),
            contains_entry(
                "longmemeval:item:830ce83f",
                "claim:rachel-austin",
                "benchmark_record",
                Some(7),
                None,
                None,
                Some(7),
            ),
            BundleRelationship::new(
                "claim:rachel-austin",
                "claim:rachel-denver",
                "supersedes",
                RelationExplanation::new(RelationSemanticClass::Evidential)
                    .with_rationale("The later statement corrects the earlier destination.")
                    .with_confidence("high"),
            ),
        ],
        Vec::new(),
        BundleMetadata::initial("0.1.0"),
    )
    .expect("sample bundle should be valid")
}

fn node(node_id: &str, kind: &str, summary: &str) -> BundleNode {
    BundleNode::new(
        node_id,
        kind,
        summary,
        summary,
        "ACTIVE",
        Vec::new(),
        BTreeMap::new(),
    )
}

fn contains_entry(
    scope_id: &str,
    entry_id: &str,
    dimension: &str,
    sequence: Option<u32>,
    occurred_at: Option<&str>,
    valid_from: Option<&str>,
    rank: Option<u32>,
) -> BundleRelationship {
    let explanation = RelationExplanation::new(RelationSemanticClass::Structural)
        .with_dimension(dimension)
        .with_scope_id(scope_id)
        .with_optional_occurred_at(occurred_at.map(ToString::to_string))
        .with_optional_valid_from(valid_from.map(ToString::to_string))
        .with_optional_sequence(sequence)
        .with_optional_rank(rank);

    BundleRelationship::new(scope_id, entry_id, "contains_entry", explanation)
}
