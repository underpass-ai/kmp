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
/// Reached because the lexical-bridge table says some of the question's words
/// and some of this candidate's words mean the same thing in two languages —
/// not enough of them to answer, enough to be worth showing.
pub(super) const REACHED_BY_BRIDGE: &str = "bridge";
/// The word pairs the table bridged on a candidate that did answer, so a
/// citation that crossed a language says which words carried it.
pub(super) const BRIDGED_TERMS_KEY: &str = "bridged_terms";
/// A cited memory a writer declared to be the same thing as one the question
/// matched: the ref it restates, and the relation that says so.
pub(super) const RESTATED_FROM_KEY: &str = "restated_from";
pub(super) const RESTATED_VIA_KEY: &str = "restated_via";
/// Metadata the ranker writes about how a candidate was retrieved. It is
/// read by people and never by the ranker: letting `valvula≈valve 0.51`
/// back into a candidate's searchable text would make a bridged word look
/// like one the memory wrote.
const RETRIEVAL_PROVENANCE_KEYS: &[&str] = &[
    REACHED_BY_KEY,
    "reached_from",
    "reached_via",
    "reached_hops",
    BRIDGED_TERMS_KEY,
    RESTATED_FROM_KEY,
    RESTATED_VIA_KEY,
];

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

/// Records that a candidate arrived through the table: which of the
/// question's words it bridged, and to what, so the route can be audited
/// pair by pair.
pub(super) fn mark_bridged(item: MemoryEvidence, pairs: &str) -> MemoryEvidence {
    let mut item = mark_reached_by(item, REACHED_BY_BRIDGE);
    item.metadata
        .insert("reached_via".to_string(), pairs.to_string());
    item
}

/// Records, on a candidate that answers, that it does so because a writer
/// declared it the same thing as a memory the question matched. It is cited:
/// declared equivalence is a claim about the answer, not a route to it.
pub(super) fn mark_restated(item: MemoryEvidence, hop: &RelationReach) -> MemoryEvidence {
    let mut item = item;
    item.metadata
        .insert(RESTATED_FROM_KEY.to_string(), hop.from_ref.clone());
    item.metadata
        .insert(RESTATED_VIA_KEY.to_string(), hop.via_relation.clone());
    item
}

/// Records, on a candidate that answered, that some of the words it answered
/// with were the table's rather than the reader's.
pub(super) fn note_bridged_terms(mut item: MemoryEvidence, pairs: &str) -> MemoryEvidence {
    item.metadata
        .insert(BRIDGED_TERMS_KEY.to_string(), pairs.to_string());
    item
}

/// Whether a metadata key describes how a candidate was retrieved rather
/// than what the memory says.
pub(super) fn is_retrieval_provenance(key: &str) -> bool {
    RETRIEVAL_PROVENANCE_KEYS.contains(&key)
}

/// Whether a candidate arrived by something other than the question's own
/// words — a proven relation, this memory's own vocabulary, or the table.
pub(super) fn was_reached_indirectly(item: &MemoryEvidence) -> bool {
    item.metadata.contains_key(REACHED_BY_KEY)
}
