//! A link can be learned after both endpoints were recorded.
use super::temporal_admission::TemporalAdmission;
use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, KmpBundle, RelationExplanation,
    RelationSemanticClass, Role, TemporalAxis, TemporalCursor, TemporalInterval, TemporalSelection,
};
use std::collections::BTreeMap;

fn bundle(relation: &str) -> KmpBundle {
    let node =
        |id: &str| BundleNode::new(id, "memory", id, id, "ACTIVE", Vec::new(), BTreeMap::new());
    let mut edges: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|id| {
            BundleRelationship::new(
                "lane",
                id,
                "contains_entry",
                RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("work")
                    .with_scope_id("lane")
                    .with_occurred_at("2026-09-01T10:00:00Z")
                    .with_observed_at("2026-09-01T10:00:00Z")
                    .with_ingested_at("2026-09-01T10:00:00Z")
                    .with_valid_from("2026-09-01T10:00:00Z"),
            )
        })
        .collect();
    edges.push(BundleRelationship::new(
        "a",
        "b",
        relation,
        RelationExplanation::new(RelationSemanticClass::Evidential)
            .with_rationale("A later review establishes the connection.")
            .with_evidence("Review received at thirteen.")
            .with_occurred_at("2026-09-01T11:00:00Z")
            .with_observed_at("2026-09-01T13:00:00Z")
            .with_ingested_at("2026-09-01T14:00:00Z")
            .with_valid_from("2026-09-01T13:00:00Z"),
    ));
    edges.push(BundleRelationship::new(
        "b",
        "a",
        "uses_background",
        RelationExplanation::new(RelationSemanticClass::Evidential),
    ));
    KmpBundle::new(
        CaseId::new("about:x").expect("about"),
        Role::new("reader").expect("role"),
        node("about:x"),
        ["lane", "a", "b"].into_iter().map(node).collect(),
        edges,
        Vec::new(),
        BundleMetadata::initial("test"),
    )
    .expect("bundle")
}

fn selected_relations(selection: TemporalSelection) -> Vec<String> {
    let bundle = bundle("supports");
    TemporalAdmission::read(&bundle, &selection)
        .expect("admission")
        .bound(&bundle)
        .relationships()
        .iter()
        .filter(|r| r.relationship_type() != "contains_entry")
        .map(|r| r.relationship_type().to_string())
        .collect()
}

#[test]
fn relation_clocks_bound_links_independently_of_their_old_endpoints() {
    for (axis, expected) in [
        (TemporalAxis::Occurred, vec!["supports", "uses_background"]),
        (TemporalAxis::Default, vec!["supports", "uses_background"]),
        (TemporalAxis::Observed, vec!["uses_background"]),
        (TemporalAxis::Ingested, vec!["uses_background"]),
        (TemporalAxis::Validity, vec!["uses_background"]),
    ] {
        let span = TemporalInterval::new(None, Some("2026-09-01T12:00:00Z".into())).expect("span");
        assert_eq!(
            selected_relations(TemporalSelection::within(span, axis)),
            expected,
            "{axis:?}"
        );
    }
}

#[test]
fn as_of_boundary_is_inclusive_but_a_span_end_is_exclusive() {
    let span = TemporalInterval::new(None, Some("2026-09-01T13:00:00Z".into())).expect("span");
    assert_eq!(
        selected_relations(TemporalSelection::within(span, TemporalAxis::Observed)),
        ["uses_background"]
    );
    let instant = TemporalSelection::as_of(
        TemporalCursor::time("2026-09-01T13:00:00Z").expect("time"),
        TemporalAxis::Observed,
    )
    .expect("instant");
    assert_eq!(selected_relations(instant), ["supports", "uses_background"]);
    assert_eq!(
        selected_relations(TemporalSelection::Frontier),
        ["supports", "uses_background"]
    );
}

#[test]
fn wake_proof_does_not_apply_a_replacement_before_its_own_observation() {
    use super::responses::wake_response_from_result;
    use kmp_application::{GetContextResult, queries::render_graph_bundle};

    for relation in ["supports", "supersedes"] {
        for (time, present) in [
            ("2026-09-01T12:00:00Z", false),
            ("2026-09-01T13:00:00Z", true),
        ] {
            let bundle = bundle(relation);
            let rendered = render_graph_bundle(&bundle);
            let result = GetContextResult {
                bundle,
                rendered,
                requested_scopes: Vec::new(),
                served_at: std::time::SystemTime::UNIX_EPOCH,
                timing: None,
            };
            let selection = TemporalSelection::as_of(
                TemporalCursor::time(time).expect("time"),
                TemporalAxis::Observed,
            )
            .expect("selection");
            let response =
                wake_response_from_result("review", None, result, &selection).expect("wake");
            let proof = response.proof.expect("proof");
            assert_eq!(
                proof.path.iter().any(|edge| edge.rel == relation),
                present,
                "{relation} at {time}"
            );
            assert_eq!(
                !proof.superseded.is_empty(),
                present && relation == "supersedes",
                "replacement state at {time}"
            );
        }
    }
}
