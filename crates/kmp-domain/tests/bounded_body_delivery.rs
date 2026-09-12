//! Bounded canonical delivery, manifest-bound named expansion and compact
//! reuse, over a recording snapshot that fails the test if a body it was not
//! supposed to read is read.

use kmp_domain::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

const ABOUT: &str = "project:test";
/// Two entries on one path plus the evidence source they share. The source is
/// the large record: it is what a shared-source selection actually pays for.
const ENTRIES: [&str; 2] = ["a", "b"];

fn body_text(id: &str) -> String {
    match id {
        "source" => "S".repeat(400),
        _ => format!("Canonical body of {id}, long enough to be worth a card."),
    }
}

fn record_bytes(id: &str) -> u64 {
    // Records are longer than the text they carry: framing and escaping. The
    // ceiling spends these, not the UTF-8 count.
    body_text(id).len() as u64 + 48
}

struct Snapshot {
    nodes: BTreeMap<String, NodeProjection>,
    edges: Vec<NodeRelationProjection>,
    cards: BTreeMap<(String, String), NodeCard>,
    /// Bodies whose record the store does not hold at all.
    without_body: BTreeSet<String>,
    /// Every id ever passed to `bodies`, in order, so a test can prove a body
    /// was not read rather than assert on a byte total that could hide it.
    body_reads: RefCell<Vec<String>>,
    body_calls: RefCell<u32>,
    descriptor_calls: RefCell<u32>,
    card_calls: RefCell<u32>,
    /// Ids of the long chain, when this snapshot is a wide one.
    wide_ids: Vec<String>,
}

impl Snapshot {
    fn new() -> Self {
        let nodes = ["a", "b", "source"]
            .into_iter()
            .map(|id| {
                (
                    id.to_string(),
                    NodeProjection {
                        node_id: id.into(),
                        node_kind: if id == "source" {
                            "memory_evidence"
                        } else {
                            "observation"
                        }
                        .into(),
                        title: id.into(),
                        summary: id.into(),
                        status: "ACTIVE".into(),
                        labels: if id == "source" {
                            vec![]
                        } else {
                            vec!["entry".into()]
                        },
                        properties: [("memory_about".into(), ABOUT.into())].into(),
                        provenance: None,
                    },
                )
            })
            .collect();
        let edges = [
            ("a", "b", "depends_on"),
            ("source", "a", "supports"),
            ("source", "b", "supports"),
        ]
        .into_iter()
        .map(|(from, to, relation)| NodeRelationProjection {
            source_node_id: from.into(),
            target_node_id: to.into(),
            relation_type: relation.into(),
            explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("The register declares this link.")
                .with_evidence(format!("{from} {relation} {to}"))
                .with_occurred_at("2026-01-01T00:00:00Z"),
        })
        .collect();
        Self {
            nodes,
            edges,
            cards: BTreeMap::new(),
            without_body: BTreeSet::new(),
            body_reads: RefCell::default(),
            body_calls: RefCell::default(),
            descriptor_calls: RefCell::default(),
            card_calls: RefCell::default(),
            wide_ids: Vec::new(),
        }
    }

    fn with_card(mut self, id: &str, text: &str, digest: &str) -> Self {
        self.cards.insert(
            (id.to_string(), "es".to_string()),
            NodeCard {
                node_id: id.into(),
                language: "es".into(),
                text: text.into(),
                source_revision: 1,
                source_content_hash: format!("public:{id}"),
                source_record_digest: digest.into(),
                source_body_bytes: body_text(id).len() as u64,
                authored_by: "reader".into(),
                authored_at: "2026-06-01T00:00:00Z".into(),
                card_revision: 1,
            },
        );
        self
    }

    fn descriptor(&self, id: &str) -> Option<NodeBodyDescriptor> {
        (!self.without_body.contains(id) && self.nodes.contains_key(id)).then(|| {
            NodeBodyDescriptor {
                node_id: id.into(),
                revision: 1,
                content_hash: format!("public:{id}"),
                record_bytes: record_bytes(id),
                body_bytes: body_text(id).len() as u64,
                record_digest: format!("sha256:{id}"),
            }
        })
    }

    fn read_bodies(&self) -> Vec<String> {
        self.body_reads.borrow().clone()
    }

