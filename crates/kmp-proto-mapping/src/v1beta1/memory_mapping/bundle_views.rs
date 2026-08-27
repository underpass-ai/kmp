use std::collections::{BTreeMap, BTreeSet, HashMap};

use kmp_application::RenderedContext;
use kmp_domain::{
    BundleNodeDetail, BundleRelationship, KmpBundle, MemoryRelationType, RelationExplanation,
    TemporalCoordinate,
};
use kmp_proto::v1beta1::{
    MemoryConfidence, MemoryEvidence, MemoryRelation, MemoryRelationExplanation,
    MemorySemanticClass, Proof, SupersededMemory, TemporalCoordinate as ProtoTemporalCoordinate,
};

use super::scalars::{proto_confidence, proto_semantic_class, timestamp_from_sort_or_rfc3339};

pub(super) fn memory_relations_from_bundle(bundle: &KmpBundle) -> Vec<MemoryRelation> {
    bundle
        .relationships()
        .iter()
        .map(memory_relation_from_bundle_relationship)
        .collect()
}

pub(super) fn memory_relation_from_bundle_relationship(
    relationship: &BundleRelationship,
) -> MemoryRelation {
    let explanation = relationship.explanation();
    MemoryRelation {
        source_ref: relationship.source_node_id().to_string(),
        target_ref: relationship.target_node_id().to_string(),
        rel: relationship.relationship_type().to_string(),
        semantic_class: proto_semantic_class(explanation.semantic_class()) as i32,
        why: explanation.rationale().unwrap_or_default().to_string(),
        evidence: explanation.evidence().unwrap_or_default().to_string(),
        confidence: proto_confidence(explanation.confidence()) as i32,
        sequence: explanation.sequence(),
        explanation: proto_relation_explanation(explanation),
        evidence_refs: Vec::new(),
    }
}

pub(super) fn memory_evidence_from_bundle(bundle: &KmpBundle) -> Vec<MemoryEvidence> {
    bundle
        .node_details()
        .iter()
        .map(|detail| evidence_from_detail(bundle, detail, vec![detail.node_id().to_string()]))
        .collect()
}

pub(super) fn answer_evidence_from_bundle(bundle: &KmpBundle) -> Vec<MemoryEvidence> {
    let node_kinds = bundle_node_kinds(bundle);
    let support_targets = support_targets_by_source(bundle);
    let mut candidates = bundle
        .node_details()
        .iter()
        .filter(|detail| {
            node_kinds
                .get(detail.node_id())
                .is_some_and(|kind| is_memory_evidence_kind(kind))
        })
        .map(|detail| {
            let supports = support_targets
                .get(detail.node_id())
                .cloned()
                .unwrap_or_else(|| vec![detail.node_id().to_string()]);
            evidence_from_detail(bundle, detail, supports)
        })
        .collect::<Vec<_>>();

    // An entry is the claim memory makes; its evidence is why that claim was
    // recorded. Ask must search both without conflating them. Keep the public
    // proof shape and mark entry-derived candidates explicitly in metadata.
    let entry_refs = bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
        .map(|relationship| relationship.target_node_id())
        .collect::<BTreeSet<_>>();
    for node in std::iter::once(bundle.root_node()).chain(bundle.neighbor_nodes()) {
        if !entry_refs.contains(node.node_id()) || node.summary().trim().is_empty() {
            continue;
        }
        let properties = node.properties();
        let mut metadata = persisted_memory_metadata(properties);
        metadata.insert("proof_role".to_string(), "entry_text".to_string());
        candidates.push(MemoryEvidence {
            id: format!("entry:{}", node.node_id()),
            supports: vec![node.node_id().to_string()],
            text: node.summary().to_string(),
            source: persisted_memory_source(properties)
                .unwrap_or(node.node_id())
                .to_string(),
            time: timestamp_from_sort_or_rfc3339(
                properties.get("payload_time").map(String::as_str),
            ),
            metadata,
        });
    }
    candidates
}

