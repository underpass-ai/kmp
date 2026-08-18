use kmp_proto::v1beta1::{
    AnswerReason, AskResponse, DimensionScopeMode, DimensionSelection, DimensionSelectionMode,
    IngestResponse, InspectResponse, MemoryConfidence, MemoryEvidence, MemoryRelation,
    MemorySemanticClass, RawMemoryRef, SupersededMemory, TemporalCoordinate, TemporalCursor,
    TemporalDirection, TemporalEntry, TemporalMoveResponse, TraceResponse, WakeClaim, WakeResponse,
};
use prost_types::Timestamp;
use serde_json::{Map, Value, json};

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_domain::TokenEstimator;

use crate::ingest::KmpIngestPlan;

pub(crate) fn ingest_from_response(response: IngestResponse) -> Value {
    let memory = response.memory.as_ref();
    let accepted = memory.and_then(|memory| memory.accepted.as_ref());

    json!({
        "summary": response.summary,
        "memory": {
            "about": memory.map(|memory| memory.about.as_str()).unwrap_or(""),
            "memory_id": memory.map(|memory| memory.memory_id.as_str()).unwrap_or(""),
            "accepted": {
                "entries": accepted.map(|accepted| accepted.entries).unwrap_or_default(),
                "relations": accepted.map(|accepted| accepted.relations).unwrap_or_default(),
                "evidence": accepted.map(|accepted| accepted.evidence).unwrap_or_default()
            },
            "read_after_write_ready": memory
                .map(|memory| memory.read_after_write_ready)
                .unwrap_or(false)
        },
        "warnings": response.warnings
    })
}

pub(crate) fn dry_run_ingest_from_plan(plan: &KmpIngestPlan) -> Value {
    json!({
        "summary": format!(
            "Ingested {} {}, {} {}, and {} {} for {}.",
            plan.accepted.entries,
            plural(plan.accepted.entries, "entry", "entries"),
            plan.accepted.relations,
            plural(plan.accepted.relations, "relation", "relations"),
            plan.accepted.evidence,
            plural(plan.accepted.evidence, "evidence item", "evidence items"),
            plan.about
        ),
        "memory": {
            "about": plan.about,
            "memory_id": plan.memory_id,
            "accepted": {
                "entries": plan.accepted.entries,
                "relations": plan.accepted.relations,
                "evidence": plan.accepted.evidence
            },
            "read_after_write_ready": false
        },
        "warnings": [
            "dry_run=true; validated memory without sending a KernelMemoryService.Ingest call"
        ]
    })
}

pub(crate) fn wake_from_response(response: WakeResponse) -> Value {
    let wake = response.wake.as_ref();
    json!({
        "summary": response.summary,
        "wake": {
            "objective": wake.map(|wake| wake.objective.as_str()).unwrap_or(""),
            "current_state": wake
                .map(|wake| wake.current_state.clone())
                .unwrap_or_default(),
            "causal_spine": wake
                .map(|wake| wake.causal_spine.iter().map(wake_claim_json).collect::<Vec<_>>())
                .unwrap_or_default(),
            "open_loops": wake.map(|wake| wake.open_loops.clone()).unwrap_or_default(),
            "next_actions": wake.map(|wake| wake.next_actions.clone()).unwrap_or_default(),
            "guardrails": wake.map(|wake| wake.guardrails.clone()).unwrap_or_default()
        },
        "proof": response.proof.as_ref().map(proof_json).unwrap_or_else(empty_proof_json),
        // Where this packet ends, so the next question — "and what changed
        // since?" — is one call to kernel_forward rather than a rewind whose
        // only purpose is to recover a timestamp.
        "resume_cursor": response
            .resume_cursor
            .as_ref()
            .map(temporal_cursor_json)
            .unwrap_or(Value::Null),
        "warnings": response.warnings
    })
}

pub(crate) fn ask_from_response(response: AskResponse) -> Value {
    let evidence = response
        .proof
        .as_ref()
        .map(|proof| proof.evidence.as_slice())
        .unwrap_or_default();
    let answer = normalized_ask_answer(&response.answer, &response.because, evidence);
    json!({
        "summary": response.summary,
        "answer": if answer.trim().is_empty() {
            Value::Null
        } else {
            Value::String(answer)
        },
        "because": response
            .because
            .iter()
            .map(|reason| answer_reason_json(reason, evidence))
            .collect::<Vec<_>>(),
        "proof": response.proof.as_ref().map(proof_json).unwrap_or_else(empty_proof_json),
        "warnings": response.warnings
    })
}