    /// A chain of `count` entries, so the proof table is larger than one
    /// expansion batch. Everything else behaves exactly as above.
    fn wide(count: usize) -> Self {
        let mut snapshot = Snapshot::new();
        snapshot.nodes.clear();
        snapshot.edges.clear();
        let ids: Vec<String> = (0..count).map(|index| format!("wide-{index:03}")).collect();
        for id in &ids {
            snapshot.nodes.insert(
                id.clone(),
                NodeProjection {
                    node_id: id.clone(),
                    node_kind: "observation".into(),
                    title: id.clone(),
                    summary: id.clone(),
                    status: "ACTIVE".into(),
                    labels: vec!["entry".into()],
                    properties: [("memory_about".into(), ABOUT.into())].into(),
                    provenance: None,
                },
            );
        }
        for pair in ids.windows(2) {
            snapshot.edges.push(NodeRelationProjection {
                source_node_id: pair[0].clone(),
                target_node_id: pair[1].clone(),
                relation_type: "depends_on".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                    .with_rationale("The register declares this link.")
                    .with_evidence(format!("{} depends_on {}", pair[0], pair[1]))
                    .with_occurred_at("2026-01-01T00:00:00Z"),
            });
        }
        snapshot.wide_ids = ids;
        snapshot
    }

    fn wide_request(&self) -> TraceSearchRequest {
        self.wide_request_with(TraceBodyOptions {
            refs: Some(BTreeSet::new()),
            ..TraceBodyOptions::default()
        })
    }

    fn wide_request_with(&self, body: TraceBodyOptions) -> TraceSearchRequest {
        let last = self.wide_ids.last().expect("a chain").clone();
        TraceSearchRequest {
            from: self.wide_ids[0].clone(),
            targets: [last].into(),
            limits: TraceSearchLimits {
                nodes: 4096,
                edges: 8192,
                depth: 1024,
                states: 32768,
            },
            relations: Default::default(),
            ..request(body)
        }
    }
}

impl TraceSnapshotReader for Snapshot {
    fn node(&self, id: &str) -> Result<Option<NodeProjection>, PortError> {
        Ok(self.nodes.get(id).cloned())
    }

    fn adjacency(&self, request: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        let mut rows: Vec<_> = self
            .edges
            .iter()
            .filter_map(|edge| {
                let (from, to) = match request.direction() {
                    RelationDirection::Outgoing => (&edge.source_node_id, &edge.target_node_id),
                    RelationDirection::Incoming => (&edge.target_node_id, &edge.source_node_id),
                };
                if from != request.node_id()
                    || request
                        .relation_type()
                        .is_some_and(|wanted| wanted != edge.relation_type)
                {
                    return None;
                }
                let key = (to.clone(), edge.relation_type.clone());
                if request
                    .after()
                    .is_some_and(|at| key <= (at.neighbor.clone(), at.relation.clone()))
                {
                    return None;
                }
                Some((key, edge.clone()))
            })
            .collect();
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        rows.truncate(request.limit() as usize);
        let exhausted = rows.len() < request.limit() as usize;
        let next = if exhausted {
            None
        } else {
            rows.last()
                .map(|((neighbor, relation), _)| RelationPosition {
                    neighbor: neighbor.clone(),
                    relation: relation.clone(),
                })
        };
        Ok(AdjacencyPage {
            edges: rows.into_iter().map(|(_, edge)| edge).collect(),
            next,
            exhausted,
        })
    }

    fn descriptors(&self, ids: &[String]) -> Result<Vec<Option<NodeBodyDescriptor>>, PortError> {
        *self.descriptor_calls.borrow_mut() += 1;
        Ok(ids.iter().map(|id| self.descriptor(id)).collect())
    }

    fn cards(&self, ids: &[String], language: &str) -> Result<Vec<Option<NodeCard>>, PortError> {
        *self.card_calls.borrow_mut() += 1;
        Ok(ids
            .iter()
            .map(|id| self.cards.get(&(id.clone(), language.to_string())).cloned())
            .collect())
    }

    fn verified_bodies(
        &self,
        ids: &[String],
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        self.bodies(ids)
    }

    fn bodies(&self, ids: &[String]) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        *self.body_calls.borrow_mut() += 1;
        self.body_reads.borrow_mut().extend(ids.iter().cloned());
        Ok(ids
            .iter()
            .map(|id| {
                (!self.without_body.contains(id)).then(|| NodeDetailProjection {
                    node_id: id.clone(),
                    detail: body_text(id),
                    content_hash: format!("public:{id}"),
                    revision: 1,
                })
            })
            .collect())
    }
}