/// Keeps only graph edges that can audit the selected answer evidence.
///
/// An answer is proved by its evidence node, the claims it supports, and the
/// relations incident to either. The latter deliberately retains conflict and
/// supersession edges for selected claims without leaking unrelated bundle
/// history into `proof.path`.
pub(super) fn answer_relations_from_bundle(
    bundle: &KmpBundle,
    evidence: &[MemoryEvidence],
) -> Vec<MemoryRelation> {
    let selected_refs = evidence
        .iter()
        .flat_map(|item| {
            item.id
                .strip_prefix("detail:")
                .map(str::to_string)
                .into_iter()
                .chain(item.supports.iter().cloned())
        })
        .collect::<BTreeSet<_>>();

    memory_relations_from_bundle(bundle)
        .into_iter()
        .filter(|relationship| {
            selected_refs.contains(&relationship.source_ref)
                || selected_refs.contains(&relationship.target_ref)
        })
        .collect()
}

pub(super) fn temporal_relations_from_bundle(
    bundle: &KmpBundle,
    selected_refs: &BTreeSet<String>,
) -> Vec<MemoryRelation> {
    memory_relations_from_bundle(bundle)
        .into_iter()
        .filter(|relationship| {
            selected_refs.contains(&relationship.source_ref)
                || selected_refs.contains(&relationship.target_ref)
        })
        .collect()
}

pub(super) fn temporal_evidence_from_bundle(
    bundle: &KmpBundle,
    selected_refs: &BTreeSet<String>,
) -> Vec<MemoryEvidence> {
    let node_kinds = bundle_node_kinds(bundle);
    let support_targets = support_targets_by_source(bundle);
    let mut evidence_refs = selected_refs.clone();
    for relationship in bundle.relationships().iter().filter(|relationship| {
        relationship.relationship_type() == "supports"
            && selected_refs.contains(relationship.target_node_id())
            && node_kinds
                .get(relationship.source_node_id())
                .is_some_and(|kind| is_memory_evidence_kind(kind))
    }) {
        evidence_refs.insert(relationship.source_node_id().to_string());
    }

    bundle
        .node_details()
        .iter()
        .filter(|detail| evidence_refs.contains(detail.node_id()))
        .map(|detail| {
            let supports = support_targets
                .get(detail.node_id())
                .cloned()
                .unwrap_or_else(|| vec![detail.node_id().to_string()]);
            evidence_from_detail(bundle, detail, supports)
        })
        .collect()
}

fn evidence_from_detail(
    bundle: &KmpBundle,
    detail: &BundleNodeDetail,
    supports: Vec<String>,
) -> MemoryEvidence {
    let properties = bundle_node_properties(bundle, detail.node_id());
    let mut metadata = properties
        .map(persisted_memory_metadata)
        .unwrap_or_default();
    metadata.entry("proof_role".to_string()).or_insert_with(|| {
        let role = properties
            .and_then(|properties| properties.get("entry_kind"))
            .map(|_| "entry_detail")
            .unwrap_or("stored_evidence");
        role.to_string()
    });
    MemoryEvidence {
        id: format!("detail:{}", detail.node_id()),
        supports,
        text: detail.detail().to_string(),
        source: properties
            .and_then(persisted_memory_source)
            .unwrap_or(detail.node_id())
            .to_string(),
        time: timestamp_from_sort_or_rfc3339(
            properties.and_then(|properties| properties.get("payload_time").map(String::as_str)),
        ),
        metadata,
    }
}

pub(super) fn bundle_memory_metadata(bundle: &KmpBundle, node_id: &str) -> HashMap<String, String> {
    bundle_node_properties(bundle, node_id)
        .map(persisted_memory_metadata)
        .unwrap_or_default()
}

pub(super) fn persisted_memory_metadata(
    properties: &BTreeMap<String, String>,
) -> HashMap<String, String> {
    properties
        .get("payload_metadata")
        .or_else(|| properties.get("metadata"))
        .and_then(|metadata| serde_json::from_str(metadata).ok())
        .unwrap_or_default()
}

