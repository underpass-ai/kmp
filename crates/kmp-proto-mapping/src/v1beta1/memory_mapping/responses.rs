use std::collections::{BTreeMap, BTreeSet};

use kmp_application::{
    GetContextPathResult, GetContextResult, GraphRelationshipView, InspectMemoryResult,
    MemoryAnswerPolicy, TemporalMemoryResult, TracePageRequest,
};
use kmp_domain::{BundleNodeDetail, KmpBundle, TemporalDirection};
use kmp_proto::v1beta1::{
    AnswerReason, AskResponse, InspectResponse, InspectedLinks, InspectedObject, MemoryConfidence,
    MemoryEvidence, MemoryRelation, MemorySemanticClass, PageInfo, RawMemoryRef, TemporalCursor,
    TemporalEntry as ProtoTemporalEntry, TemporalMoveResponse, TemporalState, TraceResponse,
    WakeClaim, WakePacket, WakeResponse,
};

use super::answer_ranker::{ANSWER_CORE_LIMIT, AnswerEvidenceRanker};

/// What the `answer` field carries when memory does not answer the question.
///
/// One token, so a caller can test for it without parsing prose, and the same
/// token whether nothing was retrieved or nothing retrieved bore on the
/// question — `summary` and `proof.missing` say which.
pub const UNANSWERED: &str = "UNKNOWN";
use super::bundle_views::{
    answer_evidence_from_bundle, answer_relations_from_bundle, bundle_memory_metadata,
    memory_evidence_from_bundle, memory_relations_from_bundle, persisted_memory_metadata,
    persisted_memory_source, proof, proto_coordinate_from_domain, proto_relation_explanation,
    rendered_current_state, rendered_summary, temporal_evidence_from_bundle,
    temporal_relations_from_bundle,
};
use super::dimensions::proto_dimension_selection_from_domain;
use super::scalars::{
    proto_confidence, proto_direction, proto_semantic_class, timestamp_from_sort_or_rfc3339,
};

/// The newest coordinate a wake packet covers, for the caller to resume from.
///
/// Wake already walked the neighbourhood to build the packet, so it holds
/// this: handing it back turns "where was I, and what changed since" from
/// three calls into two. Ordering is by time first and sequence second, which
/// is the same order the store writes in.
///
/// `None` when nothing in the packet carries a temporal coordinate — memory
/// written without one is ordinary, and inventing a cursor for it would hand
/// the caller a bookmark that points nowhere.
fn newest_cursor(relationships: &[MemoryRelation]) -> Option<TemporalCursor> {
    relationships
        .iter()
        .filter_map(|relation| {
            let coordinate = relation.explanation.as_ref()?.coordinate.as_ref()?;
            // When it happened, else when we saw it, else when we stored it —
            // the order a reader means by "since when". A coordinate carrying
            // none of the three cannot anchor a resume.
            let time = coordinate
                .occurred_at
                .or(coordinate.observed_at)
                .or(coordinate.ingested_at)?;
            Some((time, coordinate.sequence, relation))
        })
        .max_by(
            |(left_time, left_sequence, _), (right_time, right_sequence, _)| {
                (left_time.seconds, left_time.nanos, *left_sequence).cmp(&(
                    right_time.seconds,
                    right_time.nanos,
                    *right_sequence,
                ))
            },
        )
        .map(|(occurred_at, sequence, relation)| TemporalCursor {
            r#ref: relation.target_ref.clone(),
            time: Some(occurred_at),
            sequence,
        })
}