fn request(body: TraceBodyOptions) -> TraceSearchRequest {
    TraceSearchRequest {
        proof: true,
        about: ABOUT.into(),
        from: "a".into(),
        targets: ["b".to_string()].into(),
        direction: RelationDirection::Outgoing,
        relations: ["depends_on".to_string()].into(),
        follow: vec![],
        paths_per_target: 1,
        dimensions: Default::default(),
        select: None,
        temporal: Default::default(),
        limits: TraceSearchLimits {
            nodes: 16,
            ..Default::default()
        },
        body,
    }
}

fn run(snapshot: &Snapshot, body: TraceBodyOptions) -> TraceProofResult {
    bounded_trace_search(snapshot, &request(body))
        .expect("search")
        .proof
        .expect("proof")
}

fn object<'a>(proof: &'a TraceProofResult, id: &str) -> &'a TraceProofObject {
    proof
        .objects
        .iter()
        .find(|object| object.node.node_id == id)
        .unwrap_or_else(|| panic!("`{id}` must stay in the response"))
}

#[test]
fn the_no_option_read_is_unchanged_and_consults_no_descriptor() {
    let snapshot = Snapshot::new();

    let proof = run(&snapshot, TraceBodyOptions::default());

    assert_eq!(*snapshot.descriptor_calls.borrow(), 0);
    assert_eq!(*snapshot.card_calls.borrow(), 0);
    assert_eq!(proof.manifest_id, None);
    assert_eq!(proof.delivery, None);
    assert_eq!(proof.compact, None);
    for id in ["a", "b", "source"] {
        let object = object(&proof, id);
        assert_eq!(object.body_state, TraceBodyState::Loaded);
        assert_eq!(object.body.as_ref().expect("body").detail, body_text(id));
    }
    assert_eq!(proof.complete_groups, vec![0]);
}

#[test]
fn a_ceiling_defers_bodies_without_changing_the_path_or_the_selection() {
    let snapshot = Snapshot::new();
    let unbounded = run(&snapshot, TraceBodyOptions::default());
    // The oracle above read every body; only the bounded run below is under
    // observation.
    snapshot.body_reads.borrow_mut().clear();

    // Enough for the two entry records, not for the large shared source.
    let ceiling = record_bytes("a") + record_bytes("b");
    let bounded = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(ceiling),
            ..TraceBodyOptions::default()
        },
    );

    // Discovery is identical: same objects, same order, same supports, same
    // absent refs. Only delivery moved.
    let refs = |proof: &TraceProofResult| -> Vec<String> {
        proof
            .objects
            .iter()
            .map(|object| object.node.node_id.clone())
            .collect()
    };
    assert_eq!(refs(&bounded), refs(&unbounded));
    assert_eq!(bounded.supports, unbounded.supports);
    assert_eq!(bounded.missing_refs, unbounded.missing_refs);
    assert_eq!(bounded.missing_bodies, unbounded.missing_bodies);
    assert_eq!(
        bounded.clock_unknown_entries,
        unbounded.clock_unknown_entries
    );

    assert_eq!(
        object(&bounded, "source").body_state,
        TraceBodyState::DeferredBudget
    );
    assert_eq!(object(&bounded, "source").body, None);
    assert!(
        !snapshot.read_bodies().contains(&"source".to_string()),
        "a deferred body is not read; it is not read and then dropped"
    );
    assert_eq!(
        object(&bounded, "source").required_record_bytes(),
        Some(record_bytes("source")),
        "the object names its exact requirement"
    );

    // Honest accounting: loaded bytes exclude the deferred body, the
    // descriptor total includes it, and the group is no longer complete.
    let delivery = bounded.delivery.as_ref().expect("delivery");
    assert_eq!(delivery.loaded, 2);
    assert_eq!(delivery.deferred_budget, 1);
    assert_eq!(delivery.admitted_record_bytes, ceiling);
    assert_eq!(
        delivery.selected_body_bytes, unbounded.body_bytes,
        "the descriptor total equals what an unbounded read would load"
    );
    assert!(bounded.body_bytes < unbounded.body_bytes);
    assert_eq!(
        delivery.rerun_record_bytes,
        Some(ceiling + record_bytes("source"))
    );
    assert_eq!(delivery.named_record_bytes, Some(record_bytes("source")));
    assert_eq!(
        bounded.complete_groups,
        Vec::<u32>::new(),
        "a group whose shared source was withheld is not fetched proof"
    );
    for entry in ENTRIES {
        assert!(
            bounded.incomplete_entries.contains(&entry.to_string()),
            "the gap propagates from the shared source to every entry it supports"
        );
    }
    assert!(
        bounded.missing_bodies.is_empty(),
        "a withheld body is never reported as absent evidence"
    );
}

