use super::project;
use kmp_domain::consolidation::*;

fn source(reference: &str, observed_at: &str) -> ConsolidationSource {
    ConsolidationSource {
        reference: reference.into(),
        stamp: "stamp".into(),
        body: "A source-backed claim.".into(),
        status: "ACTIVE".into(),
        properties: Default::default(),
        provenance: Default::default(),
        relations: vec![],
        dependency_clocks: vec![],
        coordinates: vec![ConsolidationClocks {
            observed_at: Some(observed_at.into()),
            ..Default::default()
        }],
    }
}

fn read() -> ConsolidationRead {
    let identity = ClaimIdentity {
        referent: "person:1".into(),
        predicate: "owns".into(),
        value: "account:1".into(),
        temporal_scope: "2026-01".into(),
        polarity: ClaimPolarity::Affirmed,
        epistemic_status: EpistemicStatus::Reported,
        qualifiers: vec!["staging only".into()],
    };
    ConsolidationRead {
        status: ConsolidationReadStatus::Current,
        changed_sources: vec![],
        view: Some(ConsolidatedView {
            about: "project:test".into(),
            view: "owners".into(),
            revision: 1,
            authored_at: "2026-01-01T00:00:00Z".into(),
            author: "reader".into(),
            claims: vec![ConsolidatedClaim {
                identity: identity.clone(),
                supports: vec![ConsolidationAssertion {
                    source_ref: "source:main".into(),
                    quote: "A source-backed claim.".into(),
                    claim: identity,
                    why: "The explicit statement is the support.".into(),
                }],
            }],
            sources: vec![
                source("source:main", "2026-01-02T00:00:00Z"),
                source("source:background", "2026-01-05T00:00:00Z"),
            ],
        }),
    }
}

#[test]
fn ungrouped_dependencies_must_pass_the_cut_and_cannot_be_silently_ignored() {
    let selection = ConsolidationSelection {
        axis: ConsolidationAxis::Observed,
        as_of: "2026-01-03T00:00:00Z".into(),
    };
    let projected = project(&read(), Some(selection), 4096).expect("projection");
    assert!(projected.claims.is_empty());
    assert_eq!(projected.omitted_claims, 1);
}

#[test]
fn later_and_unknown_relation_clocks_do_not_leak_through_source_clocks() {
    for clock in [
        ConsolidationClocks::default(),
        ConsolidationClocks {
            observed_at: Some("2026-01-09T00:00:00Z".into()),
            ..Default::default()
        },
    ] {
        let mut value = read();
        value.view.as_mut().expect("view").sources[0]
            .dependency_clocks
            .push(clock);
        let selection = ConsolidationSelection {
            axis: ConsolidationAxis::Observed,
            as_of: "2026-01-06T00:00:00Z".into(),
        };
        assert!(
            project(&value, Some(selection), 4096)
                .expect("projection")
                .claims
                .is_empty()
        );
    }
}

#[test]
fn invalid_clock_is_rejected_even_when_view_is_missing_or_stale() {
    for status in [
        ConsolidationReadStatus::Missing,
        ConsolidationReadStatus::Stale,
    ] {
        let value = ConsolidationRead {
            status,
            changed_sources: vec![],
            view: None,
        };
        let selection = ConsolidationSelection {
            axis: ConsolidationAxis::Occurred,
            as_of: "invalid".into(),
        };
        assert!(project(&value, Some(selection), 4096).is_err());
    }
}

#[test]
fn budget_admits_whole_claims_and_audit_expansion_preserves_revision() {
    let full = project(&read(), None, 10000).expect("full");
    let full_bytes = serde_json::to_vec(&full).expect("json").len();
    assert_eq!(full.claims.len(), 1);
    let small = project(&read(), None, full_bytes - 1).expect("smaller");
    assert!(small.claims.is_empty());
    assert_eq!(small.omitted_claims, 1);
    assert_eq!(small.expansion.expect("audit").revision, 1);
    assert!(project(&read(), None, 511).is_err());
}
