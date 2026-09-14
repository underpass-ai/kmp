use super::*;
use kmp_domain::{
    NodeDetailProjection, NodeDetailReader, NodeProjection, NodeRelationProjection,
    ProjectionMutation, ProjectionWriter, RelationExplanation, RelationSemanticClass,
    consolidation::{ClaimIdentity, ClaimPolarity, ConsolidationAssertion, EpistemicStatus},
};

const ABOUT: &str = "project:consolidation-test";
const A: &str = "project:consolidation-test:a";
const B: &str = "project:consolidation-test:b";

#[test]
fn authorship_preserves_subsecond_cuts() {
    use kmp_domain::consolidation::{ConsolidationAxis, ConsolidationSelection};
    use std::time::{Duration, UNIX_EPOCH};

    let instant = UNIX_EPOCH + Duration::new(1_800_000_000, 123_456_789);
    let clock = authored_at(instant).expect("authorship");
    assert_eq!(
        kmp_domain::temporal_instant_nanos(&clock),
        Some(1_800_000_000_123_456_789)
    );
    let view = ConsolidatedView {
        about: ABOUT.into(),
        view: "owners".into(),
        revision: 1,
        authored_at: clock.clone(),
        author: "reader".into(),
        claims: vec![],
        sources: vec![ConsolidationSource {
            reference: A.into(),
            stamp: "stamp".into(),
            body: "source".into(),
            status: "ACTIVE".into(),
            properties: Default::default(),
            provenance: Default::default(),
            coordinates: vec![kmp_domain::consolidation::ConsolidationClocks {
                observed_at: Some("2026-01-01T00:00:00Z".into()),
                ..Default::default()
            }],
            dependency_clocks: vec![],
            relations: vec![],
        }],
    };
    let before = ConsolidationSelection {
        axis: ConsolidationAxis::Observed,
        as_of: authored_at(instant - Duration::from_nanos(1)).expect("previous instant"),
    };
    assert!(before.eligible_sources(&view).expect("before").is_empty());
    let at = ConsolidationSelection {
        as_of: clock,
        ..before
    };
    assert_eq!(at.eligible_sources(&view).expect("at"), vec![A]);
    assert!(authored_at(UNIX_EPOCH - Duration::from_nanos(1)).is_err());
}

fn node(reference: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: reference.into(),
        node_kind: "observation".into(),
        title: reference.into(),
        summary: reference.into(),
        status: "ACTIVE".into(),
        labels: vec!["entry".into()],
        properties: [("memory_about".into(), ABOUT.into())].into(),
        provenance: None,
    })
}
fn body(reference: &str, text: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
        node_id: reference.into(),
        detail: text.into(),
        revision: 1,
        content_hash: "unchanged-public-hash".into(),
    })
}
fn claim() -> ClaimIdentity {
    ClaimIdentity {
        referent: "person:elena-17".into(),
        predicate: "owns".into(),
        value: "account:amber".into(),
        temporal_scope: "2026-09-01/2026-10-01".into(),
        polarity: ClaimPolarity::Affirmed,
        epistemic_status: EpistemicStatus::Reported,
        qualifiers: vec!["only for staging".into()],
    }
}
async fn seeded() -> (tempfile::TempDir, EmbeddedKernelStore, ConsolidationWrite) {
    let dir = tempfile::tempdir().expect("scratch");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open");
    store
        .apply_mutations(vec![
            node(A),
            node(B),
            body(A, "Elena owns Amber, only for staging."),
            body(B, "Amber belongs to Elena for staging only."),
        ])
        .await
        .expect("seed");
    let sources = store
        .consolidation_sources(ABOUT.into(), vec![A.into(), B.into()])
        .await
        .expect("capture");
    let command = ConsolidationWrite { about: ABOUT.into(), view: "owners".into(), expect_revision: 0, idempotency_key: "initial".into(), author: "source-reader".into(),
        sources: sources.iter().map(|s| (s.reference.clone(), s.stamp.clone())).collect(),
        assertions: sources.iter().map(|s| ConsolidationAssertion { source_ref: s.reference.clone(), quote: s.body.clone(), claim: claim(), why: "Both supplied reports concern the same explicitly identified person, account, period and staging restriction.".into() }).collect() };
    (dir, store, command)
}

#[tokio::test]
async fn paraphrases_group_without_rewriting_originals_and_retry_survives_restart() {
    let (dir, store, command) = seeded().await;
    let before = store.load_node_detail(A).await.expect("before");
    let first = store
        .write_consolidation(command.clone())
        .await
        .expect("write");
    assert_eq!(first.claims.len(), 1);
    assert_eq!(first.claims[0].supports.len(), 2);
    assert_eq!(store.load_node_detail(A).await.expect("after"), before);
    let reopened = EmbeddedKernelStore::open(dir.path()).expect("restart");
    assert_eq!(
        reopened.write_consolidation(command).await.expect("replay"),
        first
    );
    assert_eq!(
        reopened
            .read_consolidation(ABOUT.into(), "owners".into(), None)
            .await
            .expect("read")
            .status,
        ConsolidationReadStatus::Current
    );
}