pub(super) fn persisted_memory_source(properties: &BTreeMap<String, String>) -> Option<&str> {
    properties
        .get("source")
        .or_else(|| properties.get("payload_source"))
        .map(String::as_str)
        .filter(|source| !source.trim().is_empty())
}

fn bundle_node_properties<'a>(
    bundle: &'a KmpBundle,
    node_id: &str,
) -> Option<&'a BTreeMap<String, String>> {
    std::iter::once(bundle.root_node())
        .chain(bundle.neighbor_nodes())
        .find(|node| node.node_id() == node_id)
        .map(|node| node.properties())
}

fn bundle_node_kinds(bundle: &KmpBundle) -> BTreeMap<&str, &str> {
    let mut node_kinds =
        BTreeMap::from([(bundle.root_node().node_id(), bundle.root_node().node_kind())]);
    for node in bundle.neighbor_nodes() {
        node_kinds.insert(node.node_id(), node.node_kind());
    }
    node_kinds
}

fn support_targets_by_source(bundle: &KmpBundle) -> BTreeMap<&str, Vec<String>> {
    let mut supports = BTreeMap::new();
    for relationship in bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "supports")
    {
        supports
            .entry(relationship.source_node_id())
            .or_insert_with(Vec::new)
            .push(relationship.target_node_id().to_string());
    }
    supports
}

fn is_memory_evidence_kind(kind: &str) -> bool {
    matches!(kind, "memory_evidence" | "evidence")
}

pub(super) fn proof(
    path: Vec<MemoryRelation>,
    evidence: Vec<MemoryEvidence>,
    missing: Vec<String>,
    confidence: MemoryConfidence,
) -> Proof {
    let path = normalize_proof_path(path, &evidence);
    let conflicts = conflicts_from_relations(&path);
    let superseded = superseded_from_relations(&path);
    let frontier_size = missing.len() as u32;
    Proof {
        path,
        evidence,
        conflicts,
        superseded,
        expired: Vec::new(),
        missing,
        confidence: confidence as i32,
        frontier_size,
        matched_terms: Vec::new(),
        matched_relations: Vec::new(),
    }
}

/// Normalizes proof hops around the canonical evidence registry.
///
/// Relation-specific explanations remain inline. A `why` or `evidence` body
/// that exactly repeats `Proof.evidence[].text` becomes a stable ref instead,
/// and every hop incident to a selected evidence/claim carries the refs that
/// make the join explicit.
fn normalize_proof_path(
    mut path: Vec<MemoryRelation>,
    evidence: &[MemoryEvidence],
) -> Vec<MemoryRelation> {
    for relation in &mut path {
        let mut refs = relation
            .evidence_refs
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut repeated_why = false;
        let mut repeated_evidence = false;

        for item in evidence {
            let evidence_node_ref = item.id.strip_prefix("detail:").unwrap_or(&item.id);
            let incident = relation.source_ref == evidence_node_ref
                || relation.target_ref == evidence_node_ref
                || item.supports.iter().any(|supported_ref| {
                    relation.source_ref == *supported_ref || relation.target_ref == *supported_ref
                });
            let why_matches = !relation.why.is_empty() && relation.why == item.text;
            let evidence_matches = !relation.evidence.is_empty() && relation.evidence == item.text;

            if incident || why_matches || evidence_matches {
                refs.insert(item.id.clone());
            }
            repeated_why |= why_matches;
            repeated_evidence |= evidence_matches;
        }

        if repeated_why {
            relation.why.clear();
        }
        if repeated_evidence {
            relation.evidence.clear();
        }
        relation.evidence_refs = refs.into_iter().collect();
        if relation.semantic_class != MemorySemanticClass::Structural as i32
            && relation.why.is_empty()
            && relation.evidence.is_empty()
            && !relation.evidence_refs.is_empty()
        {
            relation.why = "Supported by canonical evidence refs.".to_string();
        }
    }
    path
}

