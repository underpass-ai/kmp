use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use kmp_application::{
    GetContextPathResult, GetContextResult, GraphRelationshipView, InspectMemoryResult,
    MemoryAnswerPolicy, TemporalMemoryResult, TracePageRequest,
};
use kmp_domain::{
    BundleNodeDetail, KmpBundle, QuestionRendering, QuestionRenderingFault, TemporalAxis,
    TemporalCoordinate, TemporalDirection, TemporalSelection, compare_temporal_instants,
};
use kmp_proto::v1beta1::{
    AnswerReason, AskResponse, ExpiredMemory, InspectResponse, InspectedLinks, InspectedObject,
    MemoryConfidence, MemoryEvidence, MemoryRelation, MemorySemanticClass, PageInfo, RawMemoryRef,
    TemporalCursor, TemporalEntry as ProtoTemporalEntry, TemporalMoveResponse, TemporalState,
    TraceResponse, WakeClaim, WakePacket, WakeResponse,
};

use super::answer_ranker::{ANSWER_CORE_LIMIT, AnswerEvidenceRanker};
use super::answer_selection::was_reached_indirectly;
use super::lexical_bridge::LexicalBridge;
use super::scalars::ProtoMappingResult;
use super::temporal_admission::TemporalAdmission;

/// What the `answer` field carries when memory does not answer the question.
///
/// One token, so a caller can test for it without parsing prose, and the same
/// token whether nothing was retrieved or nothing retrieved bore on the
/// question — `summary` and `proof.missing` say which.
pub const UNANSWERED: &str = "UNKNOWN";
use super::bundle_views::{
    answer_evidence_from_bundle, answer_relations_from_bundle, bundle_memory_metadata,
    memory_evidence_from_bundle, memory_relation_from_bundle_relationship,
    memory_relations_from_bundle, persisted_memory_metadata, persisted_memory_source, proof,
    proto_coordinate_from_domain, proto_relation_explanation, rendered_current_state,
    rendered_summary, superseded_from_relations, temporal_evidence_from_bundle,
    temporal_relations_from_bundle,
};
use super::dimensions::proto_dimension_selection_from_domain;
use super::memory_lifecycle::MemoryLifecycle;
use super::relation_signal_index::RelationSignalIndex;
use super::scalars::{
    proto_confidence, proto_direction, proto_semantic_class, proto_temporal_axis,
    timestamp_from_sort_or_rfc3339,
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
            |(left_time, left_sequence, left), (right_time, right_sequence, right)| {
                (
                    left_time.seconds,
                    left_time.nanos,
                    *left_sequence,
                    &left.target_ref,
                    &left.source_ref,
                )
                    .cmp(&(
                        right_time.seconds,
                        right_time.nanos,
                        *right_sequence,
                        &right.target_ref,
                        &right.source_ref,
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
    temporal: &TemporalSelection,
) -> ProtoMappingResult<WakeResponse> {
    // A packet bounded in time stands on the selection: its evidence, its
    // spine, its cursor and its proof are the selection's. The rendered
    // prose is the about's, because it is rendered before this reads it.
    let admission = TemporalAdmission::read(&result.bundle, temporal)?;
    let bounded = admission.bound(&result.bundle);
    let lifecycle = lifecycle_for(&bounded, &admission);
    let signals = RelationSignalIndex::read(&bounded);
    let relationships = memory_relations_from_bundle(&bounded);
    let causal_spine = prioritize_wake_relationships(relationships.clone(), &signals);
    let full_evidence = memory_evidence_from_bundle(&result.bundle)
        .into_iter()
        .filter(|item| admission.admits(item))
        .collect::<Vec<_>>();
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

    // Opt-in entry cap: surface `max_entries` evidence entries and report the
    // withheld sources as proof.missing so proof.frontier_size signals
    // "near-expand to cover the rest". Unset (or not exceeded) -> every entry.
    //
    // Which ones survive the cap used to be graph-traversal order alone. Wake
    // has no question to rank against, but it does have the judgment already
    // stored on every edge, and a resume packet whose first ten entries were
    // whatever the traversal emitted first is a worse answer than one whose
    // first ten are what someone proved and has not withdrawn.
    let full_evidence = prioritize_wake_evidence(full_evidence, &lifecycle, &signals);
    let (evidence, withheld) = cap_wake_evidence(full_evidence, max_entries);
    let resume_cursor = newest_cursor(&relationships);

    Ok(WakeResponse {
        projection: None,
        truncation: None,
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
        proof: {
            let mut wake_proof = proof(relationships, evidence, withheld, MemoryConfidence::Medium);
            // The field has been in the contract since the proof shape
            // existed and nothing ever filled it, so a memory whose
            // applicability ended arrived as current state next to an empty
            // list asserting that none had.
            wake_proof.expired = lifecycle.expired_memories();
            let stood = admission.proof_fields();
            wake_proof.interval = stood.interval;
            wake_proof.axis = stood.axis;
            wake_proof.as_of = stood.as_of;
            Some(wake_proof)
        },
        warnings: Vec::new(),
    })
}

/// The lifecycles as they stand where the recall stands: at the instant the
/// caller named, or at the memory's own frontier.
fn lifecycle_for(bundle: &KmpBundle, admission: &TemporalAdmission) -> MemoryLifecycle {
    match admission.lifecycle_instant() {
        Some(instant) => MemoryLifecycle::read_at(bundle, instant, admission.axis()),
        None => MemoryLifecycle::read(bundle),
    }
}

/// Orders a wake packet before the entry cap decides what survives it.
///
/// Lifecycle leads: what was replaced or has stopped applying is still
/// returned — wake reports state, it does not censor it — but it goes to the
/// back, where a cap can cut it before it cuts a live decision. Then the
/// strongest relation the writer attached, then recency. Ties keep the
/// traversal order they arrived in, so nothing moves without a reason.
fn prioritize_wake_evidence(
    evidence: Vec<MemoryEvidence>,
    lifecycle: &MemoryLifecycle,
    signals: &RelationSignalIndex,
) -> Vec<MemoryEvidence> {
    let standing = |item: &MemoryEvidence| {
        let refs = memory_refs_of(item);
        if refs
            .iter()
            .any(|item_ref| lifecycle.is_superseded(item_ref))
        {
            0
        } else if refs.iter().any(|item_ref| lifecycle.is_expired(item_ref)) {
            1
        } else {
            2
        }
    };

    let mut evidence = evidence;
    evidence.sort_by(|left, right| {
        standing(right)
            .cmp(&standing(left))
            .then_with(|| {
                signals
                    .strength_over(memory_refs_of(right).iter().map(String::as_str))
                    .cmp(&signals.strength_over(memory_refs_of(left).iter().map(String::as_str)))
            })
            .then_with(|| {
                lifecycle
                    .recency_rank(right.time.as_ref())
                    .cmp(&lifecycle.recency_rank(left.time.as_ref()))
            })
    });
    evidence
}

/// The memory references a response item stands for: the detail it renders
/// and the claims it supports.
fn memory_refs_of(item: &MemoryEvidence) -> Vec<String> {
    item.id
        .strip_prefix("detail:")
        .or_else(|| item.id.strip_prefix("entry:"))
        .map(str::to_string)
        .into_iter()
        .chain(item.supports.iter().cloned())
        .collect()
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
    asked_as: Option<&str>,
    policy: MemoryAnswerPolicy,
    max_entries: Option<usize>,
    result: GetContextResult,
    bridge: &LexicalBridge,
    temporal: &TemporalSelection,
) -> ProtoMappingResult<AskResponse> {
    let admission = TemporalAdmission::read(&result.bundle, temporal)?;
    let bounded = admission.bound(&result.bundle);
    let lifecycle = lifecycle_for(&bounded, &admission);
    let ranker = AnswerEvidenceRanker::from_bundle_at(&bounded, bridge, lifecycle);
    // What the selection admits is decided before the ranker weighs a word,
    // so the collection its statistics read is the selection's own: a word
    // common in the about and rare in the span earns what it earns there.
    let (candidate_evidence, outside_evidence): (Vec<_>, Vec<_>) =
        answer_evidence_from_bundle(&result.bundle)
            .into_iter()
            .partition(|item| admission.admits(item));
    let relevant_evidence = ranker.rank(question, policy, candidate_evidence);
    let (evidence, withheld) = cap_wake_evidence(relevant_evidence, max_entries);
    // A candidate the graph reached is proof, not an answer. It travels in
    // `proof.evidence` with the hop that produced it, and the answer core is
    // still built only from evidence the question matched in its own words —
    // the ranker's standing rule, now enforced where the answer is written
    // rather than by refusing to retrieve the hop at all.
    let answer_core = evidence
        .iter()
        .filter(|item| !was_reached_indirectly(item))
        .cloned()
        .collect::<Vec<_>>();
    // `because` and the deterministic answer retain at most five citations.
    // Confidence must describe those surviving citations, not a stronger item
    // that `max_entries` or a later transport budget omitted.
    let retained_evidence = &answer_core[..answer_core.len().min(ANSWER_CORE_LIMIT)];
    let confidence = ranker.confidence(question, retained_evidence);
    let matched_terms = ranker.matched_query_terms(question, retained_evidence);
    let matched_relations = ranker.matched_relations(question, retained_evidence);
    let because = answer_core
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
            answer_relations_from_bundle(&bounded, &evidence)
        },
        evidence,
        if unknown {
            vec![if retrieved == 0 {
                format!("any stored memory for: {question}")
            } else {
                format!("stored memory that bears on: {question}")
            }]
        } else {
            withheld
        },
        confidence,
    );
    answer_proof.matched_terms = matched_terms;
    answer_proof.matched_relations = matched_relations;
    // Ask withholds an entry whose applicability ended rather than offering it
    // as current. Saying so is what separates that from having nothing.
    answer_proof.expired = ranker.expired_memories();
    let stood = admission.proof_fields();
    answer_proof.interval = stood.interval;
    answer_proof.axis = stood.axis;
    answer_proof.as_of = stood.as_of;
    // UNKNOWN within a span is one of two things: not known, or not then.
    // When what lies outside the span does bear on the question, the proof
    // names the closest of it, so the caller can widen the span on purpose
    // instead of concluding the memory was never written.
    if unknown && admission.bounds_a_span() && !outside_evidence.is_empty() {
        let outside_core = ranker
            .rank(question, policy, outside_evidence)
            .into_iter()
            .filter(|item| !was_reached_indirectly(item))
            .take(ANSWER_CORE_LIMIT)
            .collect::<Vec<_>>();
        let outside_bears = !outside_core.is_empty()
            && !(matches!(
                policy,
                MemoryAnswerPolicy::EvidenceOrUnknown | MemoryAnswerPolicy::ShowConflicts
            ) && ranker.confidence(question, &outside_core) == MemoryConfidence::Low);
        if outside_bears {
            answer_proof.nearest_outside = admission.nearest_outside(&outside_core);
        }
    }
    let nearest_outside_note = answer_proof
        .nearest_outside
        .as_ref()
        .map(|nearest| {
            format!(
                "; the nearest match outside the interval is {} at {}",
                nearest.r#ref,
                nearest
                    .time
                    .map(|time| time.to_string())
                    .unwrap_or_else(|| "an unread instant".to_string())
            )
        })
        .unwrap_or_default();

    Ok(AskResponse {
        projection: None,
        truncation: None,
        summary: if answer == UNANSWERED {
            // Say which of the two happened. "Found nothing" and "found
            // things that do not answer this" lead to different next moves:
            // one is a memory that has not been written yet, the other is a
            // question this memory cannot settle.
            if retrieved == 0 {
                format!(
                    "Nothing in this memory was retrieved for: {question}{nearest_outside_note}"
                )
            } else {
                format!(
                    "Retrieved {retrieved} memory {}, none of which bears on: {question}{nearest_outside_note}",
                    if retrieved == 1 { "item" } else { "items" }
                )
            }
        } else {
            format!(
                "Retrieved {} memory {} for: {question}",
                because.len(),
                if because.len() == 1 { "item" } else { "items" }
            )
        },
        answer,
        because,
        proof: Some(answer_proof),
        warnings: question_rendering_warnings(question, asked_as),
        asked_as: asked_as.unwrap_or_default().to_string(),
    })
}

/// What a rendered question lost of the user's words, said on the answer.
///
/// The kernel searched the question as given whatever this says; the warning
/// is for the agent that rendered it, so the next ask keeps the identifier or
/// the language the user's words carried.
fn question_rendering_warnings(question: &str, asked_as: Option<&str>) -> Vec<String> {
    let Some(asked_as) = asked_as else {
        return Vec::new();
    };
    match QuestionRendering::lint(asked_as, question) {
        Ok(_) => Vec::new(),
        Err(faults) => vec![format!(
            "question is a rendering of asked_as that {}; the kernel searched it as given",
            QuestionRenderingFault::describe(&faults)
        )],
    }
}

/// Orders the causal spine, which is then cut to eight.
///
/// The declared salience of the class leads, as it always did. What is new is
/// the second key: among two causal edges, the one the writer proved goes
/// first, instead of whichever endpoint happened to sort earlier in the
/// alphabet.
fn prioritize_wake_relationships(
    mut relationships: Vec<MemoryRelation>,
    signals: &RelationSignalIndex,
) -> Vec<MemoryRelation> {
    relationships.sort_by(|left, right| {
        let priority = |relationship: &MemoryRelation| match MemorySemanticClass::try_from(
            relationship.semantic_class,
        ) {
            Ok(MemorySemanticClass::Causal) => 0,
            Ok(MemorySemanticClass::Motivational) => 1,
            Ok(MemorySemanticClass::Evidential) => 2,
            Ok(MemorySemanticClass::Constraint) => 3,
            Ok(MemorySemanticClass::Procedural) => 4,
            Ok(MemorySemanticClass::Structural) => 5,
            _ => 6,
        };
        let strength = |relationship: &MemoryRelation| {
            signals.strength_of_edge(
                &relationship.source_ref,
                &relationship.target_ref,
                &relationship.rel,
            )
        };
        priority(left)
            .cmp(&priority(right))
            .then_with(|| strength(right).cmp(&strength(left)))
            .then_with(|| {
                (
                    &left.source_ref,
                    &left.target_ref,
                    &left.rel,
                    &left.why,
                    &left.evidence,
                    left.sequence,
                )
                    .cmp(&(
                        &right.source_ref,
                        &right.target_ref,
                        &right.rel,
                        &right.why,
                        &right.evidence,
                        right.sequence,
                    ))
            })
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
    let expired = expired_at_cursor(
        traversal.entries(),
        &result.source_bundle,
        traversal.axis(),
        traversal.resolved_cursor(),
    );
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
    // Lifecycle is part of temporal truth even when the caller does not ask
    // for the full relation path. Keep the selected entries' supersession
    // edges long enough to populate proof.superseded, then honor `include`
    // for the visible path itself.
    let selected_relationships =
        temporal_relations_from_bundle(&result.source_bundle, &selected_refs);
    let relationships = if result.include.relations {
        selected_relationships.clone()
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
    if !requested_cursor.r#ref.trim().is_empty()
        && traversal
            .missing()
            .iter()
            .any(|item| item == "temporal_positions")
        && let Some(clock) = missing_explicit_clock(traversal.axis(), traversal.resolved_cursor())
    {
        warnings.push(format!(
            "temporal cursor ref `{}` exists but carries no `{clock}` clock",
            requested_cursor.r#ref
        ));
    }
    let absent_requested_clock = if count == 0
        && requested_cursor.r#ref.trim().is_empty()
        && traversal
            .missing()
            .iter()
            .any(|item| item == "temporal_positions")
    {
        explicit_clock_name(traversal.axis())
    } else {
        None
    };
    if let Some(clock) = absent_requested_clock {
        warnings.push(format!(
            "no temporal entries carry the requested `{clock}` clock"
        ));
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

    let mut temporal_proof = proof(
        relationships,
        evidence,
        traversal.missing().to_vec(),
        if count == 0 {
            MemoryConfidence::Unknown
        } else {
            MemoryConfidence::Medium
        },
    );
    temporal_proof.superseded = superseded_from_relations(&selected_relationships);
    temporal_proof.expired = expired;

    TemporalMoveResponse {
        summary: match absent_requested_clock {
            Some(clock) => format!(
                "Returned 0 temporal entries; no entries carry the requested {clock} clock."
            ),
            None => format!(
                "Returned {count} temporal {}.",
                if count == 1 { "entry" } else { "entries" }
            ),
        },
        temporal: Some(TemporalState {
            direction: proto_direction(direction) as i32,
            axis: proto_temporal_axis(traversal.axis()) as i32,
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
        proof: Some(temporal_proof),
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

/// What had already stopped applying when this read was positioned.
///
/// Expiry is history, so a temporal move reports it and never hides it.
///
/// On the occurred, observed and ingested clocks a move returns recorded
/// positions, so a constraint that stopped applying comes back as it was
/// written — and came back with nothing beside it saying so. Those entries
/// are named here and still returned: a history reader that silently dropped
/// them would be answering a different question.
///
/// On the validity clock the read is about what held when, so it also names
/// what an as-of view left out for having ended. That filter runs inside the
/// traversal, before this ever sees the entries, which is why the one read
/// where expiry is the whole subject used to report none of it.
fn expired_at_cursor(
    entries: &[kmp_domain::TemporalEntry],
    bundle: &KmpBundle,
    axis: TemporalAxis,
    cursor: &TemporalCoordinate,
) -> Vec<ExpiredMemory> {
    let Some(instant) = cursor_instant(cursor, axis) else {
        return Vec::new();
    };

    let mut validity_by_ref = BTreeMap::<&str, Vec<(Option<&str>, Option<&str>)>>::new();
    for entry in entries {
        for coordinate in entry.coordinates() {
            if coordinate.valid_from().is_some() || coordinate.valid_until().is_some() {
                validity_by_ref
                    .entry(entry.ref_id())
                    .or_default()
                    .push((coordinate.valid_from(), coordinate.valid_until()));
            }
        }
    }
    if axis == TemporalAxis::Validity {
        for relationship in bundle
            .relationships()
            .iter()
            .filter(|relationship| relationship.relationship_type() == "contains_entry")
        {
            let explanation = relationship.explanation();
            if explanation.valid_from().is_some() || explanation.valid_until().is_some() {
                validity_by_ref
                    .entry(relationship.target_node_id())
                    .or_default()
                    .push((explanation.valid_from(), explanation.valid_until()));
            }
        }
    }

    validity_by_ref
        .into_iter()
        .filter_map(|(ref_id, validity)| {
            // An entry standing on any one of its coordinates is not expired,
            // however many others have run out.
            let active = validity.iter().any(|(start, end)| {
                start.is_none_or(|start| {
                    compare_temporal_instants(start, instant) != Some(Ordering::Greater)
                }) && end.is_none_or(|end| {
                    compare_temporal_instants(end, instant) == Some(Ordering::Greater)
                })
            });
            if active {
                return None;
            }
            let ended = validity
                .iter()
                .filter_map(|(_, end)| *end)
                .filter(|end| {
                    matches!(
                        compare_temporal_instants(end, instant),
                        Some(Ordering::Less | Ordering::Equal)
                    )
                })
                .max_by(|left, right| {
                    compare_temporal_instants(left, right).unwrap_or(Ordering::Equal)
                })?;
            Some(ExpiredMemory {
                r#ref: ref_id.to_string(),
                valid_until: timestamp_from_sort_or_rfc3339(Some(ended)),
            })
        })
        .collect()
}

/// The instant a read is standing at, on whatever clock it asked for.
fn cursor_instant(cursor: &TemporalCoordinate, axis: TemporalAxis) -> Option<&str> {
    match axis {
        TemporalAxis::Validity => cursor.valid_from().or(cursor.valid_until()),
        TemporalAxis::Occurred => cursor.occurred_at(),
        TemporalAxis::Observed => cursor.observed_at(),
        TemporalAxis::Ingested => cursor.ingested_at(),
        // The default clock resolves in the order the store writes in.
        TemporalAxis::Default => cursor
            .occurred_at()
            .or(cursor.observed_at())
            .or(cursor.ingested_at())
            .or(cursor.valid_from()),
    }
}

fn missing_explicit_clock(
    axis: TemporalAxis,
    coordinate: &kmp_domain::TemporalCoordinate,
) -> Option<&'static str> {
    match axis {
        TemporalAxis::Default => None,
        TemporalAxis::Occurred if coordinate.occurred_at().is_none() => Some("occurred"),
        TemporalAxis::Observed if coordinate.observed_at().is_none() => Some("observed"),
        TemporalAxis::Ingested if coordinate.ingested_at().is_none() => Some("ingested"),
        TemporalAxis::Validity
            if coordinate.valid_from().is_none() && coordinate.valid_until().is_none() =>
        {
            Some("validity")
        }
        _ => None,
    }
}

fn explicit_clock_name(axis: TemporalAxis) -> Option<&'static str> {
    match axis {
        TemporalAxis::Default => None,
        TemporalAxis::Occurred => Some("occurred"),
        TemporalAxis::Observed => Some("observed"),
        TemporalAxis::Ingested => Some("ingested"),
        TemporalAxis::Validity => Some("validity"),
    }
}

pub fn trace_response_from_result(
    result: GetContextPathResult,
    page: TracePageRequest,
) -> TraceResponse {
    let path = result.path_relationships();
    let trace = path
        .as_ref()
        .map(|relationships| {
            relationships
                .iter()
                .map(|relationship| memory_relation_from_bundle_relationship(relationship))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let total = trace.len();
    let offset = page.offset().min(total);
    let entries = page.entries_or_default();
    let end = offset.saturating_add(entries).min(total);
    let has_more = end < total;
    let mut warnings = Vec::new();
    if path.is_none() {
        warnings.push(format!(
            "no directed trace reaches `{}` from `{}`; KMP does not present the explored neighborhood as proof",
            result.target_node_id(),
            result.root_node_id()
        ));
    }
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
                Ok(kmp_proto::v1beta1::MemorySemanticClass::Causal)
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
    let evidence = result
        .evidence
        .iter()
        .map(|evidence| {
            let properties = &evidence.detail.node.properties;
            let mut metadata = persisted_memory_metadata(properties);
            metadata
                .entry("proof_role".to_string())
                .or_insert_with(|| "stored_evidence".to_string());
            MemoryEvidence {
                id: evidence.detail.node.node_id.clone(),
                supports: evidence.supports.clone(),
                text: evidence
                    .detail
                    .detail
                    .as_ref()
                    .map(|detail| detail.detail.clone())
                    .filter(|detail| !detail.trim().is_empty())
                    .unwrap_or_else(|| evidence.detail.node.summary.clone()),
                source: persisted_memory_source(properties)
                    .unwrap_or(&evidence.detail.node.node_id)
                    .to_string(),
                time: timestamp_from_sort_or_rfc3339(
                    properties.get("payload_time").map(String::as_str),
                ),
                metadata,
            }
        })
        .collect();
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
mod temporal_lifecycle_tests {
    use std::collections::BTreeMap;

    use kmp_application::{TemporalIncludeOptions, TemporalMemoryResult};
    use kmp_domain::{
        BundleMetadata, BundleNode, BundleQualityMetrics, BundleRelationship, CaseId, KmpBundle,
        RelationExplanation, RelationSemanticClass, Role, TemporalAxis,
        TemporalCursor as DomainCursor, TemporalDirection, TemporalMemoryTraversal,
        TemporalTraversalRequest,
    };
    use kmp_proto::v1beta1::TemporalCursor;

    use super::temporal_response_from_result;

    fn lifecycle_node(id: &str, kind: &str) -> BundleNode {
        BundleNode::new(id, kind, id, id, "ACTIVE", Vec::new(), BTreeMap::new())
    }

    /// A freeze that ran for nine days, and a decision taken ten days after it
    /// lapsed.
    fn lapsed_freeze_bundle() -> KmpBundle {
        let entry =
            |target: &str, occurred_at: &str, valid_from: &str, valid_until: Option<&str>| {
                let mut explanation = RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("timeline")
                    .with_scope_id("timeline:main")
                    .with_occurred_at(occurred_at)
                    .with_valid_from(valid_from);
                if let Some(valid_until) = valid_until {
                    explanation = explanation.with_valid_until(valid_until);
                }
                BundleRelationship::new("timeline:main", target, "contains_entry", explanation)
            };
        KmpBundle::new(
            CaseId::new("project:release").expect("case id"),
            Role::new("temporal-reader").expect("role"),
            lifecycle_node("project:release", "memory_anchor"),
            vec![
                lifecycle_node("timeline:main", "memory_dimension"),
                lifecycle_node("constraint:freeze", "constraint"),
                lifecycle_node("decision:cutover", "decision"),
            ],
            vec![
                entry(
                    "constraint:freeze",
                    "2026-08-01T09:00:00Z",
                    "2026-08-01T09:00:00Z",
                    Some("2026-08-10T09:00:00Z"),
                ),
                entry(
                    "decision:cutover",
                    "2026-08-20T09:00:00Z",
                    "2026-08-20T09:00:00Z",
                    None,
                ),
            ],
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("bundle")
    }

    fn move_over(
        bundle: KmpBundle,
        direction: TemporalDirection,
        at: &str,
        axis: TemporalAxis,
    ) -> kmp_proto::v1beta1::TemporalMoveResponse {
        let traversal = TemporalMemoryTraversal::traverse(
            &bundle,
            &TemporalTraversalRequest::new(direction, DomainCursor::time(at).expect("cursor"))
                .with_axis(axis),
        )
        .expect("traversal");
        temporal_response_from_result(
            TemporalCursor {
                r#ref: String::new(),
                time: None,
                sequence: None,
            },
            direction,
            TemporalMemoryResult {
                traversal,
                source_bundle: bundle,
                include: TemporalIncludeOptions {
                    evidence: false,
                    relations: false,
                    raw_refs: false,
                },
                quality: BundleQualityMetrics::new(0, 1.0, 0.0, 0.0, 0.0).expect("quality"),
            },
        )
    }

    /// A move on a recorded clock keeps returning history — dropping the
    /// lapsed constraint would answer a different question — but it now says
    /// which of what it returned had already stopped applying.
    #[test]
    fn a_recorded_clock_returns_the_lapsed_entry_and_names_it() {
        for direction in [
            TemporalDirection::Goto,
            TemporalDirection::Near,
            TemporalDirection::Rewind,
        ] {
            let response = move_over(
                lapsed_freeze_bundle(),
                direction,
                "2026-08-25T09:00:00Z",
                TemporalAxis::Occurred,
            );

            let refs = response
                .entries
                .iter()
                .map(|entry| entry.r#ref.as_str())
                .collect::<Vec<_>>();
            assert!(
                refs.contains(&"constraint:freeze"),
                "{direction:?} stopped returning history"
            );
            let proof = response.proof.expect("proof");
            assert_eq!(proof.expired.len(), 1, "{direction:?}");
            assert_eq!(proof.expired[0].r#ref, "constraint:freeze");
            assert_eq!(
                proof.expired[0]
                    .valid_until
                    .expect("validity end")
                    .to_string(),
                "2026-08-10T09:00:00Z"
            );
        }
    }

    /// The as-of read excludes what had ended, which is why it was the one
    /// read that could never report it.
    #[test]
    fn an_as_of_read_names_what_it_left_out_for_having_ended() {
        let response = move_over(
            lapsed_freeze_bundle(),
            TemporalDirection::Goto,
            "2026-08-25T09:00:00Z",
            TemporalAxis::Validity,
        );

        assert_eq!(
            response
                .entries
                .iter()
                .map(|entry| entry.r#ref.as_str())
                .collect::<Vec<_>>(),
            vec!["decision:cutover"]
        );
        let proof = response.proof.expect("proof");
        assert_eq!(proof.expired.len(), 1);
        assert_eq!(proof.expired[0].r#ref, "constraint:freeze");
    }

    /// Standing before the end of an interval is not standing after it.
    #[test]
    fn an_interval_that_had_not_ended_yet_is_not_reported() {
        let response = move_over(
            lapsed_freeze_bundle(),
            TemporalDirection::Forward,
            "2026-07-01T09:00:00Z",
            TemporalAxis::Validity,
        );

        assert!(response.proof.expect("proof").expired.is_empty());
    }

    #[test]
    fn temporal_proof_reports_selected_supersession_without_full_relation_path() {
        let node = |id: &str, kind: &str| {
            BundleNode::new(id, kind, id, id, "ACTIVE", Vec::new(), BTreeMap::new())
        };
        let coordinate = |target: &str, observed_at: &str| {
            BundleRelationship::new(
                "timeline:main",
                target,
                "contains_entry",
                RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("timeline")
                    .with_scope_id("timeline:main")
                    .with_observed_at(observed_at)
                    .with_sequence(1),
            )
        };
        let bundle = KmpBundle::new(
            CaseId::new("project:kmp").expect("case id"),
            Role::new("temporal-reader").expect("role"),
            node("project:kmp", "memory_anchor"),
            vec![
                node("timeline:main", "memory_dimension"),
                node("decision:old", "decision"),
                node("decision:new", "decision"),
            ],
            vec![
                coordinate("decision:old", "2026-08-27T16:55:00Z"),
                coordinate("decision:new", "2026-08-27T16:55:40Z"),
                BundleRelationship::new(
                    "decision:new",
                    "decision:old",
                    "supersedes",
                    RelationExplanation::new(RelationSemanticClass::Evidential)
                        .with_rationale("Format B replaces format A."),
                ),
            ],
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("bundle");
        let traversal = TemporalMemoryTraversal::traverse(
            &bundle,
            &TemporalTraversalRequest::new(
                TemporalDirection::Goto,
                DomainCursor::time("2026-08-27T17:00:00Z").expect("cursor"),
            ),
        )
        .expect("traversal");
        let result = TemporalMemoryResult {
            traversal,
            source_bundle: bundle,
            include: TemporalIncludeOptions {
                evidence: false,
                relations: false,
                raw_refs: false,
            },
            quality: BundleQualityMetrics::new(0, 1.0, 0.0, 0.0, 0.0).expect("quality"),
        };

        let response = temporal_response_from_result(
            TemporalCursor {
                r#ref: String::new(),
                time: Some(prost_types::Timestamp {
                    seconds: 0,
                    nanos: 0,
                }),
                sequence: None,
            },
            TemporalDirection::Goto,
            result,
        );
        let proof = response.proof.expect("proof");

        assert!(
            proof.path.is_empty(),
            "include.relations=false must be honored"
        );
        assert_eq!(proof.superseded.len(), 1);
        assert_eq!(proof.superseded[0].r#ref, "decision:old");
        assert_eq!(proof.superseded[0].superseded_by, "decision:new");
    }

    #[test]
    fn ref_cursor_historical_validity_read_marks_entries_whose_interval_ended() {
        let node = |id: &str, kind: &str| {
            BundleNode::new(id, kind, id, id, "ACTIVE", Vec::new(), BTreeMap::new())
        };
        let coordinate = |target: &str, valid_from: &str, valid_until: Option<&str>| {
            BundleRelationship::new(
                "validity:main",
                target,
                "contains_entry",
                RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("validity")
                    .with_scope_id("validity:main")
                    .with_valid_from(valid_from)
                    .with_optional_valid_until(valid_until.map(ToString::to_string)),
            )
        };
        let bundle = KmpBundle::new(
            CaseId::new("project:kmp").expect("case id"),
            Role::new("temporal-reader").expect("role"),
            node("project:kmp", "memory_anchor"),
            vec![
                node("validity:main", "memory_dimension"),
                node("constraint:expired", "constraint"),
                node("constraint:current", "constraint"),
            ],
            vec![
                coordinate(
                    "constraint:expired",
                    "2026-08-20T09:00:00Z",
                    Some("2026-08-20T12:00:00Z"),
                ),
                coordinate("constraint:current", "2026-08-20T12:00:00Z", None),
            ],
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("bundle");
        let traversal = TemporalMemoryTraversal::traverse(
            &bundle,
            &TemporalTraversalRequest::new(
                TemporalDirection::Rewind,
                DomainCursor::ref_id("constraint:current").expect("cursor"),
            )
            .with_axis(TemporalAxis::Validity),
        )
        .expect("historical traversal");
        let result = TemporalMemoryResult {
            traversal,
            source_bundle: bundle,
            include: TemporalIncludeOptions {
                evidence: false,
                relations: false,
                raw_refs: false,
            },
            quality: BundleQualityMetrics::new(0, 1.0, 0.0, 0.0, 0.0).expect("quality"),
        };

        let response = temporal_response_from_result(
            TemporalCursor {
                r#ref: "constraint:current".to_string(),
                time: None,
                sequence: None,
            },
            TemporalDirection::Rewind,
            result,
        );
        let proof = response.proof.expect("proof");

        assert_eq!(proof.expired.len(), 1);
        assert_eq!(proof.expired[0].r#ref, "constraint:expired");
        assert_eq!(
            proof.expired[0]
                .valid_until
                .expect("validity end")
                .to_string(),
            "2026-08-20T12:00:00Z"
        );
    }

    #[test]
    fn temporal_response_explains_when_ref_lacks_the_requested_clock() {
        let node = |id: &str, kind: &str| {
            BundleNode::new(id, kind, id, id, "ACTIVE", Vec::new(), BTreeMap::new())
        };
        let bundle = KmpBundle::new(
            CaseId::new("project:kmp").expect("case id"),
            Role::new("temporal-reader").expect("role"),
            node("project:kmp", "memory_anchor"),
            vec![
                node("timeline:main", "memory_dimension"),
                node("decision:sequence-only", "decision"),
            ],
            vec![BundleRelationship::new(
                "timeline:main",
                "decision:sequence-only",
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
        let traversal = TemporalMemoryTraversal::traverse(
            &bundle,
            &TemporalTraversalRequest::new(
                TemporalDirection::Rewind,
                DomainCursor::ref_id("decision:sequence-only").expect("cursor"),
            )
            .with_axis(TemporalAxis::Occurred),
        )
        .expect("existing ref should resolve");
        let result = TemporalMemoryResult {
            traversal,
            source_bundle: bundle,
            include: TemporalIncludeOptions {
                evidence: false,
                relations: false,
                raw_refs: false,
            },
            quality: BundleQualityMetrics::new(0, 1.0, 0.0, 0.0, 0.0).expect("quality"),
        };

        let response = temporal_response_from_result(
            TemporalCursor {
                r#ref: "decision:sequence-only".to_string(),
                time: None,
                sequence: None,
            },
            TemporalDirection::Rewind,
            result,
        );

        assert!(response.entries.is_empty());
        assert_eq!(
            response.warnings,
            ["temporal cursor ref `decision:sequence-only` exists but carries no `occurred` clock"]
        );
        assert_eq!(
            response.proof.expect("proof").missing,
            ["temporal_positions"]
        );
    }

    #[test]
    fn empty_time_read_names_the_explicit_clock_no_entry_carries() {
        let node = |id: &str, kind: &str| {
            BundleNode::new(id, kind, id, id, "ACTIVE", Vec::new(), BTreeMap::new())
        };
        let bundle = KmpBundle::new(
            CaseId::new("project:kmp").expect("case id"),
            Role::new("temporal-reader").expect("role"),
            node("project:kmp", "memory_anchor"),
            vec![
                node("timeline:main", "memory_dimension"),
                node("decision:sequence-only", "decision"),
            ],
            vec![BundleRelationship::new(
                "timeline:main",
                "decision:sequence-only",
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
        let traversal = TemporalMemoryTraversal::traverse(
            &bundle,
            &TemporalTraversalRequest::new(
                TemporalDirection::Rewind,
                DomainCursor::time("2026-08-27T12:00:00Z").expect("cursor"),
            )
            .with_axis(TemporalAxis::Validity),
        )
        .expect("empty validity traversal");
        let result = TemporalMemoryResult {
            traversal,
            source_bundle: bundle,
            include: TemporalIncludeOptions {
                evidence: false,
                relations: false,
                raw_refs: false,
            },
            quality: BundleQualityMetrics::new(0, 1.0, 0.0, 0.0, 0.0).expect("quality"),
        };

        let response = temporal_response_from_result(
            TemporalCursor {
                r#ref: String::new(),
                time: Some(prost_types::Timestamp {
                    seconds: 1_777_000_000,
                    nanos: 0,
                }),
                sequence: None,
            },
            TemporalDirection::Rewind,
            result,
        );

        assert_eq!(
            response.summary,
            "Returned 0 temporal entries; no entries carry the requested validity clock."
        );
        assert_eq!(
            response.warnings,
            ["no temporal entries carry the requested `validity` clock"]
        );
        assert_eq!(
            response.proof.expect("proof").missing,
            ["temporal_positions"]
        );
    }
}

#[cfg(test)]
mod ask_entry_text_tests {
    use std::collections::BTreeMap;

    use kmp_application::{GetContextResult, MemoryAnswerPolicy, queries::render_graph_bundle};
    use kmp_domain::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, KmpBundle, RelationExplanation,
        RelationSemanticClass, Role,
    };

    use super::{UNANSWERED, ask_response_from_result};

    #[test]
    fn ask_can_retrieve_a_fact_present_only_in_the_entry_text() {
        let node = |id: &str, kind: &str, summary: &str| {
            BundleNode::new(id, kind, id, summary, "ACTIVE", Vec::new(), BTreeMap::new())
        };
        let bundle = KmpBundle::new(
            CaseId::new("project:kmp").expect("case id"),
            Role::new("answerer").expect("role"),
            node("project:kmp", "memory_anchor", "KMP memory"),
            vec![
                node("timeline:main", "memory_dimension", "Timeline"),
                node(
                    "decision:format",
                    "decision",
                    "ZORBLATT is the selected durable format.",
                ),
            ],
            vec![BundleRelationship::new(
                "timeline:main",
                "decision:format",
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
        let rendered = render_graph_bundle(&bundle);
        let result = GetContextResult {
            bundle,
            rendered,
            requested_scopes: Vec::new(),
            served_at: std::time::SystemTime::UNIX_EPOCH,
            timing: None,
        };

        let response = ask_response_from_result(
            "ZORBLATT",
            None,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            None,
            result,
            &super::LexicalBridge::none(),
            &kmp_domain::TemporalSelection::Frontier,
        )
        .expect("an answer");

        assert_ne!(response.answer, UNANSWERED);
        let proof = response.proof.expect("proof");
        assert_eq!(proof.evidence.len(), 1);
        assert_eq!(proof.evidence[0].metadata["proof_role"], "entry_text");
        assert!(proof.matched_terms.contains(&"zorblatt".to_string()));
    }

    /// The graph may carry retrieval to a memory the question could not
    /// reach in words, and it still may not answer in that memory's name.
    /// The hop arrives as proof, with the path that produced it, and the
    /// answer keeps citing only what the question matched directly.
    #[test]
    fn a_memory_reached_through_the_graph_is_proof_and_never_the_answer() {
        let node = |id: &str, kind: &str, summary: &str| {
            BundleNode::new(id, kind, id, summary, "ACTIVE", Vec::new(), BTreeMap::new())
        };
        let entry = |target: &str| {
            BundleRelationship::new(
                "timeline:main",
                target,
                "contains_entry",
                RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("timeline")
                    .with_scope_id("timeline:main")
                    .with_sequence(1),
            )
        };
        let bundle = KmpBundle::new(
            CaseId::new("project:kmp").expect("case id"),
            Role::new("answerer").expect("role"),
            node("project:kmp", "memory_anchor", "KMP memory"),
            vec![
                node("timeline:main", "memory_dimension", "Timeline"),
                node(
                    "decision:zorblatt",
                    "decision",
                    "ZORBLATT is the selected durable format.",
                ),
                node(
                    "decision:quench",
                    "decision",
                    "The overnight batch window moved to the reserve cluster.",
                ),
            ],
            vec![
                entry("decision:zorblatt"),
                entry("decision:quench"),
                BundleRelationship::new(
                    "decision:zorblatt",
                    "decision:quench",
                    "chosen_because",
                    RelationExplanation::new(RelationSemanticClass::Motivational)
                        .with_rationale("the batch window forced a durable format decision")
                        .with_evidence("recorded in the migration review")
                        .with_confidence("high"),
                ),
            ],
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("bundle");
        let rendered = render_graph_bundle(&bundle);
        let result = GetContextResult {
            bundle,
            rendered,
            requested_scopes: Vec::new(),
            served_at: std::time::SystemTime::UNIX_EPOCH,
            timing: None,
        };

        let response = ask_response_from_result(
            "ZORBLATT",
            None,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            None,
            result,
            &super::LexicalBridge::none(),
            &kmp_domain::TemporalSelection::Frontier,
        )
        .expect("an answer");

        let proof = response.proof.expect("proof");
        let reached = proof
            .evidence
            .iter()
            .find(|item| item.metadata.contains_key("reached_by"))
            .expect("the graph carried retrieval to the connected memory");
        assert_eq!(reached.metadata["reached_via"], "chosen_because");
        assert_eq!(reached.metadata["reached_from"], "decision:zorblatt");
        assert!(
            response
                .because
                .iter()
                .all(|reason| reason.claim != "decision:quench"),
            "a memory reached through the graph must not be cited as the answer"
        );
        assert_eq!(response.because.len(), 1);
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
        let prioritized = prioritize_wake_relationships(
            vec![
                relation(MemorySemanticClass::Structural, "contains_entry"),
                relation(MemorySemanticClass::Procedural, "follows"),
                relation(MemorySemanticClass::Evidential, "supports"),
                relation(MemorySemanticClass::Causal, "triggers"),
            ],
            &RelationSignalIndex::default(),
        );

        assert_eq!(
            prioritized
                .iter()
                .map(|relationship| relationship.rel.as_str())
                .collect::<Vec<_>>(),
            vec!["triggers", "supports", "follows", "contains_entry"]
        );
    }

    #[test]
    fn causal_count_counts_only_causal_relation_classes() {
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

        assert_eq!(causal_count(&relations), 1);
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

#[cfg(test)]
mod wake_priority_tests {
    use std::collections::BTreeMap;

    use kmp_application::{GetContextResult, queries::render_graph_bundle};
    use kmp_domain::{
        BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId, KmpBundle,
        RelationExplanation, RelationSemanticClass, Role,
    };

    use super::wake_response_from_result;

    fn node(id: &str) -> BundleNode {
        BundleNode::new(
            id,
            "decision",
            id,
            id,
            "ACTIVE",
            Vec::new(),
            BTreeMap::new(),
        )
    }

    fn detail(id: &str, text: &str) -> BundleNodeDetail {
        BundleNodeDetail::new(id, text, "hash", 1)
    }

    fn entry(target: &str, observed_at: &str, valid_until: Option<&str>) -> BundleRelationship {
        let mut explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension("timeline")
            .with_scope_id("timeline:main")
            .with_observed_at(observed_at);
        if let Some(valid_until) = valid_until {
            explanation = explanation.with_valid_until(valid_until);
        }
        BundleRelationship::new("timeline:main", target, "contains_entry", explanation)
    }

    fn proven(class: RelationSemanticClass) -> RelationExplanation {
        RelationExplanation::new(class)
            .with_rationale("the reserve was diverted before the repair")
            .with_evidence("incident review 4711")
            .with_confidence("high")
    }

    fn wake_over(
        refs: &[&str],
        details: Vec<BundleNodeDetail>,
        relationships: Vec<BundleRelationship>,
        max_entries: Option<usize>,
    ) -> kmp_proto::v1beta1::WakeResponse {
        let bundle = KmpBundle::new(
            CaseId::new("project:kmp").expect("case id"),
            Role::new("resumer").expect("role"),
            node("project:kmp"),
            refs.iter().map(|id| node(id)).collect(),
            relationships,
            details,
            BundleMetadata::initial("test"),
        )
        .expect("bundle");
        let rendered = render_graph_bundle(&bundle);
        wake_response_from_result(
            "resume",
            max_entries,
            GetContextResult {
                bundle,
                rendered,
                requested_scopes: Vec::new(),
                served_at: std::time::SystemTime::UNIX_EPOCH,
                timing: None,
            },
            &kmp_domain::TemporalSelection::Frontier,
        )
        .expect("a packet")
    }

    /// The cap used to keep whatever the traversal emitted first. A resume
    /// packet capped at one entry must keep the decision someone proved, not
    /// the containment record that happens to sort earlier.
    #[test]
    fn the_entry_cap_keeps_what_the_writer_proved() {
        let response = wake_over(
            &[
                "timeline:main",
                "decision:bookkeeping",
                "decision:proven",
                "outcome:restored",
            ],
            vec![
                detail("decision:bookkeeping", "A routine note was filed."),
                detail(
                    "decision:proven",
                    "Reserve capacity was diverted overnight.",
                ),
            ],
            vec![
                entry("decision:bookkeeping", "2026-03-01T00:00:00Z", None),
                entry("decision:proven", "2026-03-01T00:00:00Z", None),
                BundleRelationship::new(
                    "decision:proven",
                    "outcome:restored",
                    "triggers",
                    proven(RelationSemanticClass::Causal),
                ),
            ],
            Some(1),
        );

        let proof = response.proof.expect("proof");
        assert_eq!(proof.evidence.len(), 1);
        assert_eq!(proof.evidence[0].id, "detail:decision:proven");
        assert_eq!(proof.frontier_size, 1);
    }

    /// `proof.expired` has been in the wake contract since the proof shape
    /// existed and nothing ever filled it.
    #[test]
    fn wake_names_what_stopped_applying_instead_of_only_returning_it() {
        let response = wake_over(
            &["timeline:main", "constraint:ended", "decision:live"],
            vec![
                detail("constraint:ended", "Deploys were frozen for the audit."),
                detail("decision:live", "The audit closed and deploys resumed."),
            ],
            vec![
                entry(
                    "constraint:ended",
                    "2026-01-01T00:00:00Z",
                    Some("2026-02-01T00:00:00Z"),
                ),
                entry("decision:live", "2026-03-01T00:00:00Z", None),
            ],
            None,
        );

        let proof = response.proof.expect("proof");
        assert_eq!(proof.expired.len(), 1);
        assert_eq!(proof.expired[0].r#ref, "constraint:ended");
        assert!(proof.expired[0].valid_until.is_some());
        // Still returned — wake reports state, it does not censor it — but
        // behind what still stands, so a cap cuts the stale entry first.
        assert_eq!(
            proof
                .evidence
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["detail:decision:live", "detail:constraint:ended"]
        );
    }

    /// Two causal edges used to be separated by the alphabet.
    #[test]
    fn the_causal_spine_leads_with_the_better_proven_edge() {
        let response = wake_over(
            &["a:source", "b:source", "z:target"],
            Vec::new(),
            vec![
                BundleRelationship::new(
                    "a:source",
                    "z:target",
                    "triggers",
                    RelationExplanation::new(RelationSemanticClass::Causal)
                        .with_rationale("a why with no evidence behind it"),
                ),
                BundleRelationship::new(
                    "b:source",
                    "z:target",
                    "triggers",
                    proven(RelationSemanticClass::Causal),
                ),
            ],
            None,
        );

        let spine = response.wake.expect("wake packet").causal_spine;
        assert_eq!(spine[0].claim, "b:source -> z:target");
    }

    /// The user's words are never searched; they are what the rendering is
    /// read against. A rendering that lost the ticket says so, and one that
    /// kept everything says nothing.
    #[test]
    fn a_rendering_that_lost_an_identifier_draws_one_warning() {
        let warnings = super::question_rendering_warnings(
            "Why was the launch postponed?",
            Some("¿Por qué se retrasó el despliegue de v0.7.0 (#469)?"),
        );

        assert_eq!(
            warnings,
            [
                "question is a rendering of asked_as that drops identifiers the user's words \
             carry: #469, v0.7.0; the kernel searched it as given"
            ]
        );
        assert!(
            super::question_rendering_warnings(
                "Why was the v0.7.0 launch (#469) postponed?",
                Some("¿Por qué se retrasó el despliegue de v0.7.0 (#469)?"),
            )
            .is_empty()
        );
        assert!(super::question_rendering_warnings("Why?", None).is_empty());
    }
}
