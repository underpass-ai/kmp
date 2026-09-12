use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::*;
use std::collections::BTreeSet;

const ABOUT: &str = "project:proof";
const EARLY: &str = "2026-09-01T10:00:00Z";
const CUT: &str = "2026-09-01T12:00:00Z";
const LATE: &str = "2026-09-02T10:00:00Z";

fn object(id: &str, kind: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: kind.into(),
        title: id.into(),
        summary: format!("Summary {id}"),
        status: "ACTIVE".into(),
        labels: if kind == "observation" {
            vec!["entry".into()]
        } else {
            vec![]
        },
        properties: [
            ("memory_about".into(), ABOUT.into()),
            ("source".into(), format!("register:{id}")),
        ]
        .into(),
        provenance: None,
    })
}

fn body(id: &str, text: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
        node_id: id.into(),
        detail: text.into(),
        content_hash: format!("hash:{id}"),
        revision: 7,
    })
}

fn link(from: &str, to: &str, rel: &str, at: Option<&str>) -> ProjectionMutation {
    let mut explanation = RelationExplanation::new(RelationSemanticClass::Evidential)
        .with_rationale("The signed register explicitly links these records.")
        .with_evidence(format!("Register: {from} {rel} {to}."));
    if let Some(at) = at {
        explanation = explanation.with_observed_at(at);
    }
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: from.into(),
        target_node_id: to.into(),
        relation_type: rel.into(),
        explanation,
    }))
}

fn request() -> TraceSearchRequest {
    TraceSearchRequest {
        proof: true,
        about: ABOUT.into(),
        from: "a".into(),
        targets: ["b".into(), "c".into()].into(),
        direction: RelationDirection::Outgoing,
        relations: ["depends_on".into()].into(),
        follow: vec![],
        paths_per_target: 1,
        dimensions: Default::default(),
        select: None,
        limits: Default::default(),
        temporal: Default::default(),
        body: Default::default(),
    }
}

async fn fixture() -> (tempfile::TempDir, EmbeddedKernelStore) {
    let dir = tempfile::tempdir().expect("valid proof fixture");
    let store = EmbeddedKernelStore::open(dir.path()).expect("valid proof fixture");
    let mut changes = vec![
        object("source", "memory_evidence"),
        body("source", "Signed source: π = 3.14; exact bytes."),
        link("a", "b", "depends_on", Some(EARLY)),
        link("a", "c", "depends_on", Some(EARLY)),
    ];
    let label = MemoryDimensionIdentity::new(ABOUT, "work", "one")
        .expect("valid proof fixture")
        .node_id();
    for id in ["a", "b", "c"] {
        changes.extend([
            object(id, "observation"),
            body(id, &format!("Canonical {id}")),
            link("source", id, "supports", Some(EARLY)),
            ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
                source_node_id: label.clone(),
                target_node_id: id.into(),
                relation_type: "contains_entry".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("work")
                    .with_scope_id(label.clone())
                    .with_observed_at(EARLY),
            })),
        ]);
    }
    store
        .apply_mutations(changes)
        .await
        .expect("valid proof fixture");
    (dir, store)
}

#[tokio::test]
async fn shared_sources_are_read_once_with_canonical_bodies_and_complete_and_groups() {
    let (_dir, store) = fixture().await;
    let mut q = request();
    q.select = Some(TraceMaterialSelection {
        max_nodes: 3,
        max_paths: 2,
        groups: vec![TraceProofRequirement {
            alternatives: vec![["b".into(), "c".into()].into()],
            weight: 1,
        }],
    });
    q.proof = false;
    let before = store
        .load_bounded_trace(&q)
        .await
        .expect("valid proof fixture");
    q.proof = true;
    let after = store
        .load_bounded_trace(&q)
        .await
        .expect("valid proof fixture");
    assert_eq!(before.routes, after.routes);
    assert_eq!(before.relations, after.relations);
    assert!(before.proof.is_none());
    let proof = after.proof.expect("valid proof fixture");
    assert_eq!(proof.objects.len(), 4);
    assert_eq!(proof.supports.len(), 3);
    assert_eq!(proof.complete_groups, [0]);
    assert!(proof.incomplete_groups.is_empty());
    assert!(proof.incomplete_entries.is_empty());
    let source = proof
        .objects
        .iter()
        .find(|o| o.node.node_id == "source")
        .expect("valid proof fixture");
    let expected = store
        .load_node_detail("source")
        .await
        .expect("valid proof fixture")
        .expect("valid proof fixture");
    assert_eq!(source.body.as_ref(), Some(&expected));
    assert_eq!(source.node.properties["source"], "register:source");
    assert_eq!(
        proof.body_bytes,
        proof
            .objects
            .iter()
            .map(|o| o.body.as_ref().expect("valid proof fixture").detail.len() as u64)
            .sum::<u64>()
    );
    assert_eq!(after.discovered_nodes, 5);
    assert_eq!(after.scanned_edges, before.scanned_edges + 6);
    assert!(proof.stop.is_none());
}