/// Entries this recall reached that a later entry replaced.
///
/// A compensating write carries `supersedes`, and the older entry kept coming
/// back through wake and ask looking exactly like a live one: the supersession
/// was on the relation, reachable by inspecting or rewinding, and invisible to
/// a reader who did neither. Acting on a decision that was already replaced is
/// the one failure an append-only memory should not have.
///
/// Deliberately not folded into `conflicts`. `contradicts` says two entries
/// disagree and both may still be live — the tension is the information.
/// `supersedes` says one replaced the other: no tension, a lifecycle, and the
/// older entry is history rather than advice.
pub(super) fn superseded_from_relations(path: &[MemoryRelation]) -> Vec<SupersededMemory> {
    let mut superseded = path
        .iter()
        .filter(|relation| is_supersession(&relation.rel))
        .map(|relation| SupersededMemory {
            r#ref: relation.target_ref.clone(),
            superseded_by: relation.source_ref.clone(),
            why: if relation.why.trim().is_empty() {
                relation.evidence.trim().to_string()
            } else {
                relation.why.trim().to_string()
            },
        })
        .collect::<Vec<_>>();
    superseded.sort_by(|left, right| {
        (&left.r#ref, &left.superseded_by, &left.why).cmp(&(
            &right.r#ref,
            &right.superseded_by,
            &right.why,
        ))
    });
    superseded.dedup_by(|left, right| left.r#ref == right.r#ref);
    superseded
}

fn is_supersession(value: &str) -> bool {
    MemoryRelationType::new(value).is_ok_and(|relation_type| relation_type.as_str() == "supersedes")
}

