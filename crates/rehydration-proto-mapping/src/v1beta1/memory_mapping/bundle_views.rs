use std::collections::{BTreeMap, BTreeSet};

use rehydration_application::RenderedContext;
use rehydration_domain::{MemoryRelationType, RehydrationBundle, TemporalCoordinate};
use rehydration_proto::v1beta1::{
    MemoryConfidence, MemoryEvidence, MemoryRelation, Proof,
    TemporalCoordinate as ProtoTemporalCoordinate,
};

use super::scalars::{proto_confidence, proto_semantic_class, timestamp_from_sort_or_rfc3339};

pub(super) fn memory_relations_from_bundle(bundle: &RehydrationBundle) -> Vec<MemoryRelation> {
    bundle
        .relationships()
        .iter()
        .map(|relationship| {
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
            }
        })
        .collect()
}

pub(super) fn memory_evidence_from_bundle(bundle: &RehydrationBundle) -> Vec<MemoryEvidence> {
    bundle
        .node_details()
        .iter()
        .map(|detail| MemoryEvidence {
            id: format!("detail:{}", detail.node_id()),
            supports: vec![detail.node_id().to_string()],
            text: detail.detail().to_string(),
            source: detail.node_id().to_string(),
            time: None,
            metadata: Default::default(),
        })
        .collect()
}

pub(super) fn answer_evidence_from_bundle(bundle: &RehydrationBundle) -> Vec<MemoryEvidence> {
    let node_kinds = bundle_node_kinds(bundle);
    let support_targets = support_targets_by_source(bundle);

    bundle
        .node_details()
        .iter()
        .filter(|detail| {
            node_kinds
                .get(detail.node_id())
                .is_some_and(|kind| is_memory_evidence_kind(kind))
        })
        .map(|detail| MemoryEvidence {
            id: format!("detail:{}", detail.node_id()),
            supports: support_targets
                .get(detail.node_id())
                .cloned()
                .unwrap_or_else(|| vec![detail.node_id().to_string()]),
            text: detail.detail().to_string(),
            source: detail.node_id().to_string(),
            time: None,
            metadata: Default::default(),
        })
        .collect()
}

pub(super) fn temporal_relations_from_bundle(
    bundle: &RehydrationBundle,
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
    bundle: &RehydrationBundle,
    selected_refs: &BTreeSet<String>,
) -> Vec<MemoryEvidence> {
    let node_kinds = bundle_node_kinds(bundle);
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
        .map(|detail| MemoryEvidence {
            id: format!("detail:{}", detail.node_id()),
            supports: vec![detail.node_id().to_string()],
            text: detail.detail().to_string(),
            source: detail.node_id().to_string(),
            time: None,
            metadata: Default::default(),
        })
        .collect()
}

fn bundle_node_kinds(bundle: &RehydrationBundle) -> BTreeMap<&str, &str> {
    let mut node_kinds =
        BTreeMap::from([(bundle.root_node().node_id(), bundle.root_node().node_kind())]);
    for node in bundle.neighbor_nodes() {
        node_kinds.insert(node.node_id(), node.node_kind());
    }
    node_kinds
}

fn support_targets_by_source(bundle: &RehydrationBundle) -> BTreeMap<&str, Vec<String>> {
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
    let conflicts = conflicts_from_relations(&path);
    let frontier_size = missing.len() as u32;
    Proof {
        path,
        evidence,
        conflicts,
        missing,
        confidence: confidence as i32,
        frontier_size,
    }
}

fn conflicts_from_relations(path: &[MemoryRelation]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    path.iter()
        .filter(|relation| is_conflict_relation(&relation.rel))
        .filter_map(|relation| {
            let summary = conflict_summary(relation);
            seen.insert(summary.clone()).then_some(summary)
        })
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

pub(super) fn rendered_current_state(rendered: &RenderedContext) -> Vec<String> {
    let sections = rendered
        .sections
        .iter()
        .take(5)
        .map(|section| section.content.clone())
        .filter(|value| !value.trim().is_empty())
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use rehydration_domain::{
        BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId,
        RelationExplanation, RelationSemanticClass, Role,
    };
    use rehydration_proto::v1beta1::MemorySemanticClass;

    use super::*;

    #[test]
    fn temporal_evidence_only_expands_memory_evidence_support_sources() {
        let bundle = RehydrationBundle::new(
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
        let bundle = RehydrationBundle::new(
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
}