#[tokio::test]
async fn missing_typed_sources_bodies_and_shared_budgets_remain_partial() {
    let (_dir, store) = fixture().await;
    for nodes in [3, 4] {
        let mut q = request();
        q.limits.nodes = nodes;
        let read = store
            .load_bounded_trace(&q)
            .await
            .expect("valid proof fixture");
        assert!(read.discovered_nodes <= nodes);
        assert!(read.scanned_edges <= q.limits.edges);
        let proof = read.proof.expect("valid proof fixture");
        assert_eq!(proof.stop, Some(TraceSearchStop::NodeBudget));
        assert!(!proof.incomplete_groups.is_empty());
    }
    let mut q = request();
    q.limits.edges = 2;
    let read = store
        .load_bounded_trace(&q)
        .await
        .expect("valid proof fixture");
    assert_eq!(read.scanned_edges, 2);
    assert_eq!(
        read.proof.expect("valid proof fixture").stop,
        Some(TraceSearchStop::EdgeBudget)
    );
    store
        .apply_mutations(vec![
            object("missing-body", "memory_evidence"),
            link("missing-body", "b", "supports", Some(EARLY)),
            link("dangling", "c", "supports", Some(EARLY)),
            body("dangling", "Orphan body must not be retrieved"),
            object("wrong-kind", "observation"),
            link("wrong-kind", "a", "supports", Some(EARLY)),
        ])
        .await
        .expect("valid proof fixture");
    let proof = store
        .load_bounded_trace(&request())
        .await
        .expect("valid proof fixture")
        .proof
        .expect("valid proof fixture");
    assert_eq!(proof.missing_bodies, ["missing-body"]);
    assert_eq!(proof.missing_refs, ["dangling", "wrong-kind"]);
    assert_eq!(proof.incomplete_entries, ["a", "b", "c"]);
    assert!(proof.objects.iter().all(|o| o.node.node_id != "dangling"));
    assert!(proof.complete_groups.is_empty());
}

#[tokio::test]
async fn source_attachment_cut_and_unknown_clocks_qualify_group_completeness() {
    let (_dir, store) = fixture().await;
    store
        .apply_mutations(vec![
            object("late", "memory_evidence"),
            body("late", "Late source"),
            link("late", "b", "supports", Some(LATE)),
            object("undated", "memory_evidence"),
            body("undated", "Undated source"),
            link("undated", "c", "supports", None),
        ])
        .await
        .expect("valid proof fixture");
    let mut q = request();
    q.temporal = TemporalSelection::as_of(
        TemporalCursor::time(CUT).expect("valid proof fixture"),
        TemporalAxis::Observed,
    )
    .expect("valid proof fixture");
    let read = store
        .load_bounded_trace(&q)
        .await
        .expect("valid proof fixture");
    let proof = read.proof.expect("valid proof fixture");
    let sources: BTreeSet<_> = proof
        .supports
        .iter()
        .map(|e| e.source_node_id.as_str())
        .collect();
    assert_eq!(sources, ["source", "undated"].into());
    assert_eq!(proof.clock_unknown_entries, ["c"]);
    assert_eq!(proof.complete_groups, [0]);
    assert_eq!(proof.incomplete_groups, [1]);
    assert!(proof.objects.iter().all(|o| o.node.node_id != "late"));
    q.about = "project:foreign".into();
    assert!(store.load_bounded_trace(&q).await.is_err());
}

