use std::collections::BTreeSet;

pub(super) use kmp_domain::language::{fold_search_term, informative_tokens};

use super::morphology::Morphology;
use super::term_counts::TermCounts;

pub(super) fn matching_terms(
    question_terms: &BTreeSet<String>,
    evidence_terms: &BTreeSet<String>,
) -> BTreeSet<String> {
    question_terms
        .iter()
        .filter(|question_term| {
            evidence_terms
                .iter()
                .any(|evidence_term| terms_match(question_term, evidence_term))
        })
        .cloned()
        .collect()
}

pub(super) fn matching_term_count(
    question_terms: &BTreeSet<String>,
    evidence_terms: &BTreeSet<String>,
) -> usize {
    matching_terms(question_terms, evidence_terms)
        .iter()
        .map(|term| concept_key(term))
        .collect::<BTreeSet<_>>()
        .len()
}

pub(super) fn concept_count(terms: &BTreeSet<String>) -> usize {
    terms
        .iter()
        .map(|term| concept_key(term))
        .collect::<BTreeSet<_>>()
        .len()
}

/// Extracts the subject-bearing clause used by strict answer policies.
pub(super) fn strict_answer_focus_terms(
    question: &str,
    morphology: &Morphology,
) -> BTreeSet<String> {
    const CONTEXT_BOUNDARIES: &[&str] = &[
        "after", "before", "because", "if", "once", "when", "while", "antes", "cuando", "despues",
        "después", "mientras", "porque", "si",
    ];
    const GENERIC_QUESTION_PREDICATES: &[&str] = &[
        "happen", "happened", "occur", "occurred", "ocurrio", "ocurrió", "paso", "pasó", "prove",
        "proved", "proves",
    ];

    let main_clause = question
        .split(|character: char| !character.is_alphanumeric())
        .map(fold_search_term)
        .take_while(|token| !CONTEXT_BOUNDARIES.contains(&token.as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let mut terms = informative_terms(&main_clause, morphology);
    for predicate in GENERIC_QUESTION_PREDICATES {
        terms.remove(*predicate);
    }
    if terms.is_empty() {
        informative_terms(question, morphology)
    } else {
        terms
    }
}

/// The same tokens `informative_terms` yields, kept with their frequencies
/// and collapsed onto concept keys so BM25 weighs a synonym pair once.
pub(super) fn informative_term_counts(value: &str, morphology: &Morphology) -> TermCounts {
    informative_tokens(value)
        .map(|term| search_key(&term, morphology))
        .collect()
}

pub(super) fn informative_terms(value: &str, morphology: &Morphology) -> BTreeSet<String> {
    informative_keys(value, morphology).collect()
}

/// The search keys of a text under one morphology, in order and with
/// repeats, so a caller stemming two fields in two languages can chain them
/// into one term bag.
pub(super) fn informative_keys<'a>(
    value: &'a str,
    morphology: &'a Morphology,
) -> impl Iterator<Item = String> + 'a {
    informative_tokens(value).map(move |term| search_key(&term, morphology))
}

/// The form two words are compared in.
///
/// The hand-kept concept table speaks first, so every family it already
/// unifies keeps behaving exactly as it did. Morphology only reaches the words
/// the table has nothing to say about, which is almost all of them and all of
/// the Spanish.
pub(super) fn search_key(term: &str, morphology: &Morphology) -> String {
    let concept = concept_key(term);
    if concept != term {
        return concept.to_string();
    }
    morphology.stem(term).into_owned()
}

pub(super) fn terms_match(left: &str, right: &str) -> bool {
    concept_key(left) == concept_key(right)
}

/// Stable semantic buckets for the small set of paraphrases the deterministic
/// ranker intentionally understands. Counting buckets rather than raw words
/// prevents a question containing two synonyms from earning two matches from
/// one evidence term.
pub(super) fn concept_key(term: &str) -> &str {
    match term {
        "query" | "recall" | "retrieval" | "retrieve" => "concept:recall",
        "accept" | "accepted" | "acceptance" => "concept:acceptance",
        "correct" | "corrected" | "correction" | "fix" | "fixed" => "concept:correction",
        "remain" | "remains" | "remaining" | "still" => "concept:currentness",
        "destination" | "move" | "moved" | "moves" | "moving" | "relocate" | "relocated"
        | "relocates" | "relocating" => "concept:movement",
        "replace" | "replaced" | "replaces" | "replacing" | "supersede" | "superseded"
        | "supersedes" => "concept:lifecycle",
        "backend" | "engine" | "sqlite" => "concept:storage-engine",
        "data" | "directory" | "store" | "stores" | "storage" => "concept:store",
        "build" | "builds" | "built" | "create" | "created" | "fresh" | "install"
        | "installation" | "installed" | "new" | "reinstall" | "reinstalled" => {
            "concept:installation"
        }
        "restart" | "restarted" | "restarting" => "concept:restart",
        "require" | "required" | "requires" => "concept:requirement",
        "check" | "checked" | "validate" | "validated" | "validation" => "concept:validation",
        "rank" | "ranked" | "ranking" | "relevance" => "concept:ranking",
        "old" | "older" | "previous" | "prior" | "stale" => "concept:historical",
        "default" | "select" | "selected" | "selection" => "concept:selection",
        "existing" | "present" | "preserve" | "preserved" | "preserving" => "concept:presence",
        _ => term,
    }
}

/// Whether a question is asking about history rather than about now.
///
/// Its vocabulary is compared in the same form the question arrives in.
/// Terms reach the ranker already folded onto their concept key or their
/// stem, so a list of bare words would no longer recognise `previous` — the
/// concept table renames it before this ever sees it.
pub(super) fn query_requests_lifecycle(
    question_terms: &BTreeSet<String>,
    morphology: &Morphology,
) -> bool {
    const LIFECYCLE_QUERY_TERMS: &[&str] = &[
        "before",
        "former",
        "old",
        "previous",
        "replace",
        "replaced",
        "replaces",
        "replacing",
        "supersede",
        "superseded",
        "supersedes",
    ];
    let asked_for = LIFECYCLE_QUERY_TERMS
        .iter()
        .map(|term| search_key(term, morphology))
        .collect::<BTreeSet<_>>();
    question_terms.iter().any(|term| asked_for.contains(term))
}