#[test]
fn a_named_expansion_recovers_the_deferred_body_and_reads_nothing_else() {
    let snapshot = Snapshot::new();
    let ceiling = record_bytes("a") + record_bytes("b");
    let first = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(ceiling),
            ..TraceBodyOptions::default()
        },
    );
    let manifest = first.manifest_id.clone().expect("manifest");
    snapshot.body_reads.borrow_mut().clear();

    let expansion = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(record_bytes("source")),
            refs: Some(BTreeSet::from(["source".to_string()])),
            expect_selection: Some(manifest.clone()),
            ..TraceBodyOptions::default()
        },
    );

    assert_eq!(expansion.manifest_id.as_deref(), Some(manifest.as_str()));
    assert_eq!(expansion.refusal, None);
    assert_eq!(snapshot.read_bodies(), vec!["source".to_string()]);
    assert_eq!(
        object(&expansion, "source")
            .body
            .as_ref()
            .expect("body")
            .detail,
        body_text("source")
    );
    for entry in ENTRIES {
        assert_eq!(
            object(&expansion, entry).body_state,
            TraceBodyState::NotRequested,
            "a body this batch did not name is not silently reloaded"
        );
        assert_eq!(object(&expansion, entry).body, None);
    }

    // Joined by ref across the two responses, every canonical byte of the
    // selection is recovered exactly once.
    let mut recovered: BTreeMap<String, String> = BTreeMap::new();
    for proof in [&first, &expansion] {
        for object in &proof.objects {
            if let Some(body) = &object.body {
                assert!(
                    recovered
                        .insert(object.node.node_id.clone(), body.detail.clone())
                        .is_none(),
                    "no body was delivered twice"
                );
            }
        }
    }
    assert_eq!(recovered.len(), 3);
    for id in ["a", "b", "source"] {
        assert_eq!(recovered[id], body_text(id));
    }
}

#[test]
fn an_expansion_of_a_selection_that_moved_is_refused_before_any_body_is_read() {
    let snapshot = Snapshot::new();
    let first = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(record_bytes("a")),
            ..TraceBodyOptions::default()
        },
    );
    let manifest = first.manifest_id.clone().expect("manifest");

    // The store moves: one entry loses its body. Same query, same refs.
    let mut moved = Snapshot::new();
    moved.without_body.insert("b".into());

    let expansion = run(
        &moved,
        TraceBodyOptions {
            max_record_bytes: Some(4096),
            refs: Some(BTreeSet::from(["source".to_string()])),
            expect_selection: Some(manifest.clone()),
            ..TraceBodyOptions::default()
        },
    );

    let TraceExpansionRefusal::SelectionChanged { expected, actual } = expansion
        .refusal
        .as_ref()
        .expect("the selection changed and must be refused")
    else {
        panic!("a moved selection is refused as such, not as unknown refs");
    };
    assert_eq!(*expected, manifest);
    assert_ne!(*actual, manifest);
    assert!(expansion.objects.is_empty(), "no text of either selection");
    assert!(expansion.supports.is_empty());
    assert!(
        moved.read_bodies().is_empty(),
        "the refusal happens before a body is read"
    );
}

