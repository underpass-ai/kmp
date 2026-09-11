use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, DimensionSelection, KmpBundle,
    LabelSelector, LabelSelectorOperator, RelationExplanation, RelationSemanticClass, Role,
    TemporalAxis, TemporalCursor, TemporalDirection, TemporalInterval, TemporalMemoryTraversal,
    TemporalTraversalRequest, TemporalTraversalResult, TemporalWindow,
};
use std::collections::{BTreeMap, BTreeSet};

const EARLY: &str = "2026-09-01T10:00:00Z";
const CUT: &str = "2026-09-01T12:00:00Z";
const LATE: &str = "2026-09-02T10:00:00Z";

fn membership(id: &str, key: &str, value: &str, observed: Option<&str>) -> BundleRelationship {
    let scope = format!("label:v1:project%3Atest:{key}:{value}");
    let mut why = RelationExplanation::new(RelationSemanticClass::Structural)
        .with_dimension(key)
        .with_scope_id(&scope)
        .with_sequence(1);
    if let Some(at) = observed {
        why = why.with_observed_at(at);
    }
    BundleRelationship::new(scope, id, "contains_entry", why)
}

fn bundle(edges: Vec<BundleRelationship>) -> KmpBundle {
    let ids = edges
        .iter()
        .flat_map(|edge| [edge.source_node_id(), edge.target_node_id()])
        .collect::<BTreeSet<_>>();
    let node = |id: &str| BundleNode::new(id, "memory", id, id, "ACTIVE", vec![], BTreeMap::new());
    KmpBundle::new(
        CaseId::new("project:test").expect("about"),
        Role::new("memory").expect("role"),
        node("project:test"),
        ids.into_iter().map(node).collect(),
        edges,
        vec![],
        BundleMetadata::initial("test"),
    )
    .expect("bundle")
}

fn read(
    bundle: &KmpBundle,
    direction: TemporalDirection,
    at: &str,
    axis: TemporalAxis,
    selection: DimensionSelection,
) -> TemporalTraversalResult {
    TemporalMemoryTraversal::traverse(
        bundle,
        &TemporalTraversalRequest::new(direction, TemporalCursor::time(at).expect("time"))
            .with_axis(axis)
            .with_dimensions(selection)
            .with_window(TemporalWindow::new(20, 20)),
    )
    .expect("read")
}

fn selector(key: &str, op: LabelSelectorOperator, values: &[&str]) -> DimensionSelection {
    DimensionSelection::all()
        .with_selectors([LabelSelector::new(key, op, values.iter().copied()).expect("selector")])
}

fn labels(result: &TemporalTraversalResult) -> Vec<(&str, &str)> {
    result
        .entries()
        .iter()
        .flat_map(|entry| entry.coordinates().iter())
        .map(|c| (c.dimension(), c.scope_id()))
        .collect()
}

#[test]
fn historical_positive_and_negative_selectors_use_only_admitted_memberships() {
    let data = bundle(vec![
        membership("fact", "task", "register", Some(EARLY)),
        membership("fact", "env", "prod", Some(LATE)),
    ]);
    for axis in [TemporalAxis::Observed, TemporalAxis::Default] {
        for direction in [TemporalDirection::Goto, TemporalDirection::Rewind] {
            assert!(
                read(
                    &data,
                    direction,
                    CUT,
                    axis,
                    selector("env", LabelSelectorOperator::In, &["prod"])
                )
                .entries()
                .is_empty()
            );
            let negative = read(
                &data,
                direction,
                CUT,
                axis,
                selector("env", LabelSelectorOperator::NotExists, &[]),
            );
            assert_eq!(negative.entries().len(), 1);
            assert_eq!(labels(&negative).len(), 1);
            assert_eq!(labels(&negative)[0].0, "task");
        }
    }
    let exact = read(
        &data,
        TemporalDirection::Goto,
        LATE,
        TemporalAxis::Observed,
        selector("env", LabelSelectorOperator::In, &["prod"]),
    );
    assert_eq!(
        labels(&exact).len(),
        2,
        "Goto includes its instant and all earlier admitted labels"
    );
    assert!(
        read(
            &data,
            TemporalDirection::Rewind,
            LATE,
            TemporalAxis::Observed,
            selector("env", LabelSelectorOperator::In, &["prod"])
        )
        .entries()
        .is_empty()
    );
}