fn normalized_ask_answer(
    answer: &str,
    reasons: &[AnswerReason],
    evidence: &[MemoryEvidence],
) -> String {
    let repeats_canonical_body = evidence.iter().any(|item| {
        let body = item.text.trim();
        !body.is_empty() && answer.contains(body)
    });
    if !repeats_canonical_body {
        return answer.to_string();
    }

    citation_answer(
        reasons
            .iter()
            .map(|reason| (reason.claim.as_str(), reason.r#ref.as_str())),
    )
}

fn citation_answer<'a>(citations: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut seen = std::collections::BTreeSet::new();
    let citations = citations
        .into_iter()
        .filter_map(|(claim, evidence_ref)| {
            let evidence_ref = evidence_ref.trim();
            if evidence_ref.is_empty() || !seen.insert(evidence_ref.to_string()) {
                return None;
            }
            let claim = claim.trim();
            Some(if claim.is_empty() {
                evidence_ref.to_string()
            } else {
                format!("{claim} [{evidence_ref}]")
            })
        })
        .collect::<Vec<_>>();

    match citations.as_slice() {
        [] => String::new(),
        [single] => {
            format!("Memory answer supported by {single}; canonical text is in proof.evidence.")
        }
        many => format!(
            "Memory answer supported by cited evidence (canonical text in proof.evidence):\n{}",
            many.iter()
                .map(|citation| format!("- {citation}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

#[derive(Clone, Default)]
struct OutputTruncation {
    path: usize,
    evidence: usize,
    conflicts: usize,
    superseded: usize,
    missing: usize,
    state: usize,
    reasons: usize,
    repeated_why: usize,
    text_chars: usize,
}

impl OutputTruncation {
    fn total_items(&self) -> usize {
        self.path
            + self.evidence
            + self.conflicts
            + self.superseded
            + self.missing
            + self.state
            + self.reasons
            + self.repeated_why
    }

    fn any(&self) -> bool {
        self.total_items() > 0 || self.text_chars > 0
    }
}

/// Applies the caller's budget to the JSON that the MCP host actually sees.
///
/// The application renderer already budgets its prose, but proof paths and
/// evidence were appended afterwards and could dwarf it. This final gate uses
/// the same cl100k estimator over serialized JSON, reports visible omissions,
/// and makes compact mode structurally compact rather than merely asking the
/// renderer for a shorter summary.
pub(crate) fn enforce_recall_output_budget(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
) -> Value {
    let estimator = Cl100kEstimator::new();
    enforce_recall_output_budget_with_estimator(value, arguments, default_tokens, &estimator)
}

fn enforce_recall_output_budget_with_estimator(
    mut value: Value,
    arguments: &Value,
    default_tokens: u32,
    estimator: &dyn TokenEstimator,
) -> Value {
    let base_frontier = value
        .pointer("/proof/frontier_size")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let budget = arguments.get("budget").and_then(Value::as_object);
    let token_limit = budget
        .and_then(|budget| budget.get("tokens"))
        .and_then(Value::as_u64)
        .and_then(|tokens| u32::try_from(tokens).ok())
        .filter(|tokens| *tokens > 0)
        .unwrap_or(default_tokens);
    let compact = budget
        .and_then(|budget| budget.get("detail"))
        .and_then(Value::as_str)
        == Some("compact");
    let mut omitted = OutputTruncation::default();

    if let Some(max_entries) = budget
        .and_then(|budget| budget.get("max_entries"))
        .and_then(Value::as_u64)
        .and_then(|entries| usize::try_from(entries).ok())
        .filter(|entries| *entries > 0)
    {
        cap_array(
            &mut value,
            &["proof", "evidence"],
            max_entries,
            &mut omitted.evidence,
        );
        cap_array(&mut value, &["because"], max_entries, &mut omitted.reasons);
        if omitted.reasons > 0 {
            rebuild_answer_from_reasons(&mut value);
        }
    }

    if compact {
        if let Some(path) = array_at_mut(&mut value, &["proof", "path"]) {
            let before = path.len();
            path.retain(|relation| {
                relation.get("class").and_then(Value::as_str) != Some("structural")
            });
            omitted.path += before - path.len();

            let mut seen_why = std::collections::BTreeSet::new();
            for relation in path.iter_mut() {
                let repeated = relation
                    .get("why")
                    .and_then(Value::as_str)
                    .is_some_and(|why| !why.is_empty() && !seen_why.insert(why.to_string()));
                if repeated && let Some(relation) = relation.as_object_mut() {
                    relation.remove("why");
                    omitted.repeated_why += 1;
                }
            }
        }
        cap_array(&mut value, &["proof", "evidence"], 3, &mut omitted.evidence);
        cap_array(
            &mut value,
            &["proof", "conflicts"],
            3,
            &mut omitted.conflicts,
        );
        cap_array(
            &mut value,
            &["proof", "superseded"],
            3,
            &mut omitted.superseded,
        );
        cap_array(
            &mut value,
            &["wake", "current_state"],
            3,
            &mut omitted.state,
        );
        cap_array(&mut value, &["wake", "causal_spine"], 4, &mut omitted.state);
        cap_array(&mut value, &["wake", "open_loops"], 3, &mut omitted.state);
        cap_array(&mut value, &["wake", "next_actions"], 3, &mut omitted.state);
        cap_array(&mut value, &["wake", "guardrails"], 3, &mut omitted.state);
        cap_array(&mut value, &["because"], 3, &mut omitted.reasons);
        if omitted.reasons > 0 {
            rebuild_answer_from_reasons(&mut value);
        }
    }

    loop {
        // Truncation metadata is part of the packet and therefore part of the
        // budget. Keep the prunable source unmarked so its sentinel and
        // accounting cannot themselves be mistaken for proof items.
        let mut candidate = value.clone();
        if omitted.any() {
            make_truncation_accounting_truthful(&mut candidate, &omitted);
            mark_output_truncation(&mut candidate, token_limit, &omitted, base_frontier);
        }
        if serialized_tokens(&candidate, estimator) <= token_limit {
            return candidate;
        }
        if !drop_one_low_priority_item(&mut value, &mut omitted) {
            break;
        }
    }

    // A single answer reason or wake state may itself exceed the whole
    // envelope. Reach this fallback only after every lower-priority item has
    // been exhausted, then bound the retained core text.
    if value.get("answer").is_some() {
        fit_minimal_ask_packet(&value, token_limit, estimator, base_frontier, &omitted)
    } else if value.get("wake").is_some() {
        fit_minimal_wake_packet(&value, token_limit, estimator, base_frontier, &omitted)
    } else {
        Value::Object(Map::new())
    }
}

fn serialized_tokens(value: &Value, estimator: &dyn TokenEstimator) -> u32 {
    estimator
        .estimate_tokens(&serde_json::to_string(value).expect("KMP response JSON should serialize"))
}

fn array_at_mut<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Vec<Value>> {
    let mut current = value;
    for key in path {
        current = current.get_mut(*key)?;
    }
    current.as_array_mut()
}

fn cap_array(value: &mut Value, path: &[&str], limit: usize, omitted: &mut usize) {
    let Some(array) = array_at_mut(value, path) else {
        return;
    };
    if array.len() > limit {
        *omitted += array.len() - limit;
        array.truncate(limit);
    }
}

fn pop_array(value: &mut Value, path: &[&str], keep: usize) -> bool {
    let Some(array) = array_at_mut(value, path) else {
        return false;
    };
    if array.len() <= keep {
        return false;
    }
    array.pop();
    true
}

fn drop_one_low_priority_item(value: &mut Value, omitted: &mut OutputTruncation) -> bool {
    if pop_array(value, &["proof", "path"], 0) {
        omitted.path += 1;
    } else if pop_array(value, &["proof", "evidence"], 0) {
        omitted.evidence += 1;
    } else if pop_array(value, &["proof", "conflicts"], 0) {
        omitted.conflicts += 1;
    } else if pop_array(value, &["proof", "superseded"], 0) {
        omitted.superseded += 1;
    } else if pop_array(value, &["proof", "missing"], 0) {
        omitted.missing += 1;
    } else if pop_array(value, &["wake", "open_loops"], 1)
        || pop_array(value, &["wake", "next_actions"], 1)
        || pop_array(value, &["wake", "guardrails"], 1)
        || pop_array(value, &["wake", "causal_spine"], 1)
        || pop_array(value, &["wake", "current_state"], 1)
    {
        omitted.state += 1;
    } else if pop_array(value, &["because"], 1) {
        omitted.reasons += 1;
        rebuild_answer_from_reasons(value);
    } else {
        return false;
    }
    true
}

fn make_ask_summary_truthful(value: &mut Value, omitted: &OutputTruncation) {
    if value.get("answer").is_none() {
        return;
    }
    let retained = value
        .get("because")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    value["summary"] = json!(format!(
        "Truncated deterministic memory answer; retained {retained} cited {}; {}.",
        plural(retained, "reason", "reasons"),
        omission_summary(omitted)
    ));
}

fn make_truncation_accounting_truthful(value: &mut Value, omitted: &OutputTruncation) {
    if value.get("answer").is_some() {
        make_ask_summary_truthful(value, omitted);
        append_warning(
            value,
            "output truncated to budget.tokens; use kernel_inspect on because refs or kernel_trace to expand",
        );
    } else if value.get("wake").is_some() {
        value["summary"] = json!(format!(
            "Truncated wake packet; retained a bounded state slice; {}.",
            omission_summary(omitted)
        ));
        append_warning(
            value,
            "output truncated to budget.tokens; use kernel_near or kernel_inspect to expand",
        );
    }
}

fn append_warning(value: &mut Value, warning: &str) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let warnings = object
        .entry("warnings")
        .or_insert_with(|| json!([]))
        .as_array_mut();
    if let Some(warnings) = warnings
        && !warnings.iter().any(|item| item.as_str() == Some(warning))
    {
        warnings.push(json!(warning));
    }
}

fn fit_minimal_ask_packet(
    source: &Value,
    token_limit: u32,
    estimator: &dyn TokenEstimator,
    base_frontier: u64,
    omitted: &OutputTruncation,
) -> Value {
    let mut base_omitted = omitted.clone();
    account_remaining_proof_items(source, &mut base_omitted);

    let reasons = source
        .get("because")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    base_omitted.reasons += reasons.len().saturating_sub(1);
    let reason = reasons.first().cloned();
    let answer = reason
        .as_ref()
        .and_then(|reason| reason.get("evidence"))
        .and_then(Value::as_str)
        .filter(|answer| !answer.is_empty())
        .or_else(|| source.get("answer").and_then(Value::as_str))
        .unwrap_or("UNKNOWN")
        .to_string();
    let high = answer
        .chars()
        .count()
        .max(reason.as_ref().map(max_truncatable_text_chars).unwrap_or(0));

    let build = |max_chars: usize| {
        let mut candidate_omitted = base_omitted.clone();
        let (bounded_answer, answer_chars) = truncate_text(&answer, max_chars);
        candidate_omitted.text_chars += answer_chars;
        let bounded_reasons = reason
            .as_ref()
            .map(|reason| {
                let mut reason = reason.clone();
                candidate_omitted.text_chars += truncate_json_text(&mut reason, max_chars);
                reason
            })
            .into_iter()
            .collect::<Vec<_>>();
        let retained = bounded_reasons.len();
        let mut candidate = json!({
            "summary": format!(
                "Truncated deterministic memory answer; retained {retained} cited {}; {}.",
                plural(retained, "reason", "reasons"),
                omission_summary(&candidate_omitted)
            ),
            "answer": bounded_answer,
            "because": bounded_reasons,
            "proof": minimal_proof_json(source, base_frontier),
            "warnings": ["output truncated to budget.tokens; retained answer and citation; use kernel_inspect on because refs to expand"]
        });
        mark_output_truncation(
            &mut candidate,
            token_limit,
            &candidate_omitted,
            base_frontier,
        );
        candidate
    };

    largest_fitting_packet(high, token_limit, estimator, build)
}

fn fit_minimal_wake_packet(
    source: &Value,
    token_limit: u32,
    estimator: &dyn TokenEstimator,
    base_frontier: u64,
    omitted: &OutputTruncation,
) -> Value {
    let mut base_omitted = omitted.clone();
    account_remaining_proof_items(source, &mut base_omitted);
    let mut wake = source.get("wake").cloned().unwrap_or_else(|| json!({}));
    for key in [
        "current_state",
        "causal_spine",
        "open_loops",
        "next_actions",
        "guardrails",
    ] {
        cap_array(&mut wake, &[key], 1, &mut base_omitted.state);
    }
    let high = max_truncatable_text_chars(&wake);

    let build = |max_chars: usize| {
        let mut candidate_omitted = base_omitted.clone();
        let mut bounded_wake = wake.clone();
        candidate_omitted.text_chars += truncate_json_text(&mut bounded_wake, max_chars);
        let mut candidate = json!({
            "summary": format!(
                "Truncated wake packet; retained a bounded state slice; {}.",
                omission_summary(&candidate_omitted)
            ),
            "wake": bounded_wake,
            "proof": minimal_proof_json(source, base_frontier),
            "resume_cursor": source.get("resume_cursor").cloned().unwrap_or(Value::Null),
            "warnings": ["output truncated to budget.tokens; use kernel_near or kernel_inspect to expand"]
        });
        mark_output_truncation(
            &mut candidate,
            token_limit,
            &candidate_omitted,
            base_frontier,
        );
        candidate
    };

    largest_fitting_packet(high, token_limit, estimator, build)
}

fn omission_summary(omitted: &OutputTruncation) -> String {
    let items = omitted.total_items();
    match (items, omitted.text_chars) {
        (0, chars) => format!(
            "shortened {chars} text {}",
            plural(chars, "character", "characters")
        ),
        (items, 0) => format!("omitted {items} {}", plural(items, "item", "items")),
        (items, chars) => format!(
            "omitted {items} {} and shortened {chars} text {}",
            plural(items, "item", "items"),
            plural(chars, "character", "characters")
        ),
    }
}

fn largest_fitting_packet(
    high: usize,
    token_limit: u32,
    estimator: &dyn TokenEstimator,
    build: impl Fn(usize) -> Value,
) -> Value {
    let mut low = 0usize;
    let mut high = high;
    let mut best = None;
    while low <= high {
        let midpoint = low + (high - low) / 2;
        let candidate = build(midpoint);
        if serialized_tokens(&candidate, estimator) <= token_limit {
            best = Some(candidate);
            low = midpoint.saturating_add(1);
        } else if midpoint == 0 {
            break;
        } else {
            high = midpoint - 1;
        }
    }
    best.unwrap_or_else(|| Value::Object(Map::new()))
}

fn minimal_proof_json(source: &Value, base_frontier: u64) -> Value {
    json!({
        "path": [],
        "evidence": [],
        "conflicts": [],
        "superseded": [],
        "missing": [],
        "frontier_size": base_frontier,
        "matched_terms": source
            .pointer("/proof/matched_terms")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "matched_relations": source
            .pointer("/proof/matched_relations")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "confidence": source
            .pointer("/proof/confidence")
            .cloned()
            .unwrap_or_else(|| json!("unknown"))
    })
}

fn account_remaining_proof_items(value: &Value, omitted: &mut OutputTruncation) {
    omitted.path += array_len(value, &["proof", "path"]);
    omitted.evidence += array_len(value, &["proof", "evidence"]);
    omitted.conflicts += array_len(value, &["proof", "conflicts"]);
    omitted.superseded += array_len(value, &["proof", "superseded"]);
    omitted.missing += value
        .pointer("/proof/missing")
        .and_then(Value::as_array)
        .map(|missing| {
            missing
                .iter()
                .filter(|item| item.as_str() != Some("output truncated to token budget"))
                .count()
        })
        .unwrap_or_default();
}

fn array_len(value: &Value, path: &[&str]) -> usize {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return 0;
        };
        current = next;
    }
    current.as_array().map(Vec::len).unwrap_or_default()
}

fn max_truncatable_text_chars(value: &Value) -> usize {
    match value {
        Value::String(text) => text.chars().count(),
        Value::Array(items) => items
            .iter()
            .map(max_truncatable_text_chars)
            .max()
            .unwrap_or_default(),
        Value::Object(object) => object
            .iter()
            .filter(|(key, _)| !is_reference_key(key))
            .map(|(_, value)| max_truncatable_text_chars(value))
            .max()
            .unwrap_or_default(),
        _ => 0,
    }
}

fn truncate_json_text(value: &mut Value, max_chars: usize) -> usize {
    match value {
        Value::String(text) => {
            let (bounded, omitted) = truncate_text(text, max_chars);
            *text = bounded;
            omitted
        }
        Value::Array(items) => items
            .iter_mut()
            .map(|item| truncate_json_text(item, max_chars))
            .sum(),
        Value::Object(object) => object
            .iter_mut()
            .filter(|(key, _)| !is_reference_key(key))
            .map(|(_, value)| truncate_json_text(value, max_chars))
            .sum(),
        _ => 0,
    }
}

fn is_reference_key(key: &str) -> bool {
    key == "ref" || key.ends_with("_ref")
}

fn truncate_text(value: &str, max_chars: usize) -> (String, usize) {
    let total = value.chars().count();
    if total <= max_chars {
        return (value.to_string(), 0);
    }
    let mut bounded = value.chars().take(max_chars).collect::<String>();
    bounded.push('…');
    (bounded, total - max_chars)
}

fn rebuild_answer_from_reasons(value: &mut Value) {
    let Some(reasons) = value.get("because").and_then(Value::as_array) else {
        return;
    };
    let citations = reasons
        .iter()
        .filter_map(|reason| {
            let evidence_ref = reason.get("ref").and_then(Value::as_str)?.trim();
            if evidence_ref.is_empty() {
                return None;
            }
            let claim = reason
                .get("claim")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            Some(if claim.is_empty() {
                evidence_ref.to_string()
            } else {
                format!("{claim} [{evidence_ref}]")
            })
        })
        .collect::<Vec<_>>();
    value["answer"] = match citations.as_slice() {
        [] => Value::Null,
        [single] => Value::String(format!(
            "Memory answer supported by {single}; canonical text is in proof.evidence."
        )),
        many => Value::String(format!(
            "Memory answer supported by cited evidence (canonical text in proof.evidence):\n{}",
            many.iter()
                .map(|citation| format!("- {citation}"))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    };
}

fn mark_output_truncation(
    value: &mut Value,
    token_limit: u32,
    omitted: &OutputTruncation,
    base_frontier: u64,
) {
    value["truncation"] = json!({
        "truncated": true,
        "token_limit": token_limit,
        "omitted": {
            "proof_path": omitted.path,
            "proof_evidence": omitted.evidence,
            "proof_conflicts": omitted.conflicts,
            "proof_superseded": omitted.superseded,
            "proof_missing": omitted.missing,
            "state_or_spine": omitted.state,
            "answer_reasons": omitted.reasons,
            "repeated_why": omitted.repeated_why,
            "text_chars": omitted.text_chars
        }
    });
    if let Some(proof) = value.get_mut("proof").and_then(Value::as_object_mut) {
        let missing = proof
            .entry("missing")
            .or_insert_with(|| json!([]))
            .as_array_mut();
        if let Some(missing) = missing
            && !missing
                .iter()
                .any(|item| item == "output truncated to token budget")
        {
            missing.push(json!("output truncated to token budget"));
        }
        proof.insert("frontier_size".to_string(), json!(base_frontier));
    }
}

pub(crate) fn temporal_from_response(response: TemporalMoveResponse) -> Value {
    json!({
        "summary": response.summary,
        "temporal": response
            .temporal
            .as_ref()
            .map(temporal_state_json)
            .unwrap_or(Value::Null),
        "coverage": response
            .coverage
            .as_ref()
            .map(|coverage| {
                json!({
                    "requested": coverage
                        .requested
                        .as_ref()
                        .map(dimension_selection_json)
                        .unwrap_or(Value::Null),
                    "included": coverage.included,
                    "missing": coverage.missing,
                    "dimensions": coverage
                        .dimensions
                        .iter()
                        .map(dimension_coverage_json)
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_else(|| json!({
                "requested": Value::Null,
                "included": Vec::<String>::new(),
                "missing": Vec::<String>::new(),
                "dimensions": Vec::<Value>::new()
        })),
        "entries": response.entries.iter().map(temporal_entry_json).collect::<Vec<_>>(),
        "page": response
            .page
            .as_ref()
            .map(page_info_json)
            .unwrap_or_else(empty_page_info_json),
        "raw_refs": response.raw_refs.iter().map(raw_memory_ref_json).collect::<Vec<_>>(),
        "proof": response.proof.as_ref().map(proof_json).unwrap_or_else(empty_proof_json),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    })
}

pub(crate) fn trace_from_response(response: TraceResponse) -> Value {
    json!({
        "summary": response.summary,
        "trace": response.trace.iter().map(memory_relation_json).collect::<Vec<_>>(),
        "page": response
            .page
            .as_ref()
            .map(page_info_json)
            .unwrap_or_else(empty_page_info_json),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    })
}

pub(crate) fn inspect_from_response(response: InspectResponse) -> Value {
    let object = response.object.as_ref().map_or_else(
        || {
            json!({
                "ref": "",
                "kind": "",
                "text": "",
                "metadata": {}
            })
        },
        |object| {
            let mut value = Map::new();
            value.insert("ref".to_string(), json!(object.r#ref));
            value.insert("kind".to_string(), json!(object.kind));
            value.insert("text".to_string(), json!(object.text));
            value.insert("metadata".to_string(), json!(object.metadata));
            insert_optional_string(&mut value, "source", &object.source);
            Value::Object(value)
        },
    );
    let links = response.links.as_ref();

    json!({
        "summary": response.summary,
        "object": object,
        "links": {
            "incoming": links
                .map(|links| links.incoming.iter().map(memory_relation_json).collect::<Vec<_>>())
                .unwrap_or_default(),
            "outgoing": links
                .map(|links| links.outgoing.iter().map(memory_relation_json).collect::<Vec<_>>())
                .unwrap_or_default()
        },
        "evidence": response.evidence.iter().map(memory_evidence_json).collect::<Vec<_>>(),
        "raw": response.raw.iter().map(raw_memory_ref_json).collect::<Vec<_>>(),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    })
}

fn wake_claim_json(claim: &WakeClaim) -> Value {
    json!({
        "claim": claim.claim,
        "because": claim.because,
        "evidence_ref": claim.evidence_ref
    })
}

fn answer_reason_json(reason: &AnswerReason, evidence: &[MemoryEvidence]) -> Value {
    let mut object = Map::new();
    object.insert("claim".to_string(), json!(reason.claim));
    // v1beta1 keeps the protobuf field for wire compatibility. New recall
    // responses leave it empty and join through `ref` to proof.evidence.
    let repeats_canonical_body = evidence.iter().any(|item| {
        item.id == reason.r#ref && !item.text.is_empty() && item.text == reason.evidence
    });
    if !repeats_canonical_body {
        insert_optional_string(&mut object, "evidence", &reason.evidence);
    }
    object.insert("ref".to_string(), json!(reason.r#ref));
    Value::Object(object)
}

fn dimension_coverage_json(dimension: &kmp_proto::v1beta1::DimensionCoverage) -> Value {
    json!({
        "dimension": dimension.dimension,
        "returned": dimension.returned,
        "present": dimension.present
    })
}

fn response_quality_json(quality: &kmp_proto::v1beta1::ResponseQuality) -> Value {
    json!({
        "nodes": quality.nodes,
        "relationships": quality.relationships,
        "details": quality.details,
        "detail_coverage": quality.detail_coverage,
        "causal_density": quality.causal_density,
        "truncated": quality.truncated
    })
}

fn optional_quality_json(quality: Option<&kmp_proto::v1beta1::ResponseQuality>) -> Value {
    quality.map(response_quality_json).unwrap_or(Value::Null)
}

fn proof_json(proof: &kmp_proto::v1beta1::Proof) -> Value {
    json!({
        "path": proof
            .path
            .iter()
            .map(|relation| proof_relation_json(relation, &proof.evidence))
            .collect::<Vec<_>>(),
        "evidence": proof.evidence.iter().map(memory_evidence_json).collect::<Vec<_>>(),
        "conflicts": proof.conflicts,
        // Kept apart from conflicts: a supersession is a lifecycle, not a
        // disagreement, and the older entry is history rather than advice.
        "superseded": proof
            .superseded
            .iter()
            .map(superseded_json)
            .collect::<Vec<_>>(),
        "missing": proof.missing,
        "frontier_size": proof.frontier_size,
        "matched_terms": proof.matched_terms,
        "matched_relations": proof.matched_relations,
        "confidence": confidence_label(proof.confidence)
    })
}

fn proof_relation_json(relation: &MemoryRelation, evidence: &[MemoryEvidence]) -> Value {
    let mut relation = relation.clone();
    let mut refs = relation
        .evidence_refs
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
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
    memory_relation_json(&relation)
}

fn superseded_json(entry: &SupersededMemory) -> Value {
    let mut object = Map::new();
    object.insert("ref".to_string(), json!(entry.r#ref));
    object.insert("superseded_by".to_string(), json!(entry.superseded_by));
    insert_optional_string(&mut object, "why", &entry.why);
    Value::Object(object)
}

fn empty_proof_json() -> Value {
    json!({
        "path": [],
        "evidence": [],
        "conflicts": [],
        "superseded": [],
        "missing": ["proof"],
        "frontier_size": 1,
        "matched_terms": [],
        "matched_relations": [],
        "confidence": "unknown"
    })
}

fn page_info_json(page: &kmp_proto::v1beta1::PageInfo) -> Value {
    json!({
        "returned": page.returned,
        "total": page.total,
        "has_more": page.has_more,
        "next_cursor": if page.next_cursor.trim().is_empty() {
            Value::Null
        } else {
            Value::String(page.next_cursor.clone())
        }
    })
}

fn empty_page_info_json() -> Value {
    json!({
        "returned": 0,
        "total": 0,
        "has_more": false,
        "next_cursor": Value::Null
    })
}

fn temporal_state_json(state: &kmp_proto::v1beta1::TemporalState) -> Value {
    json!({
        "direction": temporal_direction_label(state.direction),
        "requested": state
            .requested
            .as_ref()
            .map(temporal_cursor_json)
            .unwrap_or(Value::Null),
        "resolved": state
            .resolved
            .as_ref()
            .map(temporal_coordinate_json)
            .unwrap_or(Value::Null)
    })
}

fn temporal_entry_json(entry: &TemporalEntry) -> Value {
    json!({
        "ref": entry.r#ref,
        "kind": entry.kind,
        "text": entry.text,
        "coordinates": entry.coordinates.iter().map(temporal_coordinate_json).collect::<Vec<_>>(),
        "metadata": entry.metadata
    })
}

fn temporal_cursor_json(cursor: &TemporalCursor) -> Value {
    let mut object = Map::new();
    insert_optional_string(&mut object, "ref", &cursor.r#ref);
    if let Some(time) = cursor.time {
        object.insert("time".to_string(), json!(time.to_string()));
    }
    if let Some(sequence) = cursor.sequence {
        object.insert("sequence".to_string(), json!(sequence));
    }
    Value::Object(object)
}

fn temporal_coordinate_json(coordinate: &TemporalCoordinate) -> Value {
    let mut object = Map::new();
    insert_optional_string(&mut object, "dimension", &coordinate.dimension);
    insert_optional_string(&mut object, "scope_id", &coordinate.scope_id);
    insert_optional_timestamp(&mut object, "occurred_at", coordinate.occurred_at);
    insert_optional_timestamp(&mut object, "observed_at", coordinate.observed_at);
    insert_optional_timestamp(&mut object, "ingested_at", coordinate.ingested_at);
    insert_optional_timestamp(&mut object, "valid_from", coordinate.valid_from);
    insert_optional_timestamp(&mut object, "valid_until", coordinate.valid_until);
    if let Some(sequence) = coordinate.sequence {
        object.insert("sequence".to_string(), json!(sequence));
    }
    if let Some(rank) = coordinate.rank {
        object.insert("rank".to_string(), json!(rank));
    }
    if !coordinate.metadata.is_empty() {
        object.insert("metadata".to_string(), json!(coordinate.metadata));
    }
    Value::Object(object)
}

fn dimension_selection_json(selection: &DimensionSelection) -> Value {
    let mut object = Map::new();
    object.insert(
        "mode".to_string(),
        json!(dimension_selection_mode_label(selection.mode)),
    );
    if !selection.include.is_empty() {
        object.insert("include".to_string(), json!(selection.include));
    }
    if !selection.exclude.is_empty() {
        object.insert("exclude".to_string(), json!(selection.exclude));
    }
    if !selection.scope_ids.is_empty() {
        object.insert("scope_ids".to_string(), json!(selection.scope_ids));
    }
    object.insert(
        "scope".to_string(),
        json!(dimension_scope_mode_label(selection.scope)),
    );
    if !selection.abouts.is_empty() {
        object.insert("abouts".to_string(), json!(selection.abouts));
    }
    Value::Object(object)
}

fn memory_relation_json(relation: &MemoryRelation) -> Value {
    let mut object = Map::new();
    object.insert("from".to_string(), json!(relation.source_ref));
    object.insert("to".to_string(), json!(relation.target_ref));
    object.insert("rel".to_string(), json!(relation.rel));
    object.insert(
        "class".to_string(),
        json!(semantic_class_label(relation.semantic_class)),
    );
    insert_optional_string(&mut object, "why", &relation.why);
    insert_optional_string(&mut object, "evidence", &relation.evidence);
    object.insert(
        "confidence".to_string(),
        json!(confidence_label(relation.confidence)),
    );
    if let Some(sequence) = relation.sequence {
        object.insert("sequence".to_string(), json!(sequence));
    }
    if let Some(explanation) = relation.explanation.as_ref() {
        insert_optional_string(&mut object, "motivation", &explanation.motivation);
        insert_optional_string(&mut object, "method", &explanation.method);
        insert_optional_string(&mut object, "decision_id", &explanation.decision_id);
        insert_optional_string(
            &mut object,
            "caused_by_node_id",
            &explanation.caused_by_node_id,
        );
        if let Some(coordinate) = explanation.coordinate.as_ref() {
            object.insert(
                "coordinate".to_string(),
                temporal_coordinate_json(coordinate),
            );
        }
    }
    if !relation.evidence_refs.is_empty() {
        object.insert("evidence_refs".to_string(), json!(relation.evidence_refs));
    }
    Value::Object(object)
}

fn memory_evidence_json(evidence: &MemoryEvidence) -> Value {
    let mut object = Map::new();
    object.insert("id".to_string(), json!(evidence.id));
    object.insert("supports".to_string(), json!(evidence.supports));
    object.insert("text".to_string(), json!(evidence.text));
    insert_optional_string(&mut object, "source", &evidence.source);
    insert_optional_timestamp(&mut object, "time", evidence.time);
    if !evidence.metadata.is_empty() {
        object.insert("metadata".to_string(), json!(evidence.metadata));
    }
    Value::Object(object)
}

fn raw_memory_ref_json(raw: &RawMemoryRef) -> Value {
    json!({
        "ref": raw.r#ref,
        "kind": raw.kind,
        "text": raw.text,
        "coordinates": raw.coordinates.iter().map(temporal_coordinate_json).collect::<Vec<_>>(),
        "detail": raw.detail,
        "content_hash": raw.content_hash,
        "revision": raw.revision
    })
}

fn insert_optional_string(object: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        object.insert(key.to_string(), json!(value));
    }
}

fn insert_optional_timestamp(object: &mut Map<String, Value>, key: &str, value: Option<Timestamp>) {
    if let Some(value) = value {
        object.insert(key.to_string(), json!(value.to_string()));
    }
}

fn semantic_class_label(value: i32) -> &'static str {
    match MemorySemanticClass::try_from(value) {
        Ok(MemorySemanticClass::Structural) => "structural",
        Ok(MemorySemanticClass::Causal) => "causal",
        Ok(MemorySemanticClass::Motivational) => "motivational",
        Ok(MemorySemanticClass::Procedural) => "procedural",
        Ok(MemorySemanticClass::Evidential) => "evidential",
        Ok(MemorySemanticClass::Constraint) => "constraint",
        _ => "unspecified",
    }
}

fn confidence_label(value: i32) -> &'static str {
    match MemoryConfidence::try_from(value) {
        Ok(MemoryConfidence::High) => "high",
        Ok(MemoryConfidence::Medium) => "medium",
        Ok(MemoryConfidence::Low) => "low",
        Ok(MemoryConfidence::Unknown) => "unknown",
        _ => "unspecified",
    }
}

fn temporal_direction_label(value: i32) -> &'static str {
    match TemporalDirection::try_from(value) {
        Ok(TemporalDirection::Goto) => "goto",
        Ok(TemporalDirection::Near) => "near",
        Ok(TemporalDirection::Rewind) => "rewind",
        Ok(TemporalDirection::Forward) => "forward",
        _ => "unspecified",
    }
}

fn dimension_selection_mode_label(value: i32) -> &'static str {
    match DimensionSelectionMode::try_from(value) {
        Ok(DimensionSelectionMode::All) => "all",
        Ok(DimensionSelectionMode::Only) => "only",
        Ok(DimensionSelectionMode::Except) => "except",
        _ => "unspecified",
    }
}

fn dimension_scope_mode_label(value: i32) -> &'static str {
    match DimensionScopeMode::try_from(value) {
        Ok(DimensionScopeMode::CurrentAbout) => "current_about",
        Ok(DimensionScopeMode::Abouts) => "abouts",
        Ok(DimensionScopeMode::AllAbouts) => "all_abouts",
        _ => "current_about",
    }
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

#[cfg(test)]
mod tests {
    use kmp_proto::v1beta1::{MemoryBudget, MemoryDetailLevel, Proof, TemporalState, WakePacket};

    use super::*;

    #[test]
    fn maps_typed_ask_response_without_inventing_null_answer() {
        let response = AskResponse {
            summary: "No deterministic memory answer found.".to_string(),
            answer: String::new(),
            because: vec![AnswerReason {
                claim: "claim".to_string(),
                evidence: "evidence".to_string(),
                r#ref: "evidence:1".to_string(),
            }],
            proof: Some(Proof {
                path: vec![relation()],
                evidence: vec![evidence()],
                conflicts: Vec::new(),
                superseded: Vec::new(),
                missing: vec!["generative_answer".to_string()],
                frontier_size: 1,
                matched_terms: vec!["deterministic".to_string()],
                matched_relations: vec!["supports".to_string()],
                confidence: MemoryConfidence::Medium as i32,
            }),
            warnings: Vec::new(),
        };

        let value = ask_from_response(response);

        assert_eq!(value["answer"], Value::Null);
        assert_eq!(value["because"][0]["ref"], "evidence:1");
        assert_eq!(value["proof"]["path"][0]["from"], "claim:source");
        assert_eq!(value["proof"]["confidence"], "medium");
        assert_eq!(value["proof"]["frontier_size"], 1);
    }

    #[test]
    fn three_reason_packet_serializes_each_evidence_body_once() {
        let bodies = (0..3)
            .map(|index| {
                format!(
                    "Evidence body {index} establishes the selected claim with exact temporal and provenance detail. {}",
                    "grounded context remains canonical here. ".repeat(24)
                )
            })
            .collect::<Vec<_>>();
        let reasons = bodies
            .iter()
            .enumerate()
            .map(|(index, body)| AnswerReason {
                claim: format!("claim:{index}"),
                evidence: body.clone(),
                r#ref: format!("detail:evidence:{index}"),
            })
            .collect::<Vec<_>>();
        let evidence = bodies
            .iter()
            .enumerate()
            .map(|(index, body)| MemoryEvidence {
                id: format!("detail:evidence:{index}"),
                supports: vec![format!("claim:{index}")],
                text: body.clone(),
                source: format!("source:{index}"),
                time: None,
                metadata: Default::default(),
            })
            .collect::<Vec<_>>();
        let path = (0..12)
            .map(|index| {
                let evidence_index = index % 3;
                MemoryRelation {
                    source_ref: format!("evidence:{evidence_index}"),
                    target_ref: format!("claim:{evidence_index}"),
                    rel: "supports".to_string(),
                    semantic_class: MemorySemanticClass::Evidential as i32,
                    why: "Evidence supports the selected memory claim.".to_string(),
                    evidence: bodies[evidence_index].clone(),
                    confidence: MemoryConfidence::High as i32,
                    sequence: Some(index as u32 + 1),
                    explanation: None,
                    evidence_refs: Vec::new(),
                }
            })
            .collect::<Vec<_>>();
        let legacy_answer = bodies
            .iter()
            .map(|body| format!("- {body}"))
            .collect::<Vec<_>>()
            .join("\n");
        let response = AskResponse {
            summary: "Deterministic memory answer from 3 evidence items.".to_string(),
            answer: legacy_answer.clone(),
            because: reasons.clone(),
            proof: Some(Proof {
                path: path.clone(),
                evidence: evidence.clone(),
                conflicts: Vec::new(),
                superseded: Vec::new(),
                missing: Vec::new(),
                frontier_size: 0,
                matched_terms: vec!["canonical".to_string()],
                matched_relations: vec!["supports".to_string()],
                confidence: MemoryConfidence::High as i32,
            }),
            warnings: Vec::new(),
        };
        let legacy = json!({
            "summary": response.summary,
            "answer": legacy_answer,
            "because": reasons.iter().map(|reason| json!({
                "claim": reason.claim,
                "evidence": reason.evidence,
                "ref": reason.r#ref
            })).collect::<Vec<_>>(),
            "proof": {
                "path": path.iter().map(memory_relation_json).collect::<Vec<_>>(),
                "evidence": evidence.iter().map(memory_evidence_json).collect::<Vec<_>>(),
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "matched_terms": ["canonical"],
                "matched_relations": ["supports"],
                "confidence": "high"
            },
            "warnings": []
        });

        let outputs = (0..3)
            .map(|_| {
                serde_json::to_string(&ask_from_response(response.clone()))
                    .expect("normalized ask should serialize")
            })
            .collect::<Vec<_>>();
        let normalized = &outputs[0];
        let legacy = serde_json::to_string(&legacy).expect("legacy ask should serialize");
        let estimator = Cl100kEstimator::new();
        let normalized_tokens = estimator.estimate_tokens(normalized);
        let legacy_tokens = estimator.estimate_tokens(&legacy);
        let normalized_value: Value =
            serde_json::from_str(normalized).expect("normalized ask should parse");
        let legacy_value: Value = serde_json::from_str(&legacy).expect("legacy ask should parse");
        let canonical_body_bytes = bodies.iter().map(String::len).sum::<usize>();
        // Legacy owns every body in answer, because, proof.evidence, and four
        // supporting path hops. The normalized packet owns the registry copy.
        let legacy_owned_body_bytes = canonical_body_bytes * 7;
        let normalized_owned_body_bytes = canonical_body_bytes;
        let samples = 250;
        let legacy_started = std::time::Instant::now();
        for _ in 0..samples {
            std::hint::black_box(
                serde_json::to_vec(std::hint::black_box(&legacy_value))
                    .expect("legacy ask should serialize repeatedly"),
            );
        }
        let legacy_serialization = legacy_started.elapsed();
        let normalized_started = std::time::Instant::now();
        for _ in 0..samples {
            std::hint::black_box(
                serde_json::to_vec(std::hint::black_box(&normalized_value))
                    .expect("normalized ask should serialize repeatedly"),
            );
        }
        let normalized_serialization = normalized_started.elapsed();

        println!(
            "three-reason normalization: {} -> {} bytes; {} -> {} advisory cl100k tokens; owned evidence-body bytes {} -> {}; {samples} serializations {:?} -> {:?}",
            legacy.len(),
            normalized.len(),
            legacy_tokens,
            normalized_tokens,
            legacy_owned_body_bytes,
            normalized_owned_body_bytes,
            legacy_serialization,
            normalized_serialization
        );

        assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
        for body in &bodies {
            assert_eq!(normalized.matches(body).count(), 1, "{body}");
        }
        assert!(
            normalized.len() * 100 <= legacy.len() * 40,
            "normalized={} legacy={}",
            normalized.len(),
            legacy.len()
        );
        assert!(normalized.len() < 10_000, "{} bytes", normalized.len());

        let canonical_ids = normalized_value["proof"]["evidence"]
            .as_array()
            .expect("canonical evidence registry")
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(
            normalized_value["because"]
                .as_array()
                .expect("reasons")
                .iter()
                .all(|reason| reason.get("evidence").is_none()
                    && reason["ref"]
                        .as_str()
                        .is_some_and(|evidence_ref| canonical_ids.contains(evidence_ref)))
        );
        assert!(
            normalized_value["proof"]["path"]
                .as_array()
                .expect("proof path")
                .iter()
                .all(|relation| relation.get("evidence").is_none()
                    && relation["evidence_refs"]
                        .as_array()
                        .is_some_and(|refs| !refs.is_empty()
                            && refs.iter().all(|evidence_ref| {
                                evidence_ref.as_str().is_some_and(|evidence_ref| {
                                    canonical_ids.contains(evidence_ref)
                                })
                            })))
        );
    }

    #[test]
    fn maps_temporal_response_to_kmp_json_names() {
        let response = TemporalMoveResponse {
            summary: "Returned 1 temporal entry.".to_string(),
            temporal: Some(TemporalState {
                direction: TemporalDirection::Forward as i32,
                requested: Some(TemporalCursor {
                    r#ref: "claim:source".to_string(),
                    time: None,
                    sequence: None,
                }),
                resolved: Some(coordinate()),
            }),
            coverage: Some(kmp_proto::v1beta1::TemporalCoverage {
                requested: Some(DimensionSelection {
                    mode: DimensionSelectionMode::Only as i32,
                    include: vec!["timeline".to_string()],
                    exclude: Vec::new(),
                    scope: DimensionScopeMode::CurrentAbout as i32,
                    abouts: Vec::new(),
                    scope_ids: vec!["timeline:main".to_string()],
                }),
                included: vec!["timeline".to_string()],
                missing: Vec::new(),
                dimensions: vec![kmp_proto::v1beta1::DimensionCoverage {
                    dimension: "timeline".to_string(),
                    returned: 1,
                    present: true,
                }],
            }),
            entries: vec![TemporalEntry {
                r#ref: "claim:target".to_string(),
                kind: "claim".to_string(),
                text: "Target".to_string(),
                coordinates: vec![coordinate()],
                metadata: [("window".to_string(), "10:00-10:20".to_string())]
                    .into_iter()
                    .collect(),
            }],
            proof: None,
            warnings: Vec::new(),
            raw_refs: Vec::new(),
            page: Some(kmp_proto::v1beta1::PageInfo {
                returned: 1,
                total: 2,
                has_more: true,
                next_cursor: "claim:target".to_string(),
            }),
            quality: Some(kmp_proto::v1beta1::ResponseQuality {
                nodes: 1,
                relationships: 0,
                details: 1,
                detail_coverage: 1.0,
                causal_density: 0.0,
                truncated: true,
            }),
        };

        let value = temporal_from_response(response);

        assert_eq!(value["temporal"]["direction"], "forward");
        assert_eq!(value["entries"][0]["ref"], "claim:target");
        assert_eq!(value["entries"][0]["coordinates"][0]["scope_id"], "scope");
        assert_eq!(value["entries"][0]["metadata"]["window"], "10:00-10:20");
        assert_eq!(value["coverage"]["requested"]["scope"], "current_about");
        assert_eq!(
            value["coverage"]["requested"]["scope_ids"][0],
            "timeline:main"
        );
        assert_eq!(value["coverage"]["dimensions"][0]["dimension"], "timeline");
        assert_eq!(value["coverage"]["dimensions"][0]["returned"], 1);
        assert_eq!(value["coverage"]["dimensions"][0]["present"], true);
        assert_eq!(value["quality"]["nodes"], 1);
        assert_eq!(value["quality"]["details"], 1);
        assert_eq!(value["quality"]["detail_coverage"], 1.0);
        assert_eq!(value["quality"]["truncated"], true);
        assert_eq!(value["page"]["returned"], 1);
        assert_eq!(value["page"]["total"], 2);
        assert_eq!(value["page"]["has_more"], true);
        assert_eq!(value["page"]["next_cursor"], "claim:target");
    }

    #[test]
    fn maps_wake_and_ignores_transport_budget_types() {
        let response = WakeResponse {
            summary: "Wake summary".to_string(),
            wake: Some(WakePacket {
                objective: "continue".to_string(),
                current_state: vec!["state".to_string()],
                causal_spine: vec![WakeClaim {
                    claim: "claim".to_string(),
                    because: "because".to_string(),
                    evidence_ref: "evidence:1".to_string(),
                }],
                open_loops: Vec::new(),
                next_actions: Vec::new(),
                guardrails: Vec::new(),
            }),
            proof: None,
            resume_cursor: Some(TemporalCursor {
                r#ref: "decision:latest".to_string(),
                time: Some(prost_types::Timestamp {
                    seconds: 1_786_924_800,
                    nanos: 0,
                }),
                sequence: Some(3),
            }),
            warnings: Vec::new(),
        };
        let _budget = MemoryBudget {
            tokens: 1,
            detail: MemoryDetailLevel::Full as i32,
            depth: 1,
            max_entries: 0,
        };

        let value = wake_from_response(response);

        assert_eq!(value["wake"]["current_state"][0], "state");
        assert_eq!(
            value["wake"]["causal_spine"][0]["evidence_ref"],
            "evidence:1"
        );
        // The bookmark a caller carries to kernel_forward, so catching up is
        // one more call rather than a rewind that exists to find a timestamp.
        assert_eq!(value["resume_cursor"]["ref"], "decision:latest");
        assert_eq!(value["resume_cursor"]["sequence"], 3);
    }

    #[test]
    fn compact_output_filters_structure_and_honours_the_serialized_token_limit() {
        let path = (0..40)
            .map(|index| {
                json!({
                    "from": format!("about:root:{index}"),
                    "to": format!("entry:{index}"),
                    "rel": "contains_entry",
                    "class": "structural",
                    "why": "Repeated structural boilerplate that must not fill compact output",
                    "confidence": "high"
                })
            })
            .collect::<Vec<_>>();
        let evidence = (0..20)
            .map(|index| {
                json!({
                    "id": format!("evidence:{index}"),
                    "supports": [format!("entry:{index}")],
                    "text": "Long evidence text repeated to exercise final MCP packet budgeting",
                    "source": format!("source:{index}")
                })
            })
            .collect::<Vec<_>>();
        let value = json!({
            "summary": "Recall summary",
            "wake": {"current_state": ["state"], "causal_spine": []},
            "proof": {
                "path": path,
                "evidence": evidence,
                "missing": [],
                "frontier_size": 0,
                "confidence": "medium"
            },
            "warnings": []
        });
        let arguments = json!({"budget": {"tokens": 400, "detail": "compact"}});

        let bounded = enforce_recall_output_budget(value, &arguments, 1600);
        let estimator = Cl100kEstimator::new();
        assert!(serialized_tokens(&bounded, &estimator) <= 400);
        assert!(
            bounded["proof"]["path"]
                .as_array()
                .expect("proof path remains typed")
                .iter()
                .all(|relation| relation["class"] != "structural")
        );
        assert_eq!(bounded["truncation"]["truncated"], true);
    }

    #[test]
    fn transport_omissions_do_not_change_the_graph_frontier() {
        let proof = json!({
            "proof": {
                "path": [],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": ["unexplored:one", "unexplored:two"],
                "frontier_size": 2,
                "confidence": "high"
            }
        });
        let mut low_budget = proof.clone();
        let mut high_budget = proof;
        let low_budget_omissions = OutputTruncation {
            path: 30,
            evidence: 4,
            ..Default::default()
        };
        let high_budget_omissions = OutputTruncation {
            path: 10,
            evidence: 1,
            ..Default::default()
        };

        mark_output_truncation(&mut low_budget, 500, &low_budget_omissions, 2);
        mark_output_truncation(&mut high_budget, 1_200, &high_budget_omissions, 2);

        for result in [&low_budget, &high_budget] {
            assert_eq!(result["proof"]["frontier_size"], 2);
            assert_eq!(result["truncation"]["truncated"], true);
        }
        assert_ne!(
            low_budget["truncation"]["omitted"]["proof_path"],
            high_budget["truncation"]["omitted"]["proof_path"],
            "the regression must exercise different transport omission counts"
        );
    }

    #[test]
    fn max_entries_caps_ask_reasons_as_well_as_proof_evidence() {
        let reasons = (0..5)
            .map(|index| json!({"evidence": format!("answer {index}")}))
            .collect::<Vec<_>>();
        let evidence = (0..5)
            .map(|index| json!({"id": format!("evidence:{index}"), "text": "answer"}))
            .collect::<Vec<_>>();
        let value = json!({
            "summary": "answer",
            "answer": "unbounded",
            "because": reasons,
            "proof": {"path": [], "evidence": evidence, "missing": [], "frontier_size": 0},
            "warnings": []
        });

        let bounded = enforce_recall_output_budget(
            value,
            &json!({"budget": {"tokens": 1000, "max_entries": 2}}),
            2400,
        );

        assert_eq!(bounded["because"].as_array().expect("reasons").len(), 2);
        assert_eq!(
            bounded["proof"]["evidence"]
                .as_array()
                .expect("evidence")
                .len(),
            2
        );
    }

    #[test]
    fn truncation_envelope_is_budgeted_before_falling_back_to_the_minimal_packet() {
        let value = recall_budget_fixture();
        let mut compact_source = value.clone();
        compact_source["proof"]["path"]
            .as_array_mut()
            .expect("proof path")
            .retain(|relation| relation["class"] != "structural");
        let estimator = Cl100kEstimator::new();
        let token_limit = serialized_tokens(&compact_source, &estimator);

        // This is the exact cliff from #94: the pruned source fits, while its
        // truthful truncation envelope does not. The gate must prune another
        // proof item instead of collapsing to one reason.
        let bounded = enforce_recall_output_budget(
            value,
            &json!({"budget": {"tokens": token_limit, "detail": "compact"}}),
            2_400,
        );

        assert!(serialized_tokens(&bounded, &estimator) <= token_limit);
        assert_eq!(bounded["because"].as_array().expect("reasons").len(), 3);
        assert_eq!(bounded["truncation"]["truncated"], true);
        assert!(
            bounded["summary"]
                .as_str()
                .expect("summary")
                .starts_with("Truncated deterministic memory answer")
        );
        assert!(!bounded["warnings"].as_array().expect("warnings").is_empty());
    }

    #[test]
    fn ask_budget_sweep_preserves_the_ceiling_and_monotone_cited_core() {
        let value = recall_budget_fixture();
        let estimator = Cl100kEstimator::new();

        for detail in ["compact", "balanced", "full"] {
            let mut previous_reasons = 0;
            let mut previous_evidence = 0;
            for token_limit in (800..=6_000).step_by(25) {
                let bounded = enforce_recall_output_budget_with_estimator(
                    value.clone(),
                    &json!({
                        "budget": {
                            "tokens": token_limit,
                            "detail": detail,
                            "max_entries": 12
                        }
                    }),
                    2_400,
                    &estimator,
                );
                let reasons = array_len(&bounded, &["because"]);
                let evidence = array_len(&bounded, &["proof", "evidence"]);

                assert!(
                    serialized_tokens(&bounded, &estimator) <= token_limit,
                    "{detail} exceeded the ceiling at {token_limit} tokens"
                );
                assert!(
                    reasons >= previous_reasons,
                    "{detail} lost cited reasons when the budget grew to {token_limit}"
                );
                assert!(
                    evidence >= previous_evidence,
                    "{detail} lost proof evidence when the budget grew to {token_limit}"
                );
                if bounded
                    .pointer("/truncation/truncated")
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    assert!(
                        bounded["summary"]
                            .as_str()
                            .expect("summary")
                            .starts_with("Truncated deterministic memory answer")
                    );
                    assert!(!bounded["warnings"].as_array().expect("warnings").is_empty());
                }
                previous_reasons = reasons;
                previous_evidence = evidence;
            }
        }
    }

    #[test]
    fn wake_budget_sweep_preserves_the_ceiling_and_monotone_state() {
        let value = wake_budget_fixture();
        let estimator = Cl100kEstimator::new();
        let mut previous_state = 0;

        for token_limit in (800..=6_000).step_by(25) {
            let bounded = enforce_recall_output_budget_with_estimator(
                value.clone(),
                &json!({"budget": {"tokens": token_limit, "detail": "balanced"}}),
                2_400,
                &estimator,
            );
            let retained_state = [
                "current_state",
                "causal_spine",
                "open_loops",
                "next_actions",
                "guardrails",
            ]
            .into_iter()
            .map(|section| array_len(&bounded, &["wake", section]))
            .sum::<usize>();

            assert!(serialized_tokens(&bounded, &estimator) <= token_limit);
            assert!(
                retained_state >= previous_state,
                "wake lost state when the budget grew to {token_limit}"
            );
            if bounded
                .pointer("/truncation/truncated")
                .and_then(Value::as_bool)
                == Some(true)
            {
                assert!(
                    bounded["summary"]
                        .as_str()
                        .expect("summary")
                        .starts_with("Truncated wake packet")
                );
                assert!(!bounded["warnings"].as_array().expect("warnings").is_empty());
            }
            previous_state = retained_state;
        }
    }

    #[test]
    fn ask_budget_keeps_a_bounded_answer_and_minimal_proof() {
        let reasons = (0..5)
            .map(|index| {
                json!({
                    "claim": format!("claim:{index}"),
                    "evidence": format!("reason {index} {}", "evidence ".repeat(4_000)),
                    "ref": format!("evidence:{index}")
                })
            })
            .collect::<Vec<_>>();
        let answer = reasons
            .iter()
            .map(|reason| format!("- {}", reason["evidence"].as_str().expect("evidence")))
            .collect::<Vec<_>>()
            .join("\n");
        let value = json!({
            "summary": "Deterministic memory answer from 5 evidence items for: why?",
            "answer": answer,
            "because": reasons,
            "proof": {
                "path": [],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "confidence": "high"
            },
            "warnings": []
        });

        for (arguments, token_limit) in [
            (json!({}), 2_400),
            (json!({"budget": {"detail": "compact"}}), 2_400),
            (json!({"budget": {"tokens": 400}}), 400),
        ] {
            let bounded = enforce_recall_output_budget(value.clone(), &arguments, 2_400);
            let estimator = Cl100kEstimator::new();

            assert!(serialized_tokens(&bounded, &estimator) <= token_limit);
            assert!(
                bounded["answer"]
                    .as_str()
                    .is_some_and(|answer| !answer.is_empty()),
                "the payload must survive the budget gate"
            );
            assert_eq!(
                bounded["because"]
                    .as_array()
                    .expect("a minimal citation survives")
                    .len(),
                1
            );
            assert_eq!(bounded["because"][0]["ref"], "evidence:0");
            assert!(bounded["proof"].is_object());
            assert_eq!(bounded["proof"]["confidence"], "high");
            assert!(
                bounded["summary"]
                    .as_str()
                    .expect("summary")
                    .to_ascii_lowercase()
                    .contains("truncated")
            );
            assert_eq!(bounded["truncation"]["truncated"], true);
            assert!(bounded["truncation"]["omitted"].is_object());
            assert!(
                bounded["truncation"]["omitted"]["text_chars"]
                    .as_u64()
                    .is_some_and(|chars| chars > 0)
            );
            assert!(bounded["truncation"].get("omitted_items").is_none());
            assert!(
                bounded["proof"]["missing"]
                    .as_array()
                    .expect("proof omissions")
                    .iter()
                    .any(|item| item == "output truncated to token budget")
            );
        }
    }

    #[test]
    fn wake_budget_keeps_the_wake_shape_when_retained_text_is_too_large() {
        let value = json!({
            "summary": "Objective: keep working.",
            "wake": {
                "objective": format!("objective {}", "detail ".repeat(4_000)),
                "current_state": [format!("state {}", "detail ".repeat(4_000))],
                "causal_spine": [{
                    "claim": "claim",
                    "because": format!("because {}", "detail ".repeat(4_000)),
                    "evidence_ref": "evidence:1"
                }],
                "open_loops": [],
                "next_actions": [],
                "guardrails": []
            },
            "proof": {
                "path": [],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "confidence": "medium"
            },
            "resume_cursor": {"ref": "decision:latest"},
            "warnings": []
        });

        let bounded = enforce_recall_output_budget(value, &json!({}), 1_600);
        let estimator = Cl100kEstimator::new();

        assert!(serialized_tokens(&bounded, &estimator) <= 1_600);
        assert!(bounded["wake"].is_object());
        assert!(bounded["proof"].is_object());
        assert_eq!(bounded["resume_cursor"]["ref"], "decision:latest");
        assert_eq!(bounded["truncation"]["truncated"], true);
        assert!(
            bounded["truncation"]["omitted"]["text_chars"]
                .as_u64()
                .is_some_and(|chars| chars > 0)
        );
        assert!(
            bounded["summary"]
                .as_str()
                .expect("summary")
                .to_ascii_lowercase()
                .contains("truncated")
        );
    }

    fn recall_budget_fixture() -> Value {
        let reasons = (0..3)
            .map(|index| {
                json!({
                    "claim": format!("claim:{index}"),
                    "evidence": format!("Reason {index}: {}", "specific evidence detail ".repeat(6)),
                    "ref": format!("evidence:{index}")
                })
            })
            .collect::<Vec<_>>();
        let answer = reasons
            .iter()
            .map(|reason| format!("- {}", reason["evidence"].as_str().expect("evidence")))
            .collect::<Vec<_>>()
            .join("\n");
        let mut path = vec![json!({
            "from": "about:root",
            "to": "claim:0",
            "rel": "contains_entry",
            "class": "structural",
            "why": "structural bookkeeping",
            "confidence": "high"
        })];
        path.extend((0..8).map(|index| {
            json!({
                "from": format!("claim:{index}"),
                "to": format!("evidence:{}", index % 3),
                "rel": "supports",
                "class": "evidential",
                "why": format!("Semantic proof hop {index}: {}", "relationship detail ".repeat(4)),
                "evidence": format!("hop evidence {index}: {}", "grounded detail ".repeat(4)),
                "confidence": "high"
            })
        }));
        let evidence = (0..3)
            .map(|index| {
                json!({
                    "id": format!("evidence:{index}"),
                    "supports": [format!("claim:{index}")],
                    "text": format!("Reason {index}: {}", "specific evidence detail ".repeat(6)),
                    "source": format!("source:{index}")
                })
            })
            .collect::<Vec<_>>();

        json!({
            "summary": "Deterministic memory answer from 3 evidence items.",
            "answer": answer,
            "because": reasons,
            "proof": {
                "path": path,
                "evidence": evidence,
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "matched_terms": ["truncation"],
                "matched_relations": ["supports"],
                "confidence": "high"
            },
            "warnings": []
        })
    }

    fn wake_budget_fixture() -> Value {
        let state = (0..6)
            .map(|index| format!("state {index}: {}", "specific retained detail ".repeat(4)))
            .collect::<Vec<_>>();
        let claims = (0..6)
            .map(|index| {
                json!({
                    "claim": format!("claim {index}: {}", "specific retained detail ".repeat(4)),
                    "because": format!("reason {index}: {}", "grounded detail ".repeat(4)),
                    "evidence_ref": format!("evidence:{index}")
                })
            })
            .collect::<Vec<_>>();

        json!({
            "summary": "Wake summary",
            "wake": {
                "objective": "continue truncation work",
                "current_state": state,
                "causal_spine": claims,
                "open_loops": ["open loop 0", "open loop 1", "open loop 2"],
                "next_actions": ["next action 0", "next action 1", "next action 2"],
                "guardrails": ["guardrail 0", "guardrail 1", "guardrail 2"]
            },
            "proof": {
                "path": [],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "confidence": "high"
            },
            "resume_cursor": {"ref": "decision:latest"},
            "warnings": []
        })
    }

    fn relation() -> MemoryRelation {
        MemoryRelation {
            source_ref: "claim:source".to_string(),
            target_ref: "claim:target".to_string(),
            rel: "supports".to_string(),
            semantic_class: MemorySemanticClass::Evidential as i32,
            why: "why".to_string(),
            evidence: "evidence".to_string(),
            confidence: MemoryConfidence::High as i32,
            sequence: Some(1),
            explanation: None,
            evidence_refs: Vec::new(),
        }
    }

    fn evidence() -> MemoryEvidence {
        MemoryEvidence {
            id: "evidence:1".to_string(),
            supports: vec!["claim:target".to_string()],
            text: "Evidence".to_string(),
            source: "source".to_string(),
            time: None,
            metadata: Default::default(),
        }
    }

    fn coordinate() -> TemporalCoordinate {
        TemporalCoordinate {
            dimension: "timeline".to_string(),
            scope_id: "scope".to_string(),
            occurred_at: None,
            observed_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
            sequence: Some(2),
            rank: None,
            metadata: Default::default(),
        }
    }
}