#[test]
fn a_valid_card_stands_for_a_body_without_reading_one_canonical_record() {
    let snapshot = Snapshot::new()
        .with_card("a", "Resumen de a.", "sha256:a")
        .with_card("b", "Resumen de b.", "sha256:b")
        .with_card("source", "Resumen de la fuente.", "sha256:source");

    let compact = run(
        &snapshot,
        TraceBodyOptions {
            compact: Some("es".into()),
            ..TraceBodyOptions::default()
        },
    );

    assert!(
        snapshot.read_bodies().is_empty(),
        "every body was carded, so no canonical record was read at all"
    );
    for id in ["a", "b", "source"] {
        let object = object(&compact, id);
        assert_eq!(object.body_state, TraceBodyState::Compact);
        assert_eq!(object.body, None, "a card never carries canonical text");
        let card = object.card.as_ref().expect("card");
        assert_eq!(card.status, NodeCardStatus::Valid);
        assert!(card.text.is_some());
        assert_eq!(
            object.descriptor.as_ref().expect("descriptor").revision,
            1,
            "source identity comes from the descriptor, not from the card"
        );
    }
    let summary = compact.compact.as_ref().expect("compact summary");
    assert_eq!(summary.valid, 3);
    assert_eq!(
        summary.body_bytes_omitted,
        ["a", "b", "source"]
            .iter()
            .map(|id| body_text(id).len() as u64)
            .sum::<u64>()
    );
    assert_eq!(compact.body_bytes, 0, "nothing canonical was loaded");
    assert_eq!(
        compact.complete_groups,
        Vec::<u32>::new(),
        "cards are orientation; they do not close a canonical proof group"
    );
}

#[test]
fn an_unusable_card_never_falls_back_to_the_canonical_body() {
    // One stale card, one absent. Falling back here would load the canonical
    // record exactly when a card stopped being usable — including the large
    // shared source, which is the cost this path exists to avoid.
    let snapshot = Snapshot::new()
        .with_card("a", "Resumen viejo.", "sha256:old")
        .with_card("source", "Resumen de la fuente.", "sha256:source");

    let compact = run(
        &snapshot,
        TraceBodyOptions {
            compact: Some("es".into()),
            ..TraceBodyOptions::default()
        },
    );

    assert!(
        snapshot.read_bodies().is_empty(),
        "a compact read loads no canonical record, usable card or not"
    );
    assert_eq!(compact.body_bytes, 0);

    let stale = object(&compact, "a");
    let card = stale.card.as_ref().expect("card");
    assert_eq!(card.status, NodeCardStatus::Stale);
    assert_eq!(card.text, None, "stale prose is never returned");
    assert_eq!(
        card.stored.as_ref().expect("stamp").source_record_digest,
        "sha256:old",
        "the reader learns which version the card describes"
    );
    assert_eq!(stale.body_state, TraceBodyState::NotRequested);
    assert_eq!(stale.body, None);
    assert_eq!(
        stale.required_record_bytes(),
        Some(record_bytes("a")),
        "and exactly what expanding it would cost"
    );
    assert_eq!(
        stale.descriptor.as_ref().expect("descriptor").revision,
        1,
        "with the identity of the body it did not carry"
    );

    let absent = object(&compact, "b");
    assert_eq!(
        absent.card.as_ref().expect("card").status,
        NodeCardStatus::Absent
    );
    assert_eq!(absent.body_state, TraceBodyState::NotRequested);
    assert_eq!(absent.body, None);

    assert_eq!(
        compact.complete_groups,
        Vec::<u32>::new(),
        "no canonical body was fetched, so no group is fetched proof"
    );
    let summary = compact.compact.as_ref().expect("summary");
    assert_eq!((summary.valid, summary.stale, summary.absent), (1, 1, 1));
    assert_eq!(
        summary.body_bytes_omitted,
        body_text("source").len() as u64,
        "only the body a card actually stood for counts as omitted by a card"
    );
}

#[test]
fn a_card_in_another_language_is_absent_and_still_loads_no_body() {
    let snapshot = Snapshot::new().with_card("a", "Resumen de a.", "sha256:a");

    let compact = run(
        &snapshot,
        TraceBodyOptions {
            compact: Some("en".into()),
            ..TraceBodyOptions::default()
        },
    );

    assert!(snapshot.read_bodies().is_empty());
    let object = object(&compact, "a");
    assert_eq!(
        object.card.as_ref().expect("card").status,
        NodeCardStatus::Absent
    );
    assert_eq!(object.body_state, TraceBodyState::NotRequested);
    assert_eq!(object.body, None);
    assert_eq!(compact.compact.as_ref().expect("summary").absent, 3);
}