#[test]
fn admitted_other_lanes_multivalues_and_keys_remain_distinct() {
    let data = bundle(vec![
        membership("fact", "task", "register", Some(EARLY)),
        membership("fact", "env", "test", Some(EARLY)),
        membership("fact", "env", "prod", Some(CUT)),
        membership("other", "task", "register", Some(EARLY)),
        membership("other", "stage", "prod", Some(EARLY)),
    ]);
    let selection = DimensionSelection::only(["task"]).with_selectors([LabelSelector::new(
        "env",
        LabelSelectorOperator::In,
        ["prod"],
    )
    .expect("selector")]);
    let result = read(
        &data,
        TemporalDirection::Goto,
        CUT,
        TemporalAxis::Observed,
        selection,
    );
    assert_eq!(result.entries().len(), 1);
    assert_eq!(result.entries()[0].ref_id(), "fact");
    assert_eq!(
        labels(&result).len(),
        1,
        "lane projection must not erase selector evidence (#560)"
    );
    let all = read(
        &data,
        TemporalDirection::Goto,
        CUT,
        TemporalAxis::Observed,
        selector("env", LabelSelectorOperator::In, &["prod"]),
    );
    assert_eq!(labels(&all).len(), 3);
}

#[test]
fn an_explicit_clock_does_not_borrow_a_label_from_another_clock() {
    let mut missing = membership("fact", "env", "prod", None);
    // Sequence is present but is not an observation instant.
    assert!(missing.explanation().sequence().is_some());
    let data = bundle(vec![
        membership("fact", "task", "register", Some(EARLY)),
        missing.clone(),
    ]);
    assert!(
        read(
            &data,
            TemporalDirection::Goto,
            CUT,
            TemporalAxis::Observed,
            selector("env", LabelSelectorOperator::Exists, &[])
        )
        .entries()
        .is_empty()
    );
    missing = BundleRelationship::new(
        missing.source_node_id(),
        missing.target_node_id(),
        "contains_entry",
        missing.explanation().clone().with_occurred_at(EARLY),
    );
    let data = bundle(vec![
        membership("fact", "task", "register", Some(EARLY)),
        missing,
    ]);
    assert!(
        read(
            &data,
            TemporalDirection::Goto,
            CUT,
            TemporalAxis::Observed,
            selector("env", LabelSelectorOperator::In, &["prod"])
        )
        .entries()
        .is_empty()
    );
    assert_eq!(
        read(
            &data,
            TemporalDirection::Goto,
            CUT,
            TemporalAxis::Default,
            selector("env", LabelSelectorOperator::In, &["prod"])
        )
        .entries()
        .len(),
        1
    );
}

#[test]
fn each_direction_and_interval_filters_coordinates_before_labels_and_limits() {
    let data = bundle(vec![
        membership("fact", "task", "register", Some(EARLY)),
        membership("fact", "env", "prod", Some(LATE)),
    ]);
    let forward = read(
        &data,
        TemporalDirection::Forward,
        CUT,
        TemporalAxis::Observed,
        DimensionSelection::all(),
    );
    assert_eq!(labels(&forward).len(), 1);
    assert_eq!(labels(&forward)[0].0, "env");
    assert_eq!(
        labels(&read(
            &data,
            TemporalDirection::Near,
            CUT,
            TemporalAxis::Observed,
            DimensionSelection::all()
        ))
        .len(),
        2
    );
    for direction in [
        TemporalDirection::Goto,
        TemporalDirection::Rewind,
        TemporalDirection::Forward,
        TemporalDirection::Near,
    ] {
        let cursor = if matches!(
            direction,
            TemporalDirection::Forward | TemporalDirection::Near
        ) {
            EARLY
        } else {
            LATE
        };
        let request =
            TemporalTraversalRequest::new(direction, TemporalCursor::time(cursor).expect("time"))
                .with_axis(TemporalAxis::Observed)
                .with_interval(
                    TemporalInterval::new(Some(EARLY.into()), Some(LATE.into())).expect("interval"),
                )
                .with_dimensions(selector("env", LabelSelectorOperator::In, &["prod"]));
        assert!(
            TemporalMemoryTraversal::traverse(&data, &request)
                .expect("read")
                .entries()
                .is_empty(),
            "{direction:?}"
        );
    }
}

