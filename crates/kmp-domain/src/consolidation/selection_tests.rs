use super::*;
fn view() -> ConsolidatedView {
    ConsolidatedView {
        about: "project:test".into(),
        view: "owners".into(),
        revision: 1,
        authored_at: "2026-01-01T00:00:00Z".into(),
        author: "reader".into(),
        claims: vec![],
        sources: vec![ConsolidationSource {
            reference: "source:one".into(),
            stamp: "stamp".into(),
            body: "source".into(),
            status: "ACTIVE".into(),
            relations: vec![],
            provenance: Default::default(),
            properties: Default::default(),
            dependency_clocks: vec![],
            coordinates: vec![ConsolidationClocks {
                occurred_at: Some("2026-01-02T00:00:00Z".into()),
                observed_at: Some("2026-01-03T00:00:00Z".into()),
                ingested_at: Some("2026-01-04T00:00:00Z".into()),
                valid_from: Some("2026-01-05T00:00:00Z".into()),
                valid_until: Some("2026-01-06T00:00:00Z".into()),
            }],
        }],
    }
}

#[test]
fn four_clocks_preserve_inclusive_starts_and_exclusive_validity_end() {
    for (axis, boundary) in [
        (ConsolidationAxis::Occurred, 2),
        (ConsolidationAxis::Observed, 3),
        (ConsolidationAxis::Ingested, 4),
        (ConsolidationAxis::Validity, 5),
    ] {
        let mut selection = ConsolidationSelection {
            axis,
            as_of: format!("2026-01-0{}T23:59:59Z", boundary - 1),
        };
        assert!(
            selection
                .eligible_sources(&view())
                .expect("before")
                .is_empty()
        );
        selection.as_of = format!("2026-01-0{boundary}T00:00:00Z");
        assert_eq!(
            selection.eligible_sources(&view()).expect("at"),
            vec!["source:one"]
        );
    }
    let selection = ConsolidationSelection {
        axis: ConsolidationAxis::Validity,
        as_of: "2026-01-06T00:00:00Z".into(),
    };
    assert!(
        selection
            .eligible_sources(&view())
            .expect("expired")
            .is_empty()
    );
}

#[test]
fn missing_clocks_and_post_cut_authorship_never_acquire_inferred_times() {
    let selection = ConsolidationSelection {
        axis: ConsolidationAxis::Observed,
        as_of: "2026-01-05T00:00:00Z".into(),
    };
    let mut source = view();
    source.sources[0].coordinates.clear();
    assert!(
        selection
            .eligible_sources(&source)
            .expect("unknown")
            .is_empty()
    );
    let mut future = view();
    future.authored_at = "2026-01-06T00:00:00Z".into();
    assert!(
        selection
            .eligible_sources(&future)
            .expect("future view")
            .is_empty()
    );
    let mut invalid = selection;
    invalid.as_of = "not-an-instant".into();
    assert!(invalid.eligible_sources(&view()).is_err());
}

#[test]
fn a_future_membership_is_not_hidden_by_an_earlier_coordinate() {
    let mut source = view();
    source.sources[0].coordinates.push(ConsolidationClocks {
        observed_at: Some("2026-01-07T00:00:00Z".into()),
        ..Default::default()
    });
    let selection = ConsolidationSelection {
        axis: ConsolidationAxis::Observed,
        as_of: "2026-01-05T00:00:00Z".into(),
    };
    assert!(
        selection
            .eligible_sources(&source)
            .expect("future membership")
            .is_empty()
    );
}