#[test]
fn a_named_expansion_beats_a_valid_card_for_the_ref_it_names() {
    // Asking to expand a ref and being handed its card again is not an
    // expansion. The cards of the refs nobody named still stand.
    let snapshot = Snapshot::new()
        .with_card("a", "Resumen de a.", "sha256:a")
        .with_card("source", "Resumen de la fuente.", "sha256:source");
    let first = run(
        &snapshot,
        TraceBodyOptions {
            compact: Some("es".into()),
            ..TraceBodyOptions::default()
        },
    );
    let manifest = first.manifest_id.clone().expect("manifest");
    snapshot.body_reads.borrow_mut().clear();

    let expansion = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(record_bytes("source")),
            refs: Some(BTreeSet::from(["source".to_string()])),
            expect_selection: Some(manifest),
            compact: Some("es".into()),
        },
    );

    assert_eq!(snapshot.read_bodies(), vec!["source".to_string()]);
    let expanded = object(&expansion, "source");
    assert_eq!(expanded.body_state, TraceBodyState::Loaded);
    assert_eq!(
        expanded.body.as_ref().expect("body").detail,
        body_text("source")
    );
    assert_eq!(
        object(&expansion, "a").body_state,
        TraceBodyState::Compact,
        "a ref nobody named keeps its card"
    );
}

#[test]
fn an_expansion_naming_a_ref_outside_the_selection_delivers_nothing() {
    let snapshot = Snapshot::new();
    let first = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(8192),
            ..TraceBodyOptions::default()
        },
    );
    let manifest = first.manifest_id.clone().expect("manifest");
    snapshot.body_reads.borrow_mut().clear();

    let refused = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(8192),
            // One inside the selection, one outside it.
            refs: Some(BTreeSet::from([
                "source".to_string(),
                "somebody:elses:ref".to_string(),
            ])),
            expect_selection: Some(manifest),
            ..TraceBodyOptions::default()
        },
    );

    let TraceExpansionRefusal::UnknownRefs(unknown) = refused.refusal.as_ref().expect("refused")
    else {
        panic!("a ref outside the selection is refused as such");
    };
    assert_eq!(unknown, &vec!["somebody:elses:ref".to_string()]);
    assert!(refused.objects.is_empty(), "the whole batch is refused");
    assert!(
        snapshot.read_bodies().is_empty(),
        "not even the valid half of the batch is delivered"
    );
}

#[test]
fn a_body_the_store_lacks_stays_missing_under_every_option() {
    let mut snapshot = Snapshot::new();
    snapshot.without_body.insert("source".into());

    let bounded = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(4096),
            ..TraceBodyOptions::default()
        },
    );

    assert_eq!(
        object(&bounded, "source").body_state,
        TraceBodyState::Missing
    );
    assert_eq!(bounded.missing_bodies, vec!["source".to_string()]);
    assert_eq!(
        bounded.delivery.as_ref().expect("delivery").missing,
        1,
        "an absent body is counted as absent, never as deferred"
    );
}

#[test]
fn the_manifest_ignores_the_ceiling_and_the_requested_subset_but_not_the_selection() {
    let snapshot = Snapshot::new();

    let wide = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(8192),
            ..TraceBodyOptions::default()
        },
    );
    let narrow = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(record_bytes("a")),
            ..TraceBodyOptions::default()
        },
    );
    let carded = run(
        &Snapshot::new().with_card("a", "Resumen de a.", "sha256:a"),
        TraceBodyOptions {
            max_record_bytes: Some(8192),
            compact: Some("es".into()),
            ..TraceBodyOptions::default()
        },
    );

    assert_eq!(wide.manifest_id, narrow.manifest_id);
    assert_eq!(
        wide.manifest_id, carded.manifest_id,
        "authoring a card does not invalidate another reader's expansion"
    );

    let mut moved = Snapshot::new();
    moved.without_body.insert("b".into());
    let after = run(
        &moved,
        TraceBodyOptions {
            max_record_bytes: Some(8192),
            ..TraceBodyOptions::default()
        },
    );
    assert_ne!(wide.manifest_id, after.manifest_id);
}

#[test]
fn a_changed_cut_over_the_same_selection_is_a_different_manifest() {
    // The refs, the relations and the descriptors are identical; only the
    // instant the reader stands at moved. An expansion of one must not be
    // accepted as a continuation of the other.
    let snapshot = Snapshot::new();
    let frontier = run(
        &snapshot,
        TraceBodyOptions {
            max_record_bytes: Some(8192),
            ..TraceBodyOptions::default()
        },
    );

    let mut historical = request(TraceBodyOptions {
        max_record_bytes: Some(8192),
        ..TraceBodyOptions::default()
    });
    historical.temporal = TemporalSelection::as_of(
        TemporalCursor::Time("2026-06-01T00:00:00Z".into()),
        TemporalAxis::Occurred,
    )
    .expect("cut");
    let cut = bounded_trace_search(&snapshot, &historical)
        .expect("search")
        .proof
        .expect("proof");

    assert_ne!(
        frontier.manifest_id, cut.manifest_id,
        "the bound query is part of the selection's identity, not only its result"
    );
}

