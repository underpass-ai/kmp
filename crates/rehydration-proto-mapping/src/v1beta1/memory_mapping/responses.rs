use std::collections::{BTreeMap, BTreeSet};

use rehydration_application::{
    GetContextPathResult, GetContextResult, GraphRelationshipView, InspectMemoryResult,
    MemoryAnswerPolicy, TemporalMemoryResult, TracePageRequest,
};
use rehydration_domain::{BundleNodeDetail, RehydrationBundle, TemporalDirection};
use rehydration_proto::v1beta1::{
    AnswerReason, AskResponse, InspectResponse, InspectedLinks, InspectedObject, MemoryConfidence,
    MemoryEvidence, MemoryRelation, PageInfo, RawMemoryRef, TemporalEntry as ProtoTemporalEntry,
    TemporalMoveResponse, TemporalState, TraceResponse, WakeClaim, WakePacket, WakeResponse,
};

use super::bundle_views::{
    answer_evidence_from_bundle, memory_evidence_from_bundle, memory_relations_from_bundle, proof,
    proto_coordinate_from_domain, rendered_current_state, rendered_summary,
    temporal_evidence_from_bundle, temporal_relations_from_bundle,
};
use super::dimensions::proto_dimension_selection_from_domain;
use super::scalars::{proto_confidence, proto_direction, proto_semantic_class};

pub fn wake_response_from_result(
    intent: &str,
    max_entries: Option<usize>,
    result: GetContextResult,
) -> WakeResponse {
    let relationships = memory_relations_from_bundle(&result.bundle);
    let full_evidence = memory_evidence_from_bundle(&result.bundle);
    let current_state = rendered_current_state(&result.rendered);
    let summary = rendered_summary(&result.rendered);

    // Opt-in entry cap: surface the first `max_entries` evidence entries
    // (graph-traversal order, closest to the about) and report the withheld
    // sources as proof.missing so proof.frontier_size signals "near-expand to
    // cover the rest". Unset (or not exceeded) -> behavior unchanged.
    let (evidence, withheld) = cap_wake_evidence(full_evidence, max_entries);

    WakeResponse {
        summary,
        wake: Some(WakePacket {
            objective: intent.to_string(),
            current_state,
            causal_spine: relationships
                .iter()
                .take(8)
                .map(|relationship| WakeClaim {
                    claim: format!("{} -> {}", relationship.source_ref, relationship.target_ref),
                    because: if relationship.why.is_empty() {
                        "Kernel relationship path selected this edge.".to_string()
                    } else {
                        relationship.why.clone()
                    },
                    evidence_ref: relationship.evidence.clone(),
                })
                .collect(),
            open_loops: Vec::new(),
            next_actions: Vec::new(),
            guardrails: Vec::new(),
        }),
        proof: Some(proof(
            relationships,
            evidence,
            withheld,
            MemoryConfidence::Medium,
        )),
        warnings: Vec::new(),
    }
}

/// Opt-in entry cap for Wake: keep the first `max_entries` evidence items and
/// return the withheld sources (which become `proof.missing` → `frontier_size`,
/// signalling the client to near-expand). `None`, or a limit the evidence does
/// not exceed, leaves it unbounded — the existing behavior.
fn cap_wake_evidence(
    evidence: Vec<MemoryEvidence>,
    max_entries: Option<usize>,
) -> (Vec<MemoryEvidence>, Vec<String>) {
    match max_entries {
        Some(limit) if evidence.len() > limit => {
            let withheld = evidence[limit..]
                .iter()
                .map(|item| item.source.clone())
                .collect();
            (evidence[..limit].to_vec(), withheld)
        }
        _ => (evidence, Vec::new()),
    }
}

pub fn ask_response_from_result(
    question: &str,
    policy: MemoryAnswerPolicy,
    result: GetContextResult,
) -> AskResponse {
    let evidence = answer_evidence_from_bundle(&result.bundle);
    let because = evidence
        .iter()
        .take(5)
        .map(|item| AnswerReason {
            claim: item.source.clone(),
            evidence: item.text.clone(),
            r#ref: item.id.clone(),
        })
        .collect::<Vec<_>>();
    let confidence = if because.is_empty() {
        MemoryConfidence::Unknown
    } else {
        MemoryConfidence::Medium
    };

    let answer = match policy {
        MemoryAnswerPolicy::EvidenceOrUnknown if because.is_empty() => "UNKNOWN".to_string(),
        MemoryAnswerPolicy::EvidenceOrUnknown
        | MemoryAnswerPolicy::ShowConflicts
        | MemoryAnswerPolicy::BestEffort => deterministic_answer_from_reasons(&because),
    };
    let answer = if answer.trim().is_empty() {
        "UNKNOWN".to_string()
    } else {
        answer
    };

    AskResponse {
        summary: if answer == "UNKNOWN" {
            format!("No deterministic memory answer found for: {question}")
        } else {
            format!(
                "Deterministic memory answer from {} evidence {} for: {question}",
                because.len(),
                if because.len() == 1 { "item" } else { "items" }
            )
        },
        answer,
        because,
        proof: Some(proof(
            memory_relations_from_bundle(&result.bundle),
            evidence,
            Vec::new(),
            confidence,
        )),
        warnings: Vec::new(),
    }
}