#[tokio::test]
async fn changed_body_under_same_public_revision_invalidates_and_history_survives() {
    let (_dir, store, command) = seeded().await;
    let original = store
        .write_consolidation(command.clone())
        .await
        .expect("write");
    store
        .apply_mutations(vec![body(
            A,
            "Ownership transferred to a different person.",
        )])
        .await
        .expect("change");
    let read = store
        .read_consolidation(ABOUT.into(), "owners".into(), None)
        .await
        .expect("read");
    assert_eq!(read.status, ConsolidationReadStatus::Stale);
    assert_eq!(read.changed_sources, vec![A]);
    assert!(
        read.view.is_none(),
        "stale prose must not be returned as current"
    );
    let history = store
        .read_consolidation(ABOUT.into(), "owners".into(), Some(1))
        .await
        .expect("audit");
    assert_eq!(history.status, ConsolidationReadStatus::HistoricalAudit);
    assert_eq!(history.view, Some(original.clone()));
    assert_eq!(
        store
            .write_consolidation(command)
            .await
            .expect("old receipt"),
        original
    );
}

#[tokio::test]
async fn relation_only_changes_invalidate_but_unrelated_writes_do_not() {
    let (_dir, store, command) = seeded().await;
    store.write_consolidation(command).await.expect("write");
    store
        .apply_mutations(vec![node("unrelated"), body("unrelated", "Another fact")])
        .await
        .expect("unrelated");
    assert_eq!(
        store
            .read_consolidation(ABOUT.into(), "owners".into(), None)
            .await
            .expect("read")
            .status,
        ConsolidationReadStatus::Current
    );
    store
        .apply_mutations(vec![ProjectionMutation::UpsertNodeRelation(Box::new(
            NodeRelationProjection {
                source_node_id: A.into(),
                target_node_id: B.into(),
                relation_type: "contradicts".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                    .with_rationale("Conflicting ownership reports")
                    .with_evidence("Recorded source comparison"),
            },
        ))])
        .await
        .expect("relation");
    let read = store
        .read_consolidation(ABOUT.into(), "owners".into(), None)
        .await
        .expect("read");
    assert_eq!(read.status, ConsolidationReadStatus::Stale);
    assert_eq!(read.changed_sources.len(), 2);
}

#[tokio::test]
async fn relabel_membership_clocks_are_dependencies_even_when_structural() {
    use kmp_domain::consolidation::{ConsolidationAxis, ConsolidationSelection};
    for (observed, admitted) in [
        (Some("2026-01-01T00:00:00Z"), true),
        (Some("2099-01-01T00:00:00Z"), false),
        (None, false),
    ] {
        let (_dir, store, mut command) = seeded().await;
        let ProjectionMutation::UpsertNode(mut source) = node(A) else {
            unreachable!()
        };
        source.properties.insert(
            "payload_coordinates".into(),
            r#"[{"dimension":"task","scope_id":"old","observed_at":"2026-01-01T00:00:00Z"}]"#
                .into(),
        );
        let membership = NodeRelationProjection {
            source_node_id: "scope:new".into(),
            target_node_id: A.into(),
            relation_type: "contains_entry".into(),
            explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension("task")
                .with_scope_id("scope:new")
                .with_optional_observed_at(observed.map(String::from)),
        };
        let ancestry = NodeRelationProjection {
            source_node_id: ABOUT.into(),
            target_node_id: A.into(),
            relation_type: "records".into(),
            explanation: RelationExplanation::new(RelationSemanticClass::Structural),
        };
        store
            .apply_mutations(vec![
                ProjectionMutation::UpsertNode(source),
                ProjectionMutation::UpsertNodeRelation(Box::new(membership)),
                ProjectionMutation::UpsertNodeRelation(Box::new(ancestry)),
            ])
            .await
            .expect("relabel edges");
        let sources = store
            .consolidation_sources(ABOUT.into(), vec![A.into()])
            .await
            .expect("capture");
        assert_eq!(
            sources[0].dependency_clocks.len(),
            1,
            "timeless ancestry is not a clock dependency"
        );
        command.sources = sources
            .iter()
            .map(|s| (s.reference.clone(), s.stamp.clone()))
            .collect();
        command.assertions.retain(|a| a.source_ref == A);
        // Fixed authorship isolates membership admission from wall-clock time.
        let view = consolidate(&command, sources, "2026-09-01T00:00:00Z".into()).expect("view");
        let selection = ConsolidationSelection {
            axis: ConsolidationAxis::Observed,
            as_of: "2026-09-14T00:00:00Z".into(),
        };
        assert_eq!(
            selection
                .eligible_sources(&view)
                .expect("selection")
                .contains(&A.to_string()),
            admitted
        );
    }
}