/// The seek mode, over the same graph, so the manifest's query binding and
/// its candidate coverage can be exercised where the review found them thin.
mod seek {
    use super::*;

    fn seek_request(body: TraceBodyOptions) -> EvidencePathRequest {
        EvidencePathRequest {
            proof: true,
            about: ABOUT.into(),
            from: "a".into(),
            roles: vec![EvidencePathRole {
                name: "cause".into(),
                context: false,
                bindings: vec![],
                steps: vec![TraceRelationStep {
                    relation: MemoryRelationType::new("depends_on").expect("relation"),
                    direction: RelationDirection::Outgoing,
                }],
            }],
            constants: Default::default(),
            temporal: Default::default(),
            limits: TraceSearchLimits {
                nodes: 16,
                ..Default::default()
            },
            body,
        }
    }

    fn run_seek(snapshot: &Snapshot, request: &EvidencePathRequest) -> TraceProofResult {
        search_evidence_paths(snapshot, request)
            .expect("seek")
            .proof
            .expect("proof")
    }

    #[test]
    fn a_changed_seek_request_over_the_same_graph_is_a_different_manifest() {
        let snapshot = Snapshot::new();
        let frontier = run_seek(
            &snapshot,
            &seek_request(TraceBodyOptions {
                max_record_bytes: Some(8192),
                ..TraceBodyOptions::default()
            }),
        );

        // Same graph, same relations, same refs; a different instant to stand
        // at. Nothing in the result table distinguishes them.
        let mut historical = seek_request(TraceBodyOptions {
            max_record_bytes: Some(8192),
            ..TraceBodyOptions::default()
        });
        historical.temporal = TemporalSelection::as_of(
            TemporalCursor::Time("2026-06-01T00:00:00Z".into()),
            TemporalAxis::Occurred,
        )
        .expect("cut");
        let cut = run_seek(&snapshot, &historical);
        assert_ne!(frontier.manifest_id, cut.manifest_id);

        // And a different role name is a different question, even when it
        // selects the same edges.
        let mut renamed = seek_request(TraceBodyOptions {
            max_record_bytes: Some(8192),
            ..TraceBodyOptions::default()
        });
        renamed.roles[0].name = "effect".into();
        assert_ne!(
            frontier.manifest_id,
            run_seek(&snapshot, &renamed).manifest_id
        );
    }

    #[test]
    fn seek_delivery_options_never_change_the_manifest() {
        let snapshot = Snapshot::new();
        let wide = run_seek(
            &snapshot,
            &seek_request(TraceBodyOptions {
                max_record_bytes: Some(8192),
                ..TraceBodyOptions::default()
            }),
        );
        let narrow = run_seek(
            &snapshot,
            &seek_request(TraceBodyOptions {
                refs: Some(BTreeSet::new()),
                ..TraceBodyOptions::default()
            }),
        );
        let carded = run_seek(
            &Snapshot::new().with_card("a", "Resumen de a.", "sha256:a"),
            &seek_request(TraceBodyOptions {
                max_record_bytes: Some(8192),
                compact: Some("es".into()),
                ..TraceBodyOptions::default()
            }),
        );

        assert_eq!(wide.manifest_id, narrow.manifest_id);
        assert_eq!(
            wide.manifest_id, carded.manifest_id,
            "a ceiling, a named subset and a card are delivery, not selection"
        );
    }

    #[test]
    fn a_seek_expansion_of_a_moved_selection_reads_no_body_and_no_card() {
        let snapshot = Snapshot::new().with_card("a", "Resumen de a.", "sha256:a");
        let first = run_seek(
            &snapshot,
            &seek_request(TraceBodyOptions {
                max_record_bytes: Some(8192),
                ..TraceBodyOptions::default()
            }),
        );
        let manifest = first.manifest_id.clone().expect("manifest");

        // The store moves outside the materialized proof table: one body goes.
        let mut moved = Snapshot::new().with_card("a", "Resumen de a.", "sha256:a");
        moved.without_body.insert("b".into());

        let refused = run_seek(
            &moved,
            &seek_request(TraceBodyOptions {
                max_record_bytes: Some(8192),
                refs: Some(BTreeSet::from(["a".to_string()])),
                expect_selection: Some(manifest),
                compact: Some("es".into()),
            }),
        );

        assert!(matches!(
            refused.refusal,
            Some(TraceExpansionRefusal::SelectionChanged { .. })
        ));
        assert!(refused.objects.is_empty());
        assert!(
            moved.read_bodies().is_empty(),
            "refused before a single body was read"
        );
        assert_eq!(
            *moved.card_calls.borrow(),
            0,
            "and before a single card was read"
        );
    }
}

