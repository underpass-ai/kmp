use super::scalars::timestamp_from_sort_or_rfc3339;
use super::temporal_admission::TemporalAdmission;
use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, KmpBundle, RelationExplanation,
    RelationSemanticClass, Role, TemporalAxis, TemporalCursor, TemporalInterval, TemporalSelection,
};

#[test]
fn rfc3339_keeps_fractional_seconds() {
    let at = timestamp_from_sort_or_rfc3339(Some("2026-09-11T10:00:00.987654321Z"))
        .expect("valid fractional timestamp");
    assert_eq!(at.nanos, 987_654_321);
}

#[test]
fn rfc3339_offsets_resolve_to_the_same_instant() {
    let utc =
        timestamp_from_sort_or_rfc3339(Some("2026-09-11T10:00:00.125Z")).expect("UTC timestamp");
    for value in [
        "2026-09-11T12:00:00.125+02:00",
        "2026-09-11T05:00:00.125-05:00",
    ] {
        assert_eq!(timestamp_from_sort_or_rfc3339(Some(value)), Some(utc));
    }
}

fn bundle(at: &str) -> KmpBundle {
    let node =
        |id: &str| BundleNode::new(id, "memory", id, id, "ACTIVE", vec![], Default::default());
    let mut links: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|id| {
            BundleRelationship::new(
                "lane",
                id,
                "contains_entry",
                RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("work")
                    .with_scope_id("lane")
                    .with_observed_at("2026-09-11T09:00:00Z"),
            )
        })
        .collect();
    links.push(BundleRelationship::new(
        "a",
        "b",
        "verified_by",
        RelationExplanation::new(RelationSemanticClass::Evidential)
            .with_rationale("This review verifies the old claim.")
            .with_evidence("The review has its own receipt timestamp.")
            .with_observed_at(at),
    ));
    KmpBundle::new(
        CaseId::new("about:x").expect("about"),
        Role::new("reader").expect("role"),
        node("about:x"),
        ["lane", "a", "b"].into_iter().map(node).collect(),
        links,
        vec![],
        BundleMetadata::initial("test"),
    )
    .expect("bundle")
}

#[test]
fn subsecond_cuts_keep_earlier_links_and_exclude_later_links_between_old_endpoints() {
    let cut = "2026-09-11T10:00:00.500Z";
    let instant = TemporalSelection::as_of(
        TemporalCursor::time(cut).expect("cut"),
        TemporalAxis::Observed,
    )
    .expect("as of");
    let span = TemporalSelection::within(
        TemporalInterval::new(None, Some(cut.into())).expect("span"),
        TemporalAxis::Observed,
    );
    for (at, at_instant, in_span) in [
        ("2026-09-11T10:00:00.100Z", true, true),
        ("2026-09-11T10:00:00.500Z", true, false),
        ("2026-09-11T10:00:00.900Z", false, false),
        ("2026-09-11T12:00:00.900+02:00", false, false),
    ] {
        let bundle = bundle(at);
        for (selection, expected) in [(&instant, at_instant), (&span, in_span)] {
            let admission = TemporalAdmission::read(&bundle, selection).expect("admission");
            assert!(admission.admits_entry("a") && admission.admits_entry("b"));
            let bounded = admission.bound(&bundle);
            assert_eq!(
                bounded
                    .relationships()
                    .iter()
                    .any(|r| r.relationship_type() == "verified_by"),
                expected,
                "link={at}, selection={selection:?}"
            );
        }
    }
}