pub fn wake_response_from_result(
    intent: &str,
    max_entries: Option<usize>,
    result: GetContextResult,
) -> WakeResponse {
    let relationships = memory_relations_from_bundle(&result.bundle);
    let causal_spine = prioritize_wake_relationships(relationships.clone());
    let full_evidence = memory_evidence_from_bundle(&result.bundle);
    let current_state = rendered_current_state(&result.rendered, &result.bundle);
    let summary = rendered_summary(&result.rendered);
    // The L0 summary already selects one blocker and one next action. Leaving
    // the typed lists empty made the same packet assert `Blocker:` / `Next:`
    // in prose and deny them in structure. Project those exact selections so
    // an agent does not have to choose which half of the response to trust.
    let open_loops = l0_summary_value(&summary, "Blocker:", &["none identified"]);
    let next_actions = l0_summary_value(&summary, "Next:", &["continue"]);
    let guardrails = relationships
        .iter()
        .filter(|relationship| {
            relationship.semantic_class == MemorySemanticClass::Constraint as i32
        })
        .filter_map(|relationship| {
            let guardrail = if !relationship.why.trim().is_empty() {
                relationship.why.trim()
            } else if !relationship.evidence.trim().is_empty() {
                relationship.evidence.trim()
            } else {
                return None;
            };
            Some(guardrail.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    // Opt-in entry cap: surface the first `max_entries` evidence entries
    // (graph-traversal order, closest to the about) and report the withheld
    // sources as proof.missing so proof.frontier_size signals "near-expand to
    // cover the rest". Unset (or not exceeded) -> behavior unchanged.
    let (evidence, withheld) = cap_wake_evidence(full_evidence, max_entries);
    let resume_cursor = newest_cursor(&relationships);

    WakeResponse {
        resume_cursor,
        summary,
        wake: Some(WakePacket {
            objective: intent.to_string(),
            current_state,
            causal_spine: causal_spine
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
            open_loops,
            next_actions,
            guardrails,
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

fn l0_summary_value(summary: &str, label: &str, empty_values: &[&str]) -> Vec<String> {
    summary
        .lines()
        .find_map(|line| line.trim().strip_prefix(label).map(str::trim))
        .filter(|value| !value.is_empty() && !empty_values.contains(value))
        .map(|value| vec![value.to_string()])
        .unwrap_or_default()
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
    max_entries: Option<usize>,
    result: GetContextResult,
) -> AskResponse {
    let ranker = AnswerEvidenceRanker::from_bundle(&result.bundle);
    let candidate_evidence = answer_evidence_from_bundle(&result.bundle);
    let relevant_evidence = ranker.rank(question, policy, candidate_evidence);
    let (evidence, withheld) = cap_wake_evidence(relevant_evidence, max_entries);
    // `because` and the deterministic answer retain at most five citations.
    // Confidence must describe those surviving citations, not a stronger item
    // that `max_entries` or a later transport budget omitted.
    let retained_evidence = &evidence[..evidence.len().min(ANSWER_CORE_LIMIT)];
    let confidence = ranker.confidence(question, retained_evidence);
    let matched_terms = ranker.matched_query_terms(question, retained_evidence);
    let matched_relations = ranker.matched_relations(question, retained_evidence);
    let because = evidence
        .iter()
        .take(ANSWER_CORE_LIMIT)
        .map(|item| AnswerReason {
            claim: item
                .supports
                .first()
                .cloned()
                .unwrap_or_else(|| item.source.clone()),
            // The complete body already lives in `proof.evidence`. Keeping it
            // here made answer packets repeat the same allocation and wire
            // text; v1beta1 retains the field as an empty compatibility slot.
            evidence: String::new(),
            r#ref: item.id.clone(),
        })
        .collect::<Vec<_>>();

    // Retrieval succeeding is not the same event as the question being
    // answered, and only one of those is what the caller asked about.
    //
    // UNKNOWN used to fire on `because.is_empty()` — the selector finding
    // nothing at all. It could not fire when the selector found evidence with
    // no bearing on the question, which is the commoner case and the one that
    // matters: a question dense in words this store happens to contain came
    // back with five citations, `confidence: high`, and `missing: []`, about
    // something else entirely. That is a generated-looking answer from a
    // kernel whose whole claim is that it does not generate.
    //
    // `Low` confidence is the kernel saying the best thing it found barely
    // shares a term with the question. Under a policy that promises evidence
    // or UNKNOWN, that is UNKNOWN. `best_effort` exists for callers who want
    // the neighbourhood anyway, and keeps what it always returned.
    let bears_on_the_question = !because.is_empty()
        && !(matches!(
            policy,
            MemoryAnswerPolicy::EvidenceOrUnknown | MemoryAnswerPolicy::ShowConflicts
        ) && confidence == MemoryConfidence::Low);

    let answer = if bears_on_the_question {
        deterministic_answer_from_reasons(&because)
    } else {
        UNANSWERED.to_string()
    };
    let answer = if answer.trim().is_empty() {
        UNANSWERED.to_string()
    } else {
        answer
    };
    let unknown = !bears_on_the_question;
    let retrieved = because.len();
    // Citations belong to an answer. Returning five of them beside UNKNOWN is
    // how an unsupported answer looked supported in the first place; what was
    // retrieved is still visible in `proof.evidence`.
    let because = if unknown { Vec::new() } else { because };
    let mut answer_proof = proof(
        if unknown {
            Vec::new()
        } else {
            answer_relations_from_bundle(&result.bundle, &evidence)
        },
        evidence,
        if unknown {
            vec![if retrieved == 0 {
                format!("any evidence for: {question}")
            } else {
                format!("evidence that bears on: {question}")
            }]
        } else {
            withheld
        },
        confidence,
    );
    answer_proof.matched_terms = matched_terms;
    answer_proof.matched_relations = matched_relations;

    AskResponse {
        summary: if answer == UNANSWERED {
            // Say which of the two happened. "Found nothing" and "found
            // things that do not answer this" lead to different next moves:
            // one is a memory that has not been written yet, the other is a
            // question this memory cannot settle.
            if retrieved == 0 {
                format!("Nothing in this memory was retrieved for: {question}")
            } else {
                format!(
                    "Retrieved {retrieved} evidence {}, none of which bears on: {question}",
                    if retrieved == 1 { "item" } else { "items" }
                )
            }
        } else {
            format!(
                "Retrieved {} evidence {} for: {question}",
                because.len(),
                if because.len() == 1 { "item" } else { "items" }
            )
        },
        answer,
        because,
        proof: Some(answer_proof),
        warnings: Vec::new(),
    }
}

fn prioritize_wake_relationships(mut relationships: Vec<MemoryRelation>) -> Vec<MemoryRelation> {
    relationships.sort_by_key(|relationship| {
        match MemorySemanticClass::try_from(relationship.semantic_class) {
            Ok(MemorySemanticClass::Causal) => 0,
            Ok(MemorySemanticClass::Motivational) => 1,
            Ok(MemorySemanticClass::Evidential) => 2,
            Ok(MemorySemanticClass::Constraint) => 3,
            Ok(MemorySemanticClass::Procedural) => 4,
            Ok(MemorySemanticClass::Structural) => 5,
            _ => 6,
        }
    });
    relationships
}

/// What the kernel actually did, said in the `answer` field.
///
/// This used to open "Memory answer supported by cited evidence". The kernel
/// cannot know that: it retrieved by term overlap, and whether those items
/// answer the question is a judgement it does not make. Asserting support it
/// has not established is the one thing a kernel-not-a-model must never do,
/// and it is what made an unsupported answer read as a supported one.
///
/// So it says what it has: these items were retrieved for this question, the
/// text is in `proof.evidence`, and the reading is the caller's.
fn deterministic_answer_from_reasons(reasons: &[AnswerReason]) -> String {
    let mut seen = BTreeSet::new();
    let citations = reasons
        .iter()
        .filter_map(|reason| {
            let evidence_ref = reason.r#ref.trim();
            if evidence_ref.is_empty() || !seen.insert(evidence_ref.to_string()) {
                None
            } else {
                let claim = reason.claim.trim();
                Some(if claim.is_empty() {
                    evidence_ref.to_string()
                } else {
                    format!("{claim} [{evidence_ref}]")
                })
            }
        })
        .collect::<Vec<_>>();

    match citations.as_slice() {
        [] => String::new(),
        [single] => format!(
            "Retrieved for this question by term overlap; read proof.evidence and judge whether \
             it answers: {single}"
        ),
        many => format!(
            "Retrieved for this question by term overlap; read proof.evidence and judge whether it answers:\n{}",
            many.iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

pub fn temporal_response_from_result(
    requested_cursor: kmp_proto::v1beta1::TemporalCursor,
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
            metadata: bundle_memory_metadata(&result.source_bundle, entry.ref_id()),
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
        coverage: Some(kmp_proto::v1beta1::TemporalCoverage {
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

fn causal_count(relationships: &[kmp_proto::v1beta1::MemoryRelation]) -> u32 {
    relationships
        .iter()
        .filter(|relation| {
            matches!(
                kmp_proto::v1beta1::MemorySemanticClass::try_from(relation.semantic_class),
                Ok(kmp_proto::v1beta1::MemorySemanticClass::Causal
                    | kmp_proto::v1beta1::MemorySemanticClass::Motivational
                    | kmp_proto::v1beta1::MemorySemanticClass::Evidential)
            )
        })
        .count() as u32
}

fn response_quality(
    nodes: u32,
    relationships: u32,
    causal: u32,
    details: u32,
    truncated: bool,
) -> kmp_proto::v1beta1::ResponseQuality {
    kmp_proto::v1beta1::ResponseQuality {
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
) -> Vec<kmp_proto::v1beta1::DimensionCoverage> {
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
        coverage.push(kmp_proto::v1beta1::DimensionCoverage {
            dimension: dimension.clone(),
            returned,
            present: true,
        });
    }
    for dimension in missing {
        coverage.push(kmp_proto::v1beta1::DimensionCoverage {
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

fn distinct_relation_nodes(relationships: &[kmp_proto::v1beta1::MemoryRelation]) -> u32 {
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
    let metadata = persisted_memory_metadata(&result.detail.node.properties);
    let source = persisted_memory_source(&result.detail.node.properties)
        .unwrap_or_default()
        .to_string();
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
                    source: if source.is_empty() {
                        detail.node_id.clone()
                    } else {
                        source.clone()
                    },
                    time: timestamp_from_sort_or_rfc3339(
                        result
                            .detail
                            .node
                            .properties
                            .get("payload_time")
                            .map(String::as_str),
                    ),
                    metadata: metadata.clone(),
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
            metadata,
            source,
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
    bundle: &KmpBundle,
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
        explanation: proto_relation_explanation(explanation),
        evidence_refs: Vec::new(),
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

    #[test]
    fn wake_prioritizes_semantic_relations_before_structural_bookkeeping() {
        let relation = |semantic_class, rel: &str| MemoryRelation {
            rel: rel.to_string(),
            semantic_class: semantic_class as i32,
            ..Default::default()
        };
        let prioritized = prioritize_wake_relationships(vec![
            relation(MemorySemanticClass::Structural, "contains_entry"),
            relation(MemorySemanticClass::Procedural, "follows"),
            relation(MemorySemanticClass::Evidential, "supports"),
            relation(MemorySemanticClass::Causal, "triggers"),
        ]);

        assert_eq!(
            prioritized
                .iter()
                .map(|relationship| relationship.rel.as_str())
                .collect::<Vec<_>>(),
            vec!["triggers", "supports", "follows", "contains_entry"]
        );
    }

    #[test]
    fn causal_count_matches_domain_explanatory_relation_classes() {
        let relation = |semantic_class| MemoryRelation {
            semantic_class: semantic_class as i32,
            ..Default::default()
        };
        let relations = vec![
            relation(kmp_proto::v1beta1::MemorySemanticClass::Structural),
            relation(kmp_proto::v1beta1::MemorySemanticClass::Causal),
            relation(kmp_proto::v1beta1::MemorySemanticClass::Motivational),
            relation(kmp_proto::v1beta1::MemorySemanticClass::Evidential),
            relation(kmp_proto::v1beta1::MemorySemanticClass::Procedural),
        ];

        assert_eq!(causal_count(&relations), 3);
    }

    #[test]
    fn wake_structured_lists_agree_with_the_l0_summary() {
        assert_eq!(
            l0_summary_value(
                "Objective: ship\nStatus: active\nBlocker: waiting for CI\nNext: merge → main",
                "Blocker:",
                &["none identified"],
            ),
            vec!["waiting for CI"]
        );
        assert_eq!(
            l0_summary_value(
                "Objective: ship\nStatus: active\nBlocker: waiting for CI\nNext: merge → main",
                "Next:",
                &["continue"],
            ),
            vec!["merge → main"]
        );
        assert!(
            l0_summary_value(
                "Objective: ship\nStatus: active\nBlocker: none identified\nNext: continue",
                "Blocker:",
                &["none identified"],
            )
            .is_empty()
        );
    }

    #[test]
    fn one_citation_never_turns_retrieval_into_a_support_claim() {
        let answer = deterministic_answer_from_reasons(&[AnswerReason {
            claim: "claim:one".to_string(),
            evidence: String::new(),
            r#ref: "detail:evidence:one".to_string(),
        }]);

        assert!(answer.starts_with("Retrieved for this question by term overlap"));
        assert!(answer.contains("judge whether it answers"));
        assert!(!answer.contains("supported by"));
    }
}

#[cfg(test)]
mod tests {
    use super::newest_cursor;
    use kmp_proto::v1beta1::{MemoryRelation, MemoryRelationExplanation, TemporalCoordinate};
    use prost_types::Timestamp;

    fn relation(
        target: &str,
        seconds: i64,
        sequence: Option<u32>,
        observed: bool,
    ) -> MemoryRelation {
        let time = Some(Timestamp { seconds, nanos: 0 });
        let coordinate = TemporalCoordinate {
            dimension: "work".to_string(),
            scope_id: "scope".to_string(),
            occurred_at: if observed { None } else { time },
            observed_at: if observed { time } else { None },
            sequence,
            ..Default::default()
        };
        MemoryRelation {
            source_ref: "entry".to_string(),
            target_ref: target.to_string(),
            sequence,
            explanation: Some(MemoryRelationExplanation {
                coordinate: Some(coordinate),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn the_cursor_is_the_newest_coordinate_the_packet_covers() {
        let cursor = newest_cursor(&[
            relation("older", 1_000, Some(1), false),
            relation("newest", 3_000, Some(2), false),
            relation("middle", 2_000, Some(9), false),
        ])
        .expect("a packet with coordinates has a cursor");

        assert_eq!(cursor.r#ref, "newest");
        assert_eq!(cursor.time.expect("time").seconds, 3_000);
        assert_eq!(cursor.sequence, Some(2));
    }

    #[test]
    fn sequence_breaks_a_tie_on_time() {
        let cursor = newest_cursor(&[
            relation("first", 5_000, Some(1), false),
            relation("second", 5_000, Some(7), false),
        ])
        .expect("a cursor");

        assert_eq!(cursor.r#ref, "second", "the later sequence wins the tie");
    }

    #[test]
    fn a_coordinate_without_occurrence_still_anchors_a_resume() {
        // Relations written with only `observed_at` are ordinary — the real
        // store is full of them — and used to yield no cursor at all.
        let cursor = newest_cursor(&[relation("seen", 4_000, None, true)]).expect("a cursor");
        assert_eq!(cursor.r#ref, "seen");
        assert_eq!(cursor.time.expect("time").seconds, 4_000);
    }

    #[test]
    fn memory_with_no_coordinates_gets_no_cursor() {
        let bare = MemoryRelation {
            source_ref: "entry".to_string(),
            target_ref: "target".to_string(),
            ..Default::default()
        };
        assert!(
            newest_cursor(&[bare]).is_none(),
            "a bookmark that points nowhere is worse than none"
        );
    }
}