/// The plan the kernel computes over the whole selection, which is what makes
/// recovery independent of how a response is partitioned.
mod plan {
    use super::*;

    fn plan_of(proof: &TraceProofResult) -> &TraceExpansionPlan {
        proof
            .expansion_plan
            .as_ref()
            .expect("a plan while bodies remain")
    }

    #[test]
    fn a_descriptor_only_read_plans_the_head_of_the_selection() {
        let snapshot = Snapshot::new();

        let proof = run(
            &snapshot,
            TraceBodyOptions {
                refs: Some(BTreeSet::new()),
                ..TraceBodyOptions::default()
            },
        );

        let plan = plan_of(&proof);
        assert_eq!(plan.refs, vec!["a", "b", "source"]);
        assert_eq!(
            plan.record_bytes,
            ["a", "b", "source"]
                .iter()
                .map(|id| record_bytes(id))
                .sum::<u64>(),
            "with no ceiling the plan carries its own exact total"
        );
        assert_eq!(plan.oversized, None);
    }

    #[test]
    fn the_plan_never_exceeds_the_public_expansion_limit() {
        // A selection larger than one batch. Naming every pending ref would
        // produce a call the kernel itself refuses.
        let snapshot = Snapshot::wide(MAX_EXPANSION_REFS + 60);
        let request = snapshot.wide_request();

        let search = bounded_trace_search(&snapshot, &request).expect("search");
        let proof = search.proof.as_ref().expect("proof");
        assert!(
            proof.objects.len() > MAX_EXPANSION_REFS,
            "the fixture must exceed one batch: {} objects",
            proof.objects.len()
        );

        assert_eq!(plan_of(proof).refs.len(), MAX_EXPANSION_REFS);
    }

    #[test]
    fn following_the_plan_recovers_every_body_once_whatever_the_page_size() {
        // The chain is driven only by each response's plan. Nothing here reads
        // objects, which is exactly the position a paginated consumer is in.
        let snapshot = Snapshot::wide(MAX_EXPANSION_REFS + 60);
        let ceiling = record_bytes("wide-000") * 70;
        let mut recovered: Vec<String> = Vec::new();
        let mut refs: Option<BTreeSet<String>> = Some(BTreeSet::new());
        let mut manifest: Option<String> = None;

        for round in 0..12 {
            let proof = bounded_trace_search(
                &snapshot,
                &snapshot.wide_request_with(TraceBodyOptions {
                    max_record_bytes: Some(ceiling),
                    // A named batch declares the manifest it continues, which
                    // is what the kernel checks before reading anything.
                    expect_selection: refs
                        .as_ref()
                        .filter(|refs| !refs.is_empty())
                        .and(manifest.clone()),
                    refs: refs.clone(),
                    compact: None,
                }),
            )
            .expect("search")
            .proof
            .expect("proof");
            manifest = proof.manifest_id.clone();

            for object in &proof.objects {
                if object.body_state == TraceBodyState::Loaded {
                    let reference = object.node.node_id.clone();
                    assert!(
                        !recovered.contains(&reference),
                        "round {round} delivered `{reference}` twice"
                    );
                    recovered.push(reference);
                }
            }
            let Some(plan) = &proof.expansion_plan else {
                break;
            };
            assert_eq!(
                plan.record_bytes, ceiling,
                "the ceiling never grows to make progress"
            );
            assert!(plan.refs.len() <= MAX_EXPANSION_REFS);
            refs = Some(plan.refs.iter().cloned().collect());
        }

        let mut once = recovered.clone();
        once.sort();
        once.dedup();
        assert_eq!(once.len(), recovered.len(), "no body was delivered twice");
        assert_eq!(
            once.len(),
            MAX_EXPANSION_REFS + 60,
            "every body of the selection was recovered: {} of {}",
            once.len(),
            MAX_EXPANSION_REFS + 60
        );
    }
}