fn conflicts_from_relations(path: &[MemoryRelation]) -> Vec<String> {
    path.iter()
        .filter(|relation| is_conflict_relation(&relation.rel))
        .map(conflict_summary)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn is_conflict_relation(value: &str) -> bool {
    MemoryRelationType::new(value).is_ok_and(|relation_type| relation_type.is_conflict())
}

fn conflict_summary(relation: &MemoryRelation) -> String {
    let mut summary = format!(
        "{} {} {}",
        relation.source_ref,
        MemoryRelationType::new(&relation.rel)
            .map(|relation_type| relation_type.as_str().to_string())
            .unwrap_or_else(|_| relation.rel.trim().to_string()),
        relation.target_ref
    );
    if !relation.why.trim().is_empty() {
        summary.push_str(": ");
        summary.push_str(relation.why.trim());
    } else if !relation.evidence.trim().is_empty() {
        summary.push_str(": ");
        summary.push_str(relation.evidence.trim());
    }
    summary
}

pub(super) fn rendered_summary(rendered: &RenderedContext) -> String {
    rendered
        .tiers
        .iter()
        .find(|tier| !tier.content.trim().is_empty())
        .map(|tier| tier.content.clone())
        .or_else(|| {
            rendered
                .sections
                .iter()
                .find(|section| !section.content.trim().is_empty())
                .map(|section| section.content.clone())
        })
        .unwrap_or_else(|| rendered.content.clone())
}

pub(super) fn rendered_current_state(
    rendered: &RenderedContext,
    bundle: &KmpBundle,
) -> Vec<String> {
    let semantic_relationships = bundle
        .relationships()
        .iter()
        .filter(|relationship| {
            relationship.explanation().semantic_class()
                != &kmp_domain::RelationSemanticClass::Structural
        })
        .map(|relationship| {
            format!(
                "rel:{}→{}",
                relationship.source_node_id(),
                relationship.target_node_id()
            )
        })
        .collect::<BTreeSet<_>>();
    let structural_relationships = bundle
        .relationships()
        .iter()
        .filter(|relationship| {
            relationship.explanation().semantic_class()
                == &kmp_domain::RelationSemanticClass::Structural
        })
        .map(|relationship| {
            format!(
                "rel:{}→{}",
                relationship.source_node_id(),
                relationship.target_node_id()
            )
        })
        .collect::<BTreeSet<_>>();
    let mut sections = rendered
        .sections
        .iter()
        .filter(|section| !section.content.trim().is_empty())
        .collect::<Vec<_>>();
    // Minimal wake packets keep the first state item. Prefer the semantic
    // reason the graph changed, then concrete detail, then node anchors;
    // containment bookkeeping remains available but cannot displace state.
    sections.sort_by(|left, right| {
        let priority = |source_id: &str| {
            if semantic_relationships.contains(source_id) {
                0
            } else if source_id.starts_with("detail:") {
                1
            } else if structural_relationships.contains(source_id) {
                3
            } else {
                2
            }
        };
        (priority(&left.source_id), &left.source_id, &left.content).cmp(&(
            priority(&right.source_id),
            &right.source_id,
            &right.content,
        ))
    });
    let sections = sections
        .into_iter()
        .take(5)
        .map(|section| section.content.clone())
        .collect::<Vec<_>>();
    if sections.is_empty() && !rendered.content.trim().is_empty() {
        vec![rendered.content.clone()]
    } else {
        sections
    }
}

pub(super) fn proto_coordinate_from_domain(
    coordinate: &TemporalCoordinate,
) -> ProtoTemporalCoordinate {
    ProtoTemporalCoordinate {
        dimension: coordinate.dimension().to_string(),
        scope_id: coordinate.scope_id().to_string(),
        occurred_at: timestamp_from_sort_or_rfc3339(coordinate.occurred_at()),
        observed_at: timestamp_from_sort_or_rfc3339(coordinate.observed_at()),
        ingested_at: timestamp_from_sort_or_rfc3339(coordinate.ingested_at()),
        valid_from: timestamp_from_sort_or_rfc3339(coordinate.valid_from()),
        valid_until: timestamp_from_sort_or_rfc3339(coordinate.valid_until()),
        sequence: coordinate.sequence(),
        rank: coordinate.rank(),
        metadata: Default::default(),
    }
}

pub(super) fn proto_relation_explanation(
    explanation: &RelationExplanation,
) -> Option<MemoryRelationExplanation> {
    let value = MemoryRelationExplanation {
        motivation: explanation.motivation().unwrap_or_default().to_string(),
        method: explanation.method().unwrap_or_default().to_string(),
        decision_id: explanation.decision_id().unwrap_or_default().to_string(),
        caused_by_node_id: explanation
            .caused_by_node_id()
            .unwrap_or_default()
            .to_string(),
        coordinate: TemporalCoordinate::from_relation_explanation(explanation)
            .ok()
            .flatten()
            .map(|coordinate| proto_coordinate_from_domain(&coordinate)),
    };
    (!value.motivation.is_empty()
        || !value.method.is_empty()
        || !value.decision_id.is_empty()
        || !value.caused_by_node_id.is_empty()
        || value.coordinate.is_some())
    .then_some(value)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use kmp_application::queries::render_graph_bundle;
    use kmp_domain::{
        BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId,
        RelationExplanation, RelationSemanticClass, Role,
    };
    use kmp_proto::v1beta1::MemorySemanticClass;

    use super::*;

    #[test]
    fn proof_replaces_repeated_evidence_bodies_with_canonical_refs() {
        let body = "The canonical evidence body belongs in the registry once.";
        let normalized = proof(
            vec![MemoryRelation {
                source_ref: "evidence:selected".to_string(),
                target_ref: "claim:selected".to_string(),
                rel: "supports".to_string(),
                semantic_class: MemorySemanticClass::Evidential as i32,
                why: body.to_string(),
                evidence: body.to_string(),
                confidence: MemoryConfidence::High as i32,
                sequence: Some(1),
                explanation: None,
                evidence_refs: Vec::new(),
            }],
            vec![MemoryEvidence {
                id: "detail:evidence:selected".to_string(),
                supports: vec!["claim:selected".to_string()],
                text: body.to_string(),
                source: "source:selected".to_string(),
                time: None,
                metadata: Default::default(),
            }],
            Vec::new(),
            MemoryConfidence::High,
        );

        assert_eq!(normalized.evidence[0].text, body);
        assert_eq!(
            normalized.path[0].evidence_refs,
            vec!["detail:evidence:selected".to_string()]
        );
        assert_eq!(
            normalized.path[0].why,
            "Supported by canonical evidence refs."
        );
        assert!(normalized.path[0].evidence.is_empty());
    }

    #[test]
    fn temporal_evidence_only_expands_memory_evidence_support_sources() {
        let bundle = KmpBundle::new(
            CaseId::new("question:a").expect("case id should be valid"),
            Role::new("temporal-reader").expect("role should be valid"),
            node("question:a", "question"),
            vec![
                node("claim:selected", "claim"),
                node("claim:offscope", "claim"),
                node("evidence:selected", "memory_evidence"),
            ],
            vec![
                supports("claim:offscope", "claim:selected"),
                supports("evidence:selected", "claim:selected"),
            ],
            vec![
                BundleNodeDetail::new("claim:selected", "Selected detail", "hash-1", 1),
                BundleNodeDetail::new("claim:offscope", "Offscope detail", "hash-2", 1),
                BundleNodeDetail::new("evidence:selected", "Evidence detail", "hash-3", 1),
            ],
            BundleMetadata::initial("test"),
        )
        .expect("test bundle should be valid");
        let selected_refs = BTreeSet::from(["claim:selected".to_string()]);

        let evidence = temporal_evidence_from_bundle(&bundle, &selected_refs)
            .into_iter()
            .map(|evidence| evidence.id)
            .collect::<Vec<_>>();

        assert_eq!(
            evidence,
            vec![
                "detail:claim:selected".to_string(),
                "detail:evidence:selected".to_string()
            ]
        );
    }

    #[test]
    fn answer_evidence_uses_explicit_memory_evidence_and_not_anchor_detail() {
        let bundle = KmpBundle::new(
            CaseId::new("question:a").expect("case id should be valid"),
            Role::new("answerer").expect("role is valid"),
            node("question:a", "memory_anchor"),
            vec![
                node("claim:selected", "claim"),
                node("evidence:selected", "memory_evidence"),
                node("evidence:legacy", "evidence"),
            ],
            vec![
                supports("evidence:selected", "claim:selected"),
                supports("evidence:legacy", "claim:selected"),
            ],
            vec![
                BundleNodeDetail::new("question:a", "Anchor detail", "hash-root", 1),
                BundleNodeDetail::new("claim:selected", "Claim detail", "hash-claim", 1),
                BundleNodeDetail::new(
                    "evidence:selected",
                    "Explicit evidence detail",
                    "hash-evidence",
                    1,
                ),
                BundleNodeDetail::new(
                    "evidence:legacy",
                    "Legacy projected evidence detail",
                    "hash-legacy",
                    1,
                ),
            ],
            BundleMetadata::initial("test"),
        )
        .expect("test bundle should be valid");

        let evidence = answer_evidence_from_bundle(&bundle);

        assert_eq!(
            evidence
                .iter()
                .map(|evidence| evidence.text.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Explicit evidence detail",
                "Legacy projected evidence detail"
            ]
        );
        assert!(
            evidence
                .iter()
                .all(|evidence| evidence.supports == vec!["claim:selected".to_string()])
        );
    }

    #[test]
    fn answer_candidates_include_entry_text_and_mark_its_proof_role() {
        let bundle = KmpBundle::new(
            CaseId::new("question:a").expect("case id"),
            Role::new("answerer").expect("role"),
            node("question:a", "memory_anchor"),
            vec![
                node("timeline:main", "memory_dimension"),
                BundleNode::new(
                    "claim:format-b",
                    "decision",
                    "Format B",
                    "The project uses format B, not A.",
                    "ACTIVE",
                    Vec::new(),
                    BTreeMap::new(),
                ),
            ],
            vec![BundleRelationship::new(
                "timeline:main",
                "claim:format-b",
                "contains_entry",
                RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("timeline")
                    .with_scope_id("timeline:main")
                    .with_sequence(1),
            )],
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("bundle");

        let candidates = answer_evidence_from_bundle(&bundle);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "entry:claim:format-b");
        assert_eq!(candidates[0].text, "The project uses format B, not A.");
        assert_eq!(candidates[0].metadata["proof_role"], "entry_text");
    }

    #[test]
    fn answer_relations_exclude_unrelated_edges_and_keep_selected_claim_lifecycle() {
        let bundle = KmpBundle::new(
            CaseId::new("question:a").expect("case id should be valid"),
            Role::new("answerer").expect("role is valid"),
            node("question:a", "memory_anchor"),
            vec![
                node("claim:selected", "claim"),
                node("claim:conflicting", "claim"),
                node("claim:old", "claim"),
                node("claim:unrelated", "claim"),
                node("evidence:selected", "memory_evidence"),
                node("evidence:unrelated", "memory_evidence"),
            ],
            vec![
                supports("evidence:selected", "claim:selected"),
                relation("claim:selected", "claim:conflicting", "contradicts"),
                relation("claim:selected", "claim:old", "supersedes"),
                supports("evidence:unrelated", "claim:unrelated"),
            ],
            vec![
                BundleNodeDetail::new(
                    "evidence:selected",
                    "Selected answer evidence",
                    "hash-selected",
                    1,
                ),
                BundleNodeDetail::new(
                    "evidence:unrelated",
                    "Unrelated evidence",
                    "hash-unrelated",
                    1,
                ),
            ],
            BundleMetadata::initial("test"),
        )
        .expect("test bundle should be valid");
        let selected_evidence = answer_evidence_from_bundle(&bundle)
            .into_iter()
            .filter(|evidence| evidence.id == "detail:evidence:selected")
            .collect::<Vec<_>>();

        let relations = answer_relations_from_bundle(&bundle, &selected_evidence)
            .into_iter()
            .map(|relation| (relation.source_ref, relation.target_ref, relation.rel))
            .collect::<Vec<_>>();

        assert_eq!(
            relations,
            vec![
                (
                    "evidence:selected".to_string(),
                    "claim:selected".to_string(),
                    "supports".to_string(),
                ),
                (
                    "claim:selected".to_string(),
                    "claim:conflicting".to_string(),
                    "contradicts".to_string(),
                ),
                (
                    "claim:selected".to_string(),
                    "claim:old".to_string(),
                    "supersedes".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn wake_state_places_semantic_context_before_anchor_and_containment() {
        let bundle = KmpBundle::new(
            CaseId::new("question:a").expect("case id should be valid"),
            Role::new("developer").expect("role is valid"),
            node("question:a", "memory_anchor"),
            vec![
                node("claim:selected", "claim"),
                node("evidence:selected", "memory_evidence"),
            ],
            vec![
                BundleRelationship::new(
                    "question:a",
                    "claim:selected",
                    "contains_entry",
                    RelationExplanation::new(RelationSemanticClass::Structural)
                        .with_rationale("The anchor contains the recalled claim."),
                ),
                supports("evidence:selected", "claim:selected"),
            ],
            vec![BundleNodeDetail::new(
                "evidence:selected",
                "The selected evidence explains the current state.",
                "hash-selected",
                1,
            )],
            BundleMetadata::initial("test"),
        )
        .expect("test bundle should be valid");
        let rendered = render_graph_bundle(&bundle);

        let state = rendered_current_state(&rendered, &bundle);

        assert!(state[0].contains("--supports-->"), "{state:?}");
        assert!(!state[0].contains("contains_entry"), "{state:?}");
    }

    #[test]
    fn proof_surfaces_explicit_conflict_relations() {
        let conflicts = proof(
            vec![
                MemoryRelation {
                    source_ref: "claim:a".to_string(),
                    target_ref: "claim:b".to_string(),
                    rel: "contains_entry".to_string(),
                    semantic_class: MemorySemanticClass::Structural as i32,
                    why: "Structural relation is not a conflict.".to_string(),
                    evidence: String::new(),
                    confidence: MemoryConfidence::Medium as i32,
                    sequence: None,
                    explanation: None,
                    evidence_refs: Vec::new(),
                },
                MemoryRelation {
                    source_ref: "claim:a".to_string(),
                    target_ref: "claim:b".to_string(),
                    rel: "contradicts".to_string(),
                    semantic_class: MemorySemanticClass::Evidential as i32,
                    why: "Both claims cannot be true at the same time.".to_string(),
                    evidence: String::new(),
                    confidence: MemoryConfidence::High as i32,
                    sequence: None,
                    explanation: None,
                    evidence_refs: Vec::new(),
                },
                MemoryRelation {
                    source_ref: "claim:a".to_string(),
                    target_ref: "claim:b".to_string(),
                    rel: "CONTRADICTS".to_string(),
                    semantic_class: MemorySemanticClass::Evidential as i32,
                    why: "Both claims cannot be true at the same time.".to_string(),
                    evidence: String::new(),
                    confidence: MemoryConfidence::High as i32,
                    sequence: None,
                    explanation: None,
                    evidence_refs: Vec::new(),
                },
            ],
            Vec::new(),
            Vec::new(),
            MemoryConfidence::Medium,
        )
        .conflicts;

        assert_eq!(
            conflicts,
            vec![
                "claim:a contradicts claim:b: Both claims cannot be true at the same time."
                    .to_string()
            ]
        );
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

    fn supports(source_node_id: &str, target_node_id: &str) -> BundleRelationship {
        BundleRelationship::new(
            source_node_id,
            target_node_id,
            "supports",
            RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("Support relation for scoped temporal evidence.")
                .with_confidence("medium"),
        )
    }

    fn relation(
        source_node_id: &str,
        target_node_id: &str,
        relationship_type: &str,
    ) -> BundleRelationship {
        BundleRelationship::new(
            source_node_id,
            target_node_id,
            relationship_type,
            RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("Selected claim lifecycle relation.")
                .with_confidence("high"),
        )
    }
}

#[cfg(test)]
mod superseded_tests {
    use super::superseded_from_relations;
    use kmp_proto::v1beta1::MemoryRelation;

    fn relation(rel: &str, source: &str, target: &str, why: &str) -> MemoryRelation {
        MemoryRelation {
            source_ref: source.to_string(),
            target_ref: target.to_string(),
            rel: rel.to_string(),
            why: why.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_replaced_entry_comes_back_named_with_what_replaced_it() {
        let superseded = superseded_from_relations(&[relation(
            "supersedes",
            "decision:sqlite",
            "decision:redb",
            "two hosts need to share the store",
        )]);

        assert_eq!(superseded.len(), 1);
        assert_eq!(superseded[0].r#ref, "decision:redb");
        assert_eq!(superseded[0].superseded_by, "decision:sqlite");
        assert_eq!(superseded[0].why, "two hosts need to share the store");
    }

    #[test]
    fn a_contradiction_is_not_a_supersession() {
        // They mean different things and a reader has to tell them apart:
        // `contradicts` says two entries disagree and both may be live;
        // `supersedes` says one replaced the other. Folding them together
        // would make every revert read as an unresolved disagreement.
        assert!(
            superseded_from_relations(&[relation(
                "contradicts",
                "observation:latency",
                "decision:pool-size",
                "the premise was wrong",
            )])
            .is_empty()
        );
    }

    #[test]
    fn the_same_entry_replaced_twice_is_named_once() {
        let relations = [
            relation("supersedes", "decision:second", "decision:first", "a"),
            relation("supersedes", "decision:third", "decision:first", "b"),
        ];
        let superseded = superseded_from_relations(&relations);
        let mut reversed = relations;
        reversed.reverse();

        assert_eq!(superseded.len(), 1, "one line per replaced entry");
        assert_eq!(superseded[0].superseded_by, "decision:second");
        assert_eq!(superseded_from_relations(&reversed), superseded);
    }
}