fn seek_request() -> EvidencePathRequest {
    EvidencePathRequest {
        proof: true,
        about: ABOUT.into(),
        from: "a".into(),
        roles: vec![EvidencePathRole {
            name: "dependency".into(),
            context: false,
            steps: vec![TraceRelationStep {
                relation: MemoryRelationType::new("depends_on").expect("valid proof fixture"),
                direction: RelationDirection::Outgoing,
            }],
            bindings: vec![],
        }],
        constants: Default::default(),
        temporal: Default::default(),
        limits: Default::default(),
        body: Default::default(),
    }
}

#[tokio::test]
async fn selected_target_groups_keep_original_requirement_indices() {
    let (_dir, store) = fixture().await;
    let mut q = request();
    q.select = Some(TraceMaterialSelection {
        max_nodes: 2,
        max_paths: 1,
        groups: vec![
            TraceProofRequirement {
                weight: 1,
                alternatives: vec![["b".into()].into()],
            },
            TraceProofRequirement {
                weight: 100,
                alternatives: vec![["c".into()].into()],
            },
        ],
    });
    let result = store
        .load_bounded_trace(&q)
        .await
        .expect("valid proof fixture");
    assert_eq!(
        result
            .material
            .as_ref()
            .expect("valid proof fixture")
            .selected_candidates,
        [1]
    );
    let proof = result.proof.expect("valid proof fixture");
    assert_eq!(proof.complete_groups, [1]);
    assert_eq!(proof.incomplete_groups, [0]);
    assert!(!proof.objects.iter().any(|o| o.node.node_id == "b"));
}

#[tokio::test]
async fn seek_materializes_union_and_preserves_unknown_obligations_and_review() {
    let (_dir, store) = fixture().await;
    let mut q = seek_request();
    q.proof = false;
    let plain = store
        .load_evidence_paths(&q)
        .await
        .expect("valid proof fixture");
    q.proof = true;
    let result = store
        .load_evidence_paths(&q)
        .await
        .expect("valid proof fixture");
    assert_eq!(plain.candidates, result.candidates);
    assert_eq!(plain.groups, result.groups);
    assert_eq!(plain.status, result.status);
    let proof = result.proof.expect("valid proof fixture");
    assert_eq!(proof.objects.len(), 4);
    assert_eq!(proof.supports.len(), 3);
    assert_eq!(proof.complete_groups, [0, 1]);
    q.roles[0].bindings.push(EvidencePathBinding::Label {
        at: 1,
        name: "event".into(),
        key: "event".into(),
    });
    let unknown = store
        .load_evidence_paths(&q)
        .await
        .expect("valid proof fixture");
    assert_eq!(unknown.status, EvidencePathStatus::ReviewRequired);
    assert!(
        unknown
            .groups
            .iter()
            .all(|g| !g.bindings.missing.is_empty())
    );
    assert_eq!(
        unknown
            .proof
            .as_ref()
            .expect("valid proof fixture")
            .objects
            .len(),
        4
    );
    assert_eq!(
        unknown
            .proof
            .as_ref()
            .expect("valid proof fixture")
            .incomplete_groups,
        [0, 1]
    );
    q.roles[0].bindings.clear();
    q.roles[0].context = true;
    let context = store
        .load_evidence_paths(&q)
        .await
        .expect("valid proof fixture");
    assert_eq!(context.status, EvidencePathStatus::ReviewRequired);
    assert_eq!(
        context.proof.expect("valid proof fixture").complete_groups,
        [0, 1]
    );
    q.roles[0].steps[0].relation =
        MemoryRelationType::new("verified_by").expect("valid proof fixture");
    let empty = store
        .load_evidence_paths(&q)
        .await
        .expect("valid proof fixture");
    assert!(empty.groups.is_empty());
    let proof = empty.proof.expect("valid proof fixture");
    assert!(proof.objects.is_empty());
    assert!(proof.complete_groups.is_empty());
    assert!(proof.incomplete_groups.is_empty());
}