#[test]
fn ref_goto_keeps_its_cut_and_interval_pages_keep_their_membership_selection() {
    let data = bundle(
        (0..4)
            .flat_map(|i| {
                let id = format!("fact:{i}");
                [
                    membership(&id, "task", "register", Some(EARLY)),
                    membership(&id, "env", "prod", Some(LATE)),
                ]
            })
            .collect(),
    );
    let goto = TemporalTraversalRequest::new(
        TemporalDirection::Goto,
        TemporalCursor::ref_id("fact:3").expect("ref"),
    )
    .with_axis(TemporalAxis::Observed);
    assert!(
        labels(&TemporalMemoryTraversal::traverse(&data, &goto).expect("read"))
            .iter()
            .all(|(key, _)| *key == "task")
    );
    for direction in [TemporalDirection::Forward, TemporalDirection::Rewind] {
        let base = TemporalTraversalRequest::new(direction, None)
            .with_axis(TemporalAxis::Observed)
            .with_interval(
                TemporalInterval::new(Some(EARLY.into()), Some(LATE.into())).expect("interval"),
            )
            .with_dimensions(selector("env", LabelSelectorOperator::NotExists, &[]));
        let expected = TemporalMemoryTraversal::traverse(
            &data,
            &base.clone().with_limit_entries(10).expect("limit"),
        )
        .expect("whole");
        let mut request = base.with_limit_entries(1).expect("limit");
        let mut ids = Vec::new();
        for _ in 0..5 {
            let page = TemporalMemoryTraversal::traverse(&data, &request).expect("page");
            assert!(labels(&page).iter().all(|(key, _)| *key == "task"));
            ids.extend(
                page.entries()
                    .iter()
                    .map(|entry| entry.ref_id().to_string()),
            );
            let Some(next) = page.page().next_cursor() else {
                break;
            };
            request = TemporalTraversalRequest::new(
                direction,
                TemporalCursor::ref_id(next).expect("ref"),
            )
            .with_axis(TemporalAxis::Observed)
            .with_interval(request.interval().expect("interval").clone())
            .with_dimensions(request.dimensions().clone())
            .with_limit_entries(1)
            .expect("limit");
        }
        assert_eq!(
            ids,
            expected
                .entries()
                .iter()
                .map(|entry| entry.ref_id())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn proof_memberships_keep_older_antecedents_but_exclude_future_labels() {
    let data = bundle(vec![
        membership("fact", "task", "register", Some(EARLY)),
        membership("fact", "env", "prod", Some(LATE)),
    ]);
    let result = read(
        &data,
        TemporalDirection::Goto,
        CUT,
        TemporalAxis::Observed,
        DimensionSelection::all(),
    );
    let proof = result.proof_labels(&data).expect("proof");
    assert_eq!(proof.get("fact").expect("fact labels").keys().count(), 1);
    for direction in [TemporalDirection::Forward, TemporalDirection::Rewind] {
        let request = TemporalTraversalRequest::new(direction, None)
            .with_axis(TemporalAxis::Observed)
            .with_interval(
                TemporalInterval::new(Some(CUT.into()), Some(LATE.into())).expect("interval"),
            );
        let result = TemporalMemoryTraversal::traverse(&data, &request).expect("read");
        let proof = result.proof_labels(&data).expect("proof");
        assert_eq!(
            proof.get("fact").expect("fact labels").keys().count(),
            1,
            "older proof survives the interval lower bound; upper bound stays exclusive"
        );
    }
}

#[test]
fn validity_labels_start_inclusively_and_expire_exclusively() {
    let edges = [
        ("task", "register", EARLY, LATE),
        ("env", "prod", CUT, LATE),
    ]
    .into_iter()
    .map(|(key, value, start, end)| {
        let edge = membership("fact", key, value, Some(EARLY));
        BundleRelationship::new(
            edge.source_node_id(),
            edge.target_node_id(),
            "contains_entry",
            edge.explanation()
                .clone()
                .with_valid_from(start)
                .with_valid_until(end),
        )
    })
    .collect();
    let data = bundle(edges);
    let before = read(
        &data,
        TemporalDirection::Goto,
        EARLY,
        TemporalAxis::Validity,
        selector("env", LabelSelectorOperator::In, &["prod"]),
    );
    assert!(before.entries().is_empty());
    let exact = read(
        &data,
        TemporalDirection::Goto,
        CUT,
        TemporalAxis::Validity,
        selector("env", LabelSelectorOperator::In, &["prod"]),
    );
    assert_eq!(labels(&exact).len(), 2);
    assert_eq!(
        exact
            .proof_labels(&data)
            .expect("proof")
            .get("fact")
            .expect("fact labels")
            .keys()
            .count(),
        2
    );
    let expired = read(
        &data,
        TemporalDirection::Goto,
        LATE,
        TemporalAxis::Validity,
        DimensionSelection::all(),
    );
    assert!(expired.entries().is_empty());
    assert!(expired.proof_labels(&data).expect("proof").is_empty());
}
