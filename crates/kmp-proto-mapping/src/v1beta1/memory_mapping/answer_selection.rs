use std::collections::BTreeSet;

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate::AnswerCandidate;
use super::question_intent::QuestionIntent;
use super::relation_feature::RelationFeature;
use super::relation_reach::RelationReach;
use super::search_terms::matching_terms;

/// Metadata marking a candidate the question never matched on its own text,
/// reached by walking proven relations out from one that did.
pub(super) const REACHED_BY_KEY: &str = "reached_by";
pub(super) const REACHED_BY_RELATION: &str = "relation";
/// Reached because this memory's own vocabulary says the question's words and
/// this candidate's words go together.
pub(super) const REACHED_BY_ASSOCIATION: &str = "association";

const MAX_RERANK_CANDIDATES: usize = 64;

pub(super) fn diversify_candidates(
    question_terms: &BTreeSet<String>,
    focus_terms: &BTreeSet<String>,
    mut candidates: Vec<AnswerCandidate>,
) -> Vec<AnswerCandidate> {
    if candidates.len() < 2 {
        return candidates;
    }

    // Novelty is useful only near the answer boundary. Bounding the greedy
    // window keeps reranking independent of graph degree; the complete proof
    // tail retains its already deterministic relevance order.
    let tail = if candidates.len() > MAX_RERANK_CANDIDATES {
        candidates.split_off(MAX_RERANK_CANDIDATES)
    } else {
        Vec::new()
    };

    let first = candidates.remove(0);
    let mut covered = matching_terms(question_terms, &first.searchable_terms);
    let mut covered_focus = matching_terms(focus_terms, &first.searchable_terms);
    let mut ranked = vec![first];

    while !candidates.is_empty() {
        let mut best_index = 0;
        let mut best_key = None;
        for (index, candidate) in candidates.iter().enumerate() {
            let focus_gain = matching_terms(focus_terms, &candidate.searchable_terms)
                .difference(&covered_focus)
                .count();
            let total_gain = matching_terms(question_terms, &candidate.searchable_terms)
                .difference(&covered)
                .count();
            let key = (candidate.relevance, focus_gain, total_gain);
            if best_key.is_none_or(|current| key > current) {
                best_index = index;
                best_key = Some(key);
            }
        }

        let selected = candidates.remove(best_index);
        covered.extend(matching_terms(question_terms, &selected.searchable_terms));
        covered_focus.extend(matching_terms(focus_terms, &selected.searchable_terms));
        ranked.push(selected);
    }
    ranked.extend(tail);
    ranked
}

pub(super) fn prioritize_distinct_claims(candidates: Vec<AnswerCandidate>) -> Vec<AnswerCandidate> {
    let mut seen_claims = BTreeSet::new();
    let mut distinct = Vec::with_capacity(candidates.len());
    let mut repeated = Vec::new();

    for candidate in candidates {
        let claim = candidate
            .item
            .supports
            .first()
            .map(String::as_str)
            .unwrap_or(candidate.item.source.as_str());
        if seen_claims.insert(claim.to_string()) {
            distinct.push(candidate);
        } else {
            repeated.push(candidate);
        }
    }
    distinct.extend(repeated);
    distinct
}

pub(super) fn stable_evidence_key(item: &MemoryEvidence) -> String {
    format!(
        "{}\u{0}{}\u{0}{}\u{0}{}",
        item.id,
        item.supports
            .first()
            .map(String::as_str)
            .unwrap_or_default(),
        item.source,
        item.text
    )
}

pub(super) fn answer_context_refs(item: &MemoryEvidence) -> BTreeSet<String> {
    item.id
        .strip_prefix("detail:")
        .map(str::to_string)
        .into_iter()
        .chain(item.supports.iter().cloned())
        .collect()
}

/// How many of a candidate's relations are the kind the question asked for.
///
/// An unspecific question asks for none, and every candidate scores zero —
/// which is what the ranker did before questions had an intent at all.
pub(super) fn intent_relation_matches(
    intent: &QuestionIntent,
    relations: &[&RelationFeature],
) -> usize {
    if intent.is_unspecific() {
        return 0;
    }
    relations
        .iter()
        .filter(|relation| intent.matches(&relation.rel, &relation.semantic_class))
        .count()
}

/// The writer's judgment of the relations that actually touch the question,
/// summed and capped.
///
/// Only matching relations count: a candidate cannot climb by being densely
/// connected to material the question never mentioned.
pub(super) fn relation_signal_total(
    question_terms: &BTreeSet<String>,
    relations: &[&RelationFeature],
) -> u32 {
    relations
        .iter()
        .filter(|relation| relation.matches_any(question_terms))
        .map(|relation| relation.signal)
        .sum()
}

/// Records how a rescued candidate was reached, so the hop can be audited
/// rather than trusted.
/// Records that a candidate arrived by something other than the question's own
/// words, so a reader can weigh the route instead of taking it on faith.
pub(super) fn mark_reached_by(mut item: MemoryEvidence, how: &str) -> MemoryEvidence {
    item.metadata
        .insert(REACHED_BY_KEY.to_string(), how.to_string());
    item
}

pub(super) fn mark_reached(item: MemoryEvidence, hop: &RelationReach) -> MemoryEvidence {
    let mut item = mark_reached_by(item, REACHED_BY_RELATION);
    item.metadata
        .insert("reached_from".to_string(), hop.from_ref.clone());
    item.metadata
        .insert("reached_via".to_string(), hop.via_relation.clone());
    item.metadata
        .insert("reached_hops".to_string(), hop.hops.to_string());
    item
}

/// Whether a candidate arrived by something other than the question's own
/// words — a proven relation, or this memory's own vocabulary.
pub(super) fn was_reached_indirectly(item: &MemoryEvidence) -> bool {
    item.metadata.contains_key(REACHED_BY_KEY)
}