#[tokio::test]
async fn foreign_and_ownerless_sources_never_expose_payloads_or_certify_proof() {
    let (_dir, store) = fixture().await;
    let mut foreign = match object("foreign", "memory_evidence") {
        ProjectionMutation::UpsertNode(n) => n,
        _ => unreachable!(),
    };
    foreign
        .properties
        .insert("memory_about".into(), "project:private".into());
    let mut ownerless = foreign.clone();
    ownerless.node_id = "ownerless".into();
    ownerless.properties.remove("memory_about");
    store
        .apply_mutations(vec![
            ProjectionMutation::UpsertNode(foreign),
            ProjectionMutation::UpsertNode(ownerless),
            body("foreign", "Private source bytes"),
            body("ownerless", "Unowned bytes"),
            link("foreign", "b", "supports", Some(EARLY)),
            link("ownerless", "c", "supports", Some(EARLY)),
        ])
        .await
        .expect("valid proof fixture");
    for proof in [
        store
            .load_bounded_trace(&request())
            .await
            .expect("valid proof fixture")
            .proof
            .expect("valid proof fixture"),
        store
            .load_evidence_paths(&seek_request())
            .await
            .expect("valid proof fixture")
            .proof
            .expect("valid proof fixture"),
    ] {
        assert_eq!(proof.missing_refs, ["foreign", "ownerless"]);
        assert_eq!(proof.incomplete_entries, ["b", "c"]);
        assert!(proof.complete_groups.is_empty());
        assert!(
            !proof
                .objects
                .iter()
                .any(|o| ["foreign", "ownerless"].contains(&o.node.node_id.as_str()))
        );
        assert!(
            !proof
                .supports
                .iter()
                .any(|e| ["foreign", "ownerless"].contains(&e.source_node_id.as_str()))
        );
    }
}

#[tokio::test]
async fn seek_cutoffs_preserve_groups_without_inventing_missing_objects() {
    let (_dir, store) = fixture().await;
    for (nodes, edges, stop) in [
        (4, 100, TraceSearchStop::NodeBudget),
        (100, 3, TraceSearchStop::EdgeBudget),
    ] {
        let mut q = seek_request();
        q.limits.nodes = nodes;
        q.limits.edges = edges;
        let got = store
            .load_evidence_paths(&q)
            .await
            .expect("valid proof fixture");
        assert_eq!(got.groups.len(), 2);
        assert_eq!(got.stop, stop);
        assert!(got.discovered_nodes <= nodes && got.scanned_edges <= edges);
        assert_eq!(got.status, EvidencePathStatus::Partial);
        let proof = got.proof.expect("valid proof fixture");
        assert_eq!(proof.stop, Some(stop));
        assert!(proof.complete_groups.is_empty());
        assert_eq!(proof.incomplete_groups, [0, 1]);
        assert!(proof.missing_refs.is_empty());
        assert!(proof.missing_bodies.is_empty());
    }
}

fn coordinate_on_all_clocks(id: &str) -> ProjectionMutation {
    let scope = MemoryDimensionIdentity::new(ABOUT, "work", "one")
        .expect("valid proof fixture")
        .node_id();
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: scope.clone(),
        target_node_id: id.into(),
        relation_type: "contains_entry".into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension("work")
            .with_scope_id(scope)
            .with_occurred_at(EARLY)
            .with_observed_at(EARLY)
            .with_ingested_at(EARLY)
            .with_valid_from(EARLY)
            .with_valid_until(LATE),
    }))
}
fn all_clocks(mut mutation: ProjectionMutation) -> ProjectionMutation {
    if let ProjectionMutation::UpsertNodeRelation(e) = &mut mutation {
        e.explanation = e
            .explanation
            .clone()
            .with_occurred_at(EARLY)
            .with_observed_at(EARLY)
            .with_ingested_at(EARLY)
            .with_valid_from(EARLY);
    }
    mutation
}