#[tokio::test]
async fn changed_capture_is_refused_atomically() {
    let (_dir, store, command) = seeded().await;
    store
        .apply_mutations(vec![body(A, "Changed after capture")])
        .await
        .expect("change");
    assert!(matches!(
        store.write_consolidation(command).await,
        Err(PortError::Conflict(_))
    ));
    assert_eq!(
        store
            .read_consolidation(ABOUT.into(), "owners".into(), None)
            .await
            .expect("read")
            .status,
        ConsolidationReadStatus::Missing
    );
}

#[tokio::test]
async fn concurrent_sessions_compare_and_set_one_view_revision() {
    let (dir, store, command) = seeded().await;
    let other = EmbeddedKernelStore::open(dir.path()).expect("second session");
    let mut competing = command.clone();
    competing.idempotency_key = "competing".into();
    let (one, two) = tokio::join!(
        store.write_consolidation(command),
        other.write_consolidation(competing)
    );
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    assert!(matches!(
        one.err().or_else(|| two.err()),
        Some(PortError::Conflict(_))
    ));
}

#[tokio::test]
async fn invalid_quotes_and_foreign_sources_write_nothing() {
    let (_dir, store, mut command) = seeded().await;
    command.assertions[0].quote = "invented quote".into();
    assert!(matches!(
        store.write_consolidation(command).await,
        Err(PortError::InvalidState(_))
    ));
    assert_eq!(
        store
            .read_consolidation(ABOUT.into(), "owners".into(), None)
            .await
            .expect("read")
            .status,
        ConsolidationReadStatus::Missing
    );
    assert!(
        store
            .consolidation_sources("project:foreign".into(), vec![A.into()])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn refresh_is_incremental_and_preserves_both_revisions() {
    let (_dir, store, mut command) = seeded().await;
    store
        .write_consolidation(command.clone())
        .await
        .expect("first");
    store
        .apply_mutations(vec![body(B, "Amber belongs to Elena for production only.")])
        .await
        .expect("change");
    let sources = store
        .consolidation_sources(ABOUT.into(), vec![A.into(), B.into()])
        .await
        .expect("refresh");
    command.sources = sources
        .iter()
        .map(|s| (s.reference.clone(), s.stamp.clone()))
        .collect();
    command.assertions[1].quote = sources
        .iter()
        .find(|s| s.reference == B)
        .expect("b")
        .body
        .clone();
    command.assertions[1].claim.qualifiers = vec!["only for production".into()];
    command.expect_revision = 1;
    command.idempotency_key = "refresh".into();
    let refreshed = store
        .write_consolidation(command)
        .await
        .expect("refresh view");
    assert_eq!(refreshed.revision, 2);
    assert_eq!(refreshed.claims.len(), 2);
    assert_eq!(
        store
            .read_consolidation(ABOUT.into(), "owners".into(), Some(1))
            .await
            .expect("old")
            .view
            .expect("old view")
            .claims
            .len(),
        1
    );
}

#[tokio::test]
async fn incompatible_dimensions_never_collapse_and_unknowns_remain_separate() {
    for field in [
        "referent",
        "value",
        "temporal_scope",
        "polarity",
        "epistemic_status",
        "qualifiers",
        "unknown",
    ] {
        let (_dir, store, mut command) = seeded().await;
        match field {
            "referent" => command.assertions[1].claim.referent = "person:elena-homonym".into(),
            "value" => command.assertions[1].claim.value = "account:different".into(),
            "temporal_scope" => {
                command.assertions[1].claim.temporal_scope = "2026-10-01/2026-11-01".into()
            }
            "polarity" => command.assertions[1].claim.polarity = ClaimPolarity::Negated,
            "epistemic_status" => {
                command.assertions[1].claim.epistemic_status = EpistemicStatus::Tentative
            }
            "qualifiers" => command.assertions[1].claim.qualifiers = vec!["production".into()],
            _ => command
                .assertions
                .iter_mut()
                .for_each(|a| a.claim.epistemic_status = EpistemicStatus::Unknown),
        }
        assert_eq!(
            store
                .write_consolidation(command)
                .await
                .expect("separate")
                .claims
                .len(),
            2,
            "{field}"
        );
    }
}

#[tokio::test]
async fn reused_key_with_different_payload_is_a_conflict() {
    let (_dir, store, mut command) = seeded().await;
    store
        .write_consolidation(command.clone())
        .await
        .expect("first");
    command.author = "different".into();
    assert!(matches!(
        store.write_consolidation(command).await,
        Err(PortError::Conflict(_))
    ));
}

#[tokio::test]
async fn source_limits_and_unknown_fields_fail_explicitly() {
    let (_dir, store, command) = seeded().await;
    assert!(
        store
            .consolidation_sources(ABOUT.into(), vec![A.into(); 65])
            .await
            .is_err()
    );
    let mut json = serde_json::to_value(command).expect("json");
    json["as_of"] = "2026-09-01T00:00:00Z".into();
    assert!(serde_json::from_value::<ConsolidationWrite>(json).is_err());
}
