//! A label coordinate with no time on the selected clock stands where its
//! entry stands (V-01). The shipped demo, and any raw ingest, writes labels
//! that carry only a sequence; on the occurred and observed clocks those
//! labels used to be invisible, so a label selector returned nothing.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, DimensionSelection, KmpBundle,
    LabelSelector, LabelSelectorOperator, RelationExplanation, RelationSemanticClass, Role,
    TemporalAxis, TemporalCoordinate, TemporalCursor, TemporalDirection, TemporalInterval,
    TemporalReadWindow, TemporalSelection,
};

use super::{
    TemporalEntry, TemporalMemoryIndex, TemporalMemoryTraversal, TemporalTraversalRequest,
};

/// One label coordinate: (dimension, scope, occurred_at).
type Label<'a> = (&'a str, &'a str, Option<&'a str>);

/// Each entry has a timeline coordinate dated on occurred and observed (or
/// none at all when `at` is `None`) plus the given labels.
fn bundle(entries: &[(&str, Option<&str>, &[Label<'_>])]) -> KmpBundle {
    let mut scopes = BTreeSet::from(["timeline".to_string()]);
    let mut relationships = Vec::new();
    for (index, (ref_id, at, labels)) in entries.iter().enumerate() {
        let sequence = index as u32 + 1;
        let timeline = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension("timeline")
            .with_scope_id("timeline")
            .with_sequence(sequence);
        let timeline = match at {
            Some(at) => timeline.with_occurred_at(*at).with_observed_at(*at),
            None => timeline,
        };
        relationships.push(BundleRelationship::new(
            "timeline",
            *ref_id,
            "contains_entry",
            timeline,
        ));
        for (dimension, scope_id, occurred_at) in labels.iter() {
            scopes.insert(scope_id.to_string());
            let label = RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension(*dimension)
                .with_scope_id(*scope_id)
                .with_sequence(sequence);
            let label = match occurred_at {
                Some(at) => label.with_occurred_at(*at),
                None => label,
            };
            relationships.push(BundleRelationship::new(
                *scope_id,
                *ref_id,
                "contains_entry",
                label,
            ));
        }
    }
    let mut nodes = scopes
        .iter()
        .map(|scope| node(scope, "memory_dimension"))
        .collect::<Vec<_>>();
    nodes.extend(entries.iter().map(|(ref_id, _, _)| node(ref_id, "claim")));
    KmpBundle::new(
        CaseId::new("question:a").expect("case id"),
        Role::new("temporal-reader").expect("role"),
        node("question:a", "question"),
        nodes,
        relationships,
        Vec::new(),
        BundleMetadata::initial("test"),
    )
    .expect("undated label bundle")
}

fn node(node_id: &str, kind: &str) -> BundleNode {
    BundleNode::new(
        node_id,
        kind,
        node_id,
        node_id,
        "ACTIVE",
        Vec::new(),
        BTreeMap::new(),
    )
}

fn selector(key: &str, operator: LabelSelectorOperator, values: &[&str]) -> DimensionSelection {
    DimensionSelection::all().with_selectors([LabelSelector::new(
        key,
        operator,
        values.iter().map(ToString::to_string).collect::<Vec<_>>(),
    )
    .expect("selector")])
}

fn refs(entries: &[TemporalEntry]) -> BTreeSet<String> {
    entries
        .iter()
        .map(|entry| entry.ref_id().to_string())
        .collect()
}

fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(ToString::to_string).collect()
}

/// The demo packet's shape: a dated timeline and undated labels.
fn demo() -> KmpBundle {
    bundle(&[
        (
            "decision:retry",
            Some("2026-07-15T09:00:00Z"),
            &[("component", "checkout-api", None)],
        ),
        (
            "observation:incident",
            Some("2026-09-02T14:30:00Z"),
            &[
                ("component", "checkout-api", None),
                ("incident", "INC-2041", None),
            ],
        ),
        (
            "decision:cap",
            Some("2026-09-10T09:00:00Z"),
            &[
                ("component", "checkout-api", None),
                ("incident", "INC-2041", None),
            ],
        ),
    ])
}

#[test]
fn a_label_selector_finds_undated_labels_on_the_entry_clocks() {
    let bundle = demo();
    for axis in [TemporalAxis::Occurred, TemporalAxis::Observed] {
        let read = |dimensions: DimensionSelection| {
            let request = TemporalTraversalRequest::new(
                TemporalDirection::Rewind,
                TemporalCursor::time("2100-01-01T00:00:00Z").expect("time cursor"),
            )
            .with_axis(axis)
            .with_limit_entries(20)
            .expect("limit")
            .with_dimensions(dimensions);
            let scanned = TemporalMemoryTraversal::traverse(&bundle, &request).expect("scan");
            let indexed = TemporalMemoryIndex::new(bundle.clone(), axis)
                .expect("index")
                .traverse(&request)
                .expect("indexed read");
            assert_eq!(
                refs(scanned.entries()),
                refs(indexed.entries()),
                "the scan and the index agree on {axis:?}"
            );
            scanned
        };

        let component = read(selector(
            "component",
            LabelSelectorOperator::In,
            &["checkout-api"],
        ));
        assert_eq!(
            refs(component.entries()),
            set(&["decision:retry", "observation:incident", "decision:cap"]),
            "every entry labelled checkout-api on {axis:?}"
        );
        assert!(component.entries().iter().all(|entry| {
            entry
                .coordinates()
                .iter()
                .any(|coordinate| coordinate.dimension() == "component")
        }));
        let incident = read(selector("incident", LabelSelectorOperator::Exists, &[]));
        assert_eq!(
            refs(incident.entries()),
            set(&["observation:incident", "decision:cap"])
        );
        let without = read(selector("incident", LabelSelectorOperator::NotExists, &[]));
        assert_eq!(refs(without.entries()), set(&["decision:retry"]));
    }
}

#[test]
fn an_undated_label_stands_where_its_entry_stands_in_an_as_of_read() {
    let bundle = demo();
    let goto = |at: &str| {
        let request = TemporalTraversalRequest::new(
            TemporalDirection::Goto,
            TemporalCursor::time(at).expect("time cursor"),
        )
        .with_axis(TemporalAxis::Occurred)
        .with_dimensions(selector("incident", LabelSelectorOperator::Exists, &[]));
        refs(
            TemporalMemoryTraversal::traverse(&bundle, &request)
                .expect("as-of read")
                .entries(),
        )
    };
    assert_eq!(goto("2026-08-01T00:00:00Z"), set(&[]));
    assert_eq!(goto("2026-09-05T00:00:00Z"), set(&["observation:incident"]));
    assert_eq!(
        goto("2026-12-01T00:00:00Z"),
        set(&["observation:incident", "decision:cap"])
    );
}

#[test]
fn a_dated_label_keeps_its_own_time_on_the_clock() {
    // The incident label was put on the decision on 1 October, after the
    // decision itself (15 July). As of September the label did not stand.
    let bundle = bundle(&[(
        "decision:retry",
        Some("2026-07-15T09:00:00Z"),
        &[
            ("component", "checkout-api", None),
            ("incident", "INC-2041", Some("2026-10-01T00:00:00Z")),
        ],
    )]);
    let goto = |at: &str| {
        let request = TemporalTraversalRequest::new(
            TemporalDirection::Goto,
            TemporalCursor::time(at).expect("time cursor"),
        )
        .with_axis(TemporalAxis::Occurred);
        let incident = request.clone().with_dimensions(selector(
            "incident",
            LabelSelectorOperator::Exists,
            &[],
        ));
        let selected = refs(
            TemporalMemoryTraversal::traverse(&bundle, &incident)
                .expect("as-of read")
                .entries(),
        );
        let proof = TemporalMemoryTraversal::traverse(&bundle, &request)
            .expect("as-of read")
            .proof_labels(&bundle)
            .expect("proof labels");
        let labels = proof
            .get("decision:retry")
            .map(|labels| format!("{labels:?}"))
            .unwrap_or_default();
        (selected, labels)
    };

    let (september, september_labels) = goto("2026-09-01T00:00:00Z");
    assert_eq!(september, set(&[]), "the later label does not stand yet");
    assert!(
        september_labels.contains("checkout-api"),
        "{september_labels}"
    );
    assert!(!september_labels.contains("INC-2041"), "{september_labels}");

    let (november, november_labels) = goto("2026-11-01T00:00:00Z");
    assert_eq!(november, set(&["decision:retry"]));
    assert!(november_labels.contains("INC-2041"), "{november_labels}");
}

#[test]
fn an_entry_with_no_position_on_the_clock_gains_none_from_its_labels() {
    let bundle = bundle(&[
        (
            "decision:dated",
            Some("2026-07-15T09:00:00Z"),
            &[("component", "checkout-api", None)],
        ),
        (
            "decision:undated",
            None,
            &[("component", "checkout-api", None)],
        ),
    ]);
    let request = TemporalTraversalRequest::new(
        TemporalDirection::Rewind,
        TemporalCursor::time("2100-01-01T00:00:00Z").expect("time cursor"),
    )
    .with_axis(TemporalAxis::Occurred)
    .with_dimensions(selector(
        "component",
        LabelSelectorOperator::In,
        &["checkout-api"],
    ));
    let result = TemporalMemoryTraversal::traverse(&bundle, &request).expect("read");
    assert_eq!(refs(result.entries()), set(&["decision:dated"]));
}

#[test]
fn the_read_window_admits_an_undated_label_where_its_entry_stands() {
    let explanation = |dimension: &str, occurred_at: Option<&str>| {
        let explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension(dimension)
            .with_scope_id(dimension)
            .with_sequence(1);
        let explanation = match occurred_at {
            Some(at) => explanation.with_occurred_at(at),
            None => explanation,
        };
        TemporalCoordinate::from_relation_explanation(&explanation)
            .expect("coordinate")
            .expect("coordinate present")
    };
    let entry = vec![
        explanation("timeline", Some("2026-07-15T09:00:00Z")),
        explanation("component", None),
        explanation("incident", Some("2026-10-01T00:00:00Z")),
    ];
    let admitted = |selection: &TemporalSelection, at: Option<&str>| {
        TemporalReadWindow::new(selection, at)
            .admitted_coordinates(&entry)
            .into_iter()
            .map(|coordinate| coordinate.dimension().to_string())
            .collect::<BTreeSet<_>>()
    };

    let as_of = TemporalSelection::as_of(
        TemporalCursor::time("2026-09-01T00:00:00Z").expect("cursor"),
        TemporalAxis::Occurred,
    )
    .expect("as of");
    assert_eq!(
        admitted(&as_of, Some("2026-09-01T00:00:00Z")),
        set(&["timeline", "component"])
    );
    let span = TemporalSelection::within(
        TemporalInterval::new(
            Some("2026-07-01T00:00:00Z".to_string()),
            Some("2026-08-01T00:00:00Z".to_string()),
        )
        .expect("interval"),
        TemporalAxis::Occurred,
    );
    assert_eq!(admitted(&span, None), set(&["timeline", "component"]));
    let later = TemporalSelection::within(
        TemporalInterval::new(Some("2026-08-01T00:00:00Z".to_string()), None).expect("interval"),
        TemporalAxis::Occurred,
    );
    assert_eq!(admitted(&later, None), set(&["incident"]));
    let undated = vec![explanation("component", None)];
    assert!(
        TemporalReadWindow::new(&as_of, Some("2026-09-01T00:00:00Z"))
            .admitted_coordinates(&undated)
            .is_empty(),
        "an entry with no instant on the clock admits nothing"
    );
}