#[tokio::test]
async fn source_time_never_backdates_attachment_and_missing_selected_clocks_stay_unknown() {
    let (_dir, store) = fixture().await;
    let mut old_source = match object("late-attachment", "memory_evidence") {
        ProjectionMutation::UpsertNode(n) => n,
        _ => unreachable!(),
    };
    old_source.properties.insert("time".into(), EARLY.into());
    let mut changes = vec![
        ProjectionMutation::UpsertNode(old_source),
        body("late-attachment", "Old event; later proof association."),
    ];
    for id in ["a", "b", "c"] {
        changes.push(coordinate_on_all_clocks(id));
        changes.push(all_clocks(link("source", id, "supports", Some(EARLY))));
    }
    changes.extend([
        all_clocks(link("a", "b", "depends_on", Some(EARLY))),
        all_clocks(link("a", "c", "depends_on", Some(EARLY))),
    ]);
    let mut attachment = link("late-attachment", "b", "supports", Some(LATE));
    if let ProjectionMutation::UpsertNodeRelation(e) = &mut attachment {
        e.explanation = e.explanation.clone().with_ingested_at(LATE);
    }
    changes.push(attachment);
    store
        .apply_mutations(changes)
        .await
        .expect("valid proof fixture");
    for axis in [
        TemporalAxis::Observed,
        TemporalAxis::Ingested,
        TemporalAxis::Occurred,
        TemporalAxis::Validity,
    ] {
        let temporal = TemporalSelection::as_of(
            TemporalCursor::time(CUT).expect("valid proof fixture"),
            axis,
        )
        .expect("valid proof fixture");
        let mut t = request();
        t.temporal = temporal.clone();
        let mut s = seek_request();
        s.temporal = temporal;
        for proof in [
            store
                .load_bounded_trace(&t)
                .await
                .expect("valid proof fixture")
                .proof
                .expect("valid proof fixture"),
            store
                .load_evidence_paths(&s)
                .await
                .expect("valid proof fixture")
                .proof
                .expect("valid proof fixture"),
        ] {
            let selected_has_clock =
                matches!(axis, TemporalAxis::Observed | TemporalAxis::Ingested);
            assert_eq!(
                proof
                    .objects
                    .iter()
                    .any(|o| o.node.node_id == "late-attachment"),
                !selected_has_clock
            );
            if selected_has_clock {
                assert_eq!(proof.complete_groups, [0, 1]);
                assert!(proof.clock_unknown_entries.is_empty());
            } else {
                assert_eq!(proof.clock_unknown_entries, ["b"]);
                assert_eq!(proof.complete_groups, [1]);
                assert_eq!(proof.incomplete_groups, [0]);
            }
        }
    }
}

#[tokio::test]
async fn validity_cut_excludes_expired_support_and_preserves_canonical_supersession_status() {
    let (_dir, store) = fixture().await;
    let mut changes = Vec::new();
    for id in ["a", "b", "c"] {
        changes.push(coordinate_on_all_clocks(id));
        changes.push(all_clocks(link("source", id, "supports", Some(EARLY))));
    }
    changes.extend([
        all_clocks(link("a", "b", "depends_on", Some(EARLY))),
        all_clocks(link("a", "c", "depends_on", Some(EARLY))),
        object("expired", "memory_evidence"),
        body("expired", "Proof no longer valid at cut"),
        ProjectionMutation::UpdateNodeStatus {
            node_id: "b".into(),
            status: "SUPERSEDED".into(),
        },
    ]);
    let mut support = all_clocks(link("expired", "b", "supports", Some(EARLY)));
    if let ProjectionMutation::UpsertNodeRelation(e) = &mut support {
        e.explanation = e.explanation.clone().with_valid_until(CUT);
    }
    changes.push(support);
    store
        .apply_mutations(changes)
        .await
        .expect("valid proof fixture");
    let temporal = TemporalSelection::as_of(
        TemporalCursor::time(CUT).expect("valid proof fixture"),
        TemporalAxis::Validity,
    )
    .expect("valid proof fixture");
    let mut t = request();
    t.temporal = temporal.clone();
    let mut s = seek_request();
    s.temporal = temporal;
    for proof in [
        store
            .load_bounded_trace(&t)
            .await
            .expect("valid proof fixture")
            .proof
            .expect("valid proof fixture"),
        store
            .load_evidence_paths(&s)
            .await
            .expect("valid proof fixture")
            .proof
            .expect("valid proof fixture"),
    ] {
        assert_eq!(proof.complete_groups, [0, 1]);
        assert!(!proof.objects.iter().any(|o| o.node.node_id == "expired"));
        assert!(!proof.supports.iter().any(|e| e.source_node_id == "expired"));
        let node = proof
            .objects
            .iter()
            .find(|o| o.node.node_id == "b")
            .expect("valid proof fixture");
        assert_eq!(node.node.status, "SUPERSEDED");
        assert_eq!(node.coordinates.len(), 1);
        assert_eq!(node.coordinates[0].valid_from(), Some(EARLY));
        assert_eq!(node.coordinates[0].valid_until(), Some(LATE));
    }
}