fn deterministic_answer_from_reasons(reasons: &[AnswerReason]) -> String {
    let mut seen = BTreeSet::new();
    let evidence = reasons
        .iter()
        .filter_map(|reason| {
            let text = reason.evidence.trim();
            if text.is_empty() || !seen.insert(text.to_string()) {
                None
            } else {
                Some(text.to_string())
            }
        })
        .collect::<Vec<_>>();

    match evidence.as_slice() {
        [] => String::new(),
        [single] => single.clone(),
        many => many
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

pub fn temporal_response_from_result(
    requested_cursor: rehydration_proto::v1beta1::TemporalCursor,
    direction: TemporalDirection,
    result: TemporalMemoryResult,
) -> TemporalMoveResponse {
    let traversal = result.traversal;
    let entries = traversal
        .entries()
        .iter()
        .map(|entry| ProtoTemporalEntry {
            r#ref: entry.ref_id().to_string(),
            kind: entry.kind().to_string(),
            text: entry.text().to_string(),
            coordinates: entry
                .coordinates()
                .iter()
                .map(proto_coordinate_from_domain)
                .collect(),
        })
        .collect::<Vec<_>>();
    let selected_refs = entries
        .iter()
        .map(|entry| entry.r#ref.clone())
        .collect::<BTreeSet<_>>();
    let relationships = if result.include.relations {
        temporal_relations_from_bundle(&result.source_bundle, &selected_refs)
    } else {
        Vec::new()
    };
    let evidence = if result.include.evidence {
        temporal_evidence_from_bundle(&result.source_bundle, &selected_refs)
    } else {
        Vec::new()
    };
    let count = entries.len();
    let raw_refs = if result.include.raw_refs {
        raw_refs_from_temporal_entries(&entries, &result.source_bundle)
    } else {
        Vec::new()
    };
    let page = traversal.page();
    let mut warnings = Vec::new();
    if page.has_more() {
        warnings.push(
            "temporal response paginated; use page.next_cursor as a temporal cursor ref to continue"
                .to_string(),
        );
    }

    let dimensions = temporal_dimension_coverage(
        &entries,
        traversal.included_dimensions(),
        traversal.missing_dimensions(),
    );
    let quality = response_quality(
        count as u32,
        relationships.len() as u32,
        causal_count(&relationships),
        entries_with_detail(&entries),
        page.has_more(),
    );

    TemporalMoveResponse {
        summary: format!(
            "Returned {count} temporal {}.",
            if count == 1 { "entry" } else { "entries" }
        ),
        temporal: Some(TemporalState {
            direction: proto_direction(direction) as i32,
            requested: Some(requested_cursor),
            resolved: Some(proto_coordinate_from_domain(traversal.resolved_cursor())),
        }),
        coverage: Some(rehydration_proto::v1beta1::TemporalCoverage {
            requested: Some(proto_dimension_selection_from_domain(
                traversal.requested_dimensions(),
            )),
            included: traversal.included_dimensions().to_vec(),
            missing: traversal.missing_dimensions().to_vec(),
            dimensions,
        }),
        entries,
        proof: Some(proof(
            relationships,
            evidence,
            traversal.missing().to_vec(),
            if count == 0 {
                MemoryConfidence::Unknown
            } else {
                MemoryConfidence::Medium
            },
        )),
        warnings,
        raw_refs,
        page: Some(PageInfo {
            returned: u32_saturating(page.returned()),
            total: u32_saturating(page.total()),
            has_more: page.has_more(),
            next_cursor: page.next_cursor().unwrap_or_default().to_string(),
        }),
        quality: Some(quality),
    }
}

pub fn trace_response_from_result(
    result: GetContextPathResult,
    page: TracePageRequest,
) -> TraceResponse {
    let trace = memory_relations_from_bundle(&result.path_bundle);
    let total = trace.len();
    let offset = page.offset().min(total);
    let entries = page.entries_or_default();
    let end = offset.saturating_add(entries).min(total);
    let has_more = end < total;
    let mut warnings = Vec::new();
    if offset >= total && total > 0 {
        warnings.push(format!(
            "trace page cursor {offset} is at or beyond total trace length {total}"
        ));
    }
    if has_more {
        warnings.push("trace response paginated; use page.next_cursor to continue".to_string());
    }

    let returned_trace = trace[offset..end].to_vec();
    let quality = response_quality(
        distinct_relation_nodes(&returned_trace),
        returned_trace.len() as u32,
        causal_count(&returned_trace),
        0,
        has_more,
    );

    TraceResponse {
        summary: rendered_summary(&result.rendered),
        trace: returned_trace,
        warnings,
        page: Some(PageInfo {
            returned: u32_saturating(end.saturating_sub(offset)),
            total: u32_saturating(total),
            has_more,
            next_cursor: if has_more {
                end.to_string()
            } else {
                String::new()
            },
        }),
        quality: Some(quality),
    }
}

fn u32_saturating(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

fn ratio(numerator: u32, denominator: u32) -> f64 {
    if denominator > 0 {
        f64::from(numerator) / f64::from(denominator)
    } else {
        0.0
    }
}

fn causal_count(relationships: &[rehydration_proto::v1beta1::MemoryRelation]) -> u32 {
    relationships
        .iter()
        .filter(|relation| {
            relation.semantic_class
                == rehydration_proto::v1beta1::MemorySemanticClass::Causal as i32
        })
        .count() as u32
}

fn response_quality(
    nodes: u32,
    relationships: u32,
    causal: u32,
    details: u32,
    truncated: bool,
) -> rehydration_proto::v1beta1::ResponseQuality {
    rehydration_proto::v1beta1::ResponseQuality {
        nodes,
        relationships,
        details,
        detail_coverage: ratio(details, nodes),
        causal_density: ratio(causal, relationships),
        truncated,
    }
}

fn temporal_dimension_coverage(
    entries: &[ProtoTemporalEntry],
    included: &[String],
    missing: &[String],
) -> Vec<rehydration_proto::v1beta1::DimensionCoverage> {
    let mut coverage = Vec::with_capacity(included.len() + missing.len());
    for dimension in included {
        let returned = entries
            .iter()
            .filter(|entry| {
                entry
                    .coordinates
                    .iter()
                    .any(|coordinate| &coordinate.dimension == dimension)
            })
            .count() as u32;
        coverage.push(rehydration_proto::v1beta1::DimensionCoverage {
            dimension: dimension.clone(),
            returned,
            present: true,
        });
    }
    for dimension in missing {
        coverage.push(rehydration_proto::v1beta1::DimensionCoverage {
            dimension: dimension.clone(),
            returned: 0,
            present: false,
        });
    }
    coverage
}

fn entries_with_detail(entries: &[ProtoTemporalEntry]) -> u32 {
    entries
        .iter()
        .filter(|entry| !entry.text.trim().is_empty())
        .count() as u32
}

fn distinct_relation_nodes(relationships: &[rehydration_proto::v1beta1::MemoryRelation]) -> u32 {
    let mut refs = std::collections::BTreeSet::new();
    for relation in relationships {
        if !relation.source_ref.is_empty() {
            refs.insert(relation.source_ref.as_str());
        }
        if !relation.target_ref.is_empty() {
            refs.insert(relation.target_ref.as_str());
        }
    }
    refs.len() as u32
}

pub fn inspect_response_from_result(result: InspectMemoryResult) -> InspectResponse {
    let node_ref = result.detail.node.node_id.clone();
    let node_kind = result.detail.node.node_kind.clone();
    let text = if result.include_details {
        result
            .detail
            .detail
            .as_ref()
            .map(|detail| detail.detail.clone())
            .filter(|detail| !detail.trim().is_empty())
            .unwrap_or_else(|| result.detail.node.summary.clone())
    } else {
        result.detail.node.summary.clone()
    };
    let evidence = if result.include_details {
        result
            .detail
            .detail
            .as_ref()
            .map_or_else(Vec::new, |detail| {
                vec![MemoryEvidence {
                    id: format!("detail:{}", detail.node_id),
                    supports: vec![detail.node_id.clone()],
                    text: detail.detail.clone(),
                    source: detail.node_id.clone(),
                    time: None,
                    metadata: Default::default(),
                }]
            })
    } else {
        Vec::new()
    };
    let incoming: Vec<MemoryRelation> = result
        .incoming
        .iter()
        .map(memory_relation_from_graph_relationship)
        .collect();
    let outgoing: Vec<MemoryRelation> = result
        .outgoing
        .iter()
        .map(memory_relation_from_graph_relationship)
        .collect();
    let raw = if result.include_raw {
        vec![RawMemoryRef {
            r#ref: node_ref.clone(),
            kind: node_kind.clone(),
            text: result.detail.node.summary.clone(),
            coordinates: result
                .raw_coordinates
                .iter()
                .map(proto_coordinate_from_domain)
                .collect(),
            detail: result
                .detail
                .detail
                .as_ref()
                .map(|detail| detail.detail.clone())
                .unwrap_or_default(),
            content_hash: result
                .detail
                .detail
                .as_ref()
                .map(|detail| detail.content_hash.clone())
                .unwrap_or_default(),
            revision: result
                .detail
                .detail
                .as_ref()
                .map(|detail| detail.revision)
                .unwrap_or_default(),
        }]
    } else {
        Vec::new()
    };

    let inspect_details = u32::from(!text.trim().is_empty());
    let inspect_relationships = (incoming.len() + outgoing.len()) as u32;
    let inspect_causal = causal_count(&incoming) + causal_count(&outgoing);
    let quality = response_quality(
        1,
        inspect_relationships,
        inspect_causal,
        inspect_details,
        false,
    );

    InspectResponse {
        summary: format!("Found live kernel node `{}`.", node_ref),
        object: Some(InspectedObject {
            r#ref: node_ref,
            kind: node_kind,
            text,
        }),
        links: Some(InspectedLinks { incoming, outgoing }),
        evidence,
        warnings: Vec::new(),
        raw,
        quality: Some(quality),
    }
}

fn raw_refs_from_temporal_entries(
    entries: &[ProtoTemporalEntry],
    bundle: &RehydrationBundle,
) -> Vec<RawMemoryRef> {
    let detail_by_ref = bundle
        .node_details()
        .iter()
        .map(|detail| (detail.node_id(), detail))
        .collect::<BTreeMap<_, _>>();

    entries
        .iter()
        .map(|entry| {
            let detail = detail_by_ref.get(entry.r#ref.as_str()).copied();
            raw_ref_from_temporal_entry(entry, detail)
        })
        .collect()
}

fn raw_ref_from_temporal_entry(
    entry: &ProtoTemporalEntry,
    detail: Option<&BundleNodeDetail>,
) -> RawMemoryRef {
    RawMemoryRef {
        r#ref: entry.r#ref.clone(),
        kind: entry.kind.clone(),
        text: entry.text.clone(),
        coordinates: entry.coordinates.clone(),
        detail: detail
            .map(|detail| detail.detail().to_string())
            .unwrap_or_default(),
        content_hash: detail
            .map(|detail| detail.content_hash().to_string())
            .unwrap_or_default(),
        revision: detail.map(BundleNodeDetail::revision).unwrap_or_default(),
    }
}

fn memory_relation_from_graph_relationship(relationship: &GraphRelationshipView) -> MemoryRelation {
    let explanation = &relationship.explanation;
    MemoryRelation {
        source_ref: relationship.source_node_id.clone(),
        target_ref: relationship.target_node_id.clone(),
        rel: relationship.relationship_type.clone(),
        semantic_class: proto_semantic_class(explanation.semantic_class()) as i32,
        why: explanation.rationale().unwrap_or_default().to_string(),
        evidence: explanation.evidence().unwrap_or_default().to_string(),
        confidence: proto_confidence(explanation.confidence()) as i32,
        sequence: explanation.sequence(),
    }
}

#[cfg(test)]
mod wake_cap_tests {
    use super::*;

    fn ev(source: &str) -> MemoryEvidence {
        MemoryEvidence {
            id: format!("detail:{source}"),
            supports: vec![source.to_string()],
            text: source.to_string(),
            source: source.to_string(),
            time: None,
            metadata: Default::default(),
        }
    }

    #[test]
    fn unbounded_when_max_entries_is_none() {
        let (kept, withheld) = cap_wake_evidence(vec![ev("a"), ev("b")], None);
        assert_eq!(kept.len(), 2);
        assert!(withheld.is_empty());
    }

    #[test]
    fn unbounded_when_evidence_within_limit() {
        let (kept, withheld) = cap_wake_evidence(vec![ev("a"), ev("b")], Some(5));
        assert_eq!(kept.len(), 2);
        assert!(withheld.is_empty());
    }

    #[test]
    fn caps_and_reports_withheld_sources() {
        let (kept, withheld) = cap_wake_evidence(vec![ev("a"), ev("b"), ev("c")], Some(1));
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].source, "a");
        assert_eq!(withheld, vec!["b".to_string(), "c".to_string()]);
    }
}
