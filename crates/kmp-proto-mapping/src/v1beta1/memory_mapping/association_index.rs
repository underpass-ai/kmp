use std::collections::BTreeMap;

use super::lexical_field::LexicalField;
use super::term_counts::TermCounts;

/// What this memory has learned that its own words mean together.
///
/// The hand-kept concept table asserts that `backend`, `engine` and `sqlite`
/// are one concept. That is true here and wrong in a memory comparing engines,
/// and it is a domain opinion living inside a generic kernel. Co-occurrence
/// says the same kind of thing without anyone having to type it: inside a store
/// where `cache` and `valkey` keep appearing in the same entry, they are about
/// each other, and a question naming one can reach a memory that only names the
/// other.
///
/// What it cannot do is the reason it is not the whole answer. It knows nothing
/// the store has never said, it cannot align two languages that never share an
/// entry, and it needs enough entries to be measuring an association rather
/// than a coincidence. Below that, it declines — the same way `Morphology`
/// declines to stem a memory whose language it cannot read.
#[derive(Debug, Default)]
pub(super) struct AssociationIndex {
    neighbours: BTreeMap<String, Vec<(String, f64)>>,
}

/// Fewer documents than this and co-occurrence is anecdote. KMP stores hold
/// decisions and evidence rather than transcripts, so this floor is reached
/// late and declining is the common case, not the exception.
const MINIMUM_DOCUMENTS: usize = 12;
/// A pair seen fewer times than this is not a pattern.
const MINIMUM_PAIR_COUNT: usize = 3;
/// Pointwise mutual information above zero means the two words appear together
/// more than chance would explain; the bar sits above zero so that "more than
/// chance" has to be a margin rather than a rounding.
const MINIMUM_ASSOCIATION: f64 = 0.5;
/// How many neighbours one term may bring, so a common word cannot widen a
/// question indefinitely.
const MAX_NEIGHBOURS: usize = 3;
/// What an expanded term is worth against a word the reader actually wrote.
/// It is a hint about this store's vocabulary, not a term of the question.
const EXPANSION_WEIGHT: f64 = 0.35;
/// How far below `MINIMUM_ASSOCIATION` the PMI bound ln(N/df) must fall before
/// a question term is skipped uncounted. Far wider than the few ulps either
/// logarithm can drift, far narrower than any bound that matters.
const BOUND_MARGIN: f64 = 1e-9;

impl AssociationIndex {
    /// Every term's neighbours, from every pair of every document.
    ///
    /// Θ(Σ L_d²) over the documents' distinct terms. Retrieval asks only for
    /// the question's own terms (`for_question`); this whole-store form stays
    /// as the oracle that the restricted one must reproduce bit for bit.
    #[cfg(test)]
    pub(super) fn build<'a>(documents: impl IntoIterator<Item = &'a TermCounts>) -> Self {
        let documents = documents.into_iter().collect::<Vec<_>>();
        if documents.len() < MINIMUM_DOCUMENTS {
            return Self::default();
        }
        let total = documents.len() as f64;

        let mut occurrences = BTreeMap::<&str, usize>::new();
        let mut pairs = BTreeMap::<(&str, &str), usize>::new();
        for document in &documents {
            let terms = document.terms().collect::<Vec<_>>();
            for (index, left) in terms.iter().enumerate() {
                *occurrences.entry(left.as_str()).or_default() += 1;
                for right in terms.iter().skip(index + 1) {
                    *pairs.entry((left.as_str(), right.as_str())).or_default() += 1;
                }
            }
        }

        let mut neighbours = BTreeMap::<String, Vec<(String, f64)>>::new();
        for ((left, right), together) in pairs {
            if together < MINIMUM_PAIR_COUNT {
                continue;
            }
            let (left_count, right_count) = (
                occurrences.get(left).copied().unwrap_or_default() as f64,
                occurrences.get(right).copied().unwrap_or_default() as f64,
            );
            if left_count <= 0.0 || right_count <= 0.0 {
                continue;
            }
            let association = association(together, left_count, right_count, total);
            if association < MINIMUM_ASSOCIATION {
                continue;
            }
            neighbours
                .entry(left.to_string())
                .or_default()
                .push((right.to_string(), association));
            neighbours
                .entry(right.to_string())
                .or_default()
                .push((left.to_string(), association));
        }

        for entries in neighbours.values_mut() {
            strongest_first(entries);
        }
        Self { neighbours }
    }

    /// The neighbours of the question's own terms only, exactly as `build`
    /// would find them.
    ///
    /// `expand` reads nothing but the neighbours of the terms the reader
    /// wrote, and a term's neighbours depend only on the pairs it is part of:
    /// its count with each co-occurring word, both document frequencies and
    /// the document total. Counting the documents that contain a question
    /// term costs O(df(t)·L̄) instead of every pair in the store.
    ///
    /// A term is skipped before counting when no partner could clear the bar:
    /// with c(t,x) ≤ df(x), PMI(t,x) = ln(c·N / (df(t)·df(x))) ≤ ln(N/df(t)).
    /// The margin keeps a rounding in either computation from skipping a
    /// term the whole-store build would keep; a term below it is below the
    /// bar by far more than a float can drift.
    ///
    /// `field` must be the direct field measured over exactly `documents`:
    /// its document count and frequencies are the N and df of the pairs.
    pub(super) fn for_question<'a>(
        question: &TermCounts,
        field: &LexicalField,
        documents: impl IntoIterator<Item = &'a TermCounts>,
    ) -> Self {
        if field.documents() < MINIMUM_DOCUMENTS {
            return Self::default();
        }
        let total = field.documents() as f64;
        let asked = question
            .terms()
            .map(String::as_str)
            .filter(|term| {
                let frequency = field.document_frequency(term);
                frequency >= MINIMUM_PAIR_COUNT
                    && (total / frequency as f64).ln() >= MINIMUM_ASSOCIATION - BOUND_MARGIN
            })
            .collect::<Vec<_>>();
        if asked.is_empty() {
            return Self::default();
        }

        let mut together = asked
            .iter()
            .map(|term| (*term, BTreeMap::<&str, usize>::new()))
            .collect::<BTreeMap<_, _>>();
        for document in documents {
            for (term, partners) in &mut together {
                if document.count(term) == 0 {
                    continue;
                }
                for other in document.terms() {
                    if other != term {
                        *partners.entry(other.as_str()).or_default() += 1;
                    }
                }
            }
        }

        let mut neighbours = BTreeMap::<String, Vec<(String, f64)>>::new();
        for (term, partners) in together {
            let mut entries = Vec::new();
            for (other, count) in partners {
                if count < MINIMUM_PAIR_COUNT {
                    continue;
                }
                // The whole-store build sees the pair with its terms in
                // lexical order; keep that order so the float is the same.
                let (left, right) = if term < other {
                    (term, other)
                } else {
                    (other, term)
                };
                let (left_count, right_count) = (
                    field.document_frequency(left) as f64,
                    field.document_frequency(right) as f64,
                );
                if left_count <= 0.0 || right_count <= 0.0 {
                    continue;
                }
                let association = association(count, left_count, right_count, total);
                if association < MINIMUM_ASSOCIATION {
                    continue;
                }
                entries.push((other.to_string(), association));
            }
            if !entries.is_empty() {
                strongest_first(&mut entries);
                neighbours.insert(term.to_string(), entries);
            }
        }
        Self { neighbours }
    }

    /// One term's neighbours, strongest first; only the tests ask.
    #[cfg(test)]
    pub(super) fn neighbours_of(&self, term: &str) -> Option<&[(String, f64)]> {
        self.neighbours.get(term).map(Vec::as_slice)
    }

    /// The terms that have neighbours; only the tests ask.
    #[cfg(test)]
    pub(super) fn terms(&self) -> impl Iterator<Item = &String> {
        self.neighbours.keys()
    }

    /// Whether this memory said enough for its vocabulary to mean anything.
    /// What it gates is visible in what gets expanded, so only the tests ask.
    #[cfg(test)]
    fn is_silent(&self) -> bool {
        self.neighbours.is_empty()
    }

    /// What the question asked for, plus what this memory says goes with it.
    ///
    /// A term the reader wrote is worth its full weight; one this store
    /// associated with it is worth a fraction, and never displaces the words
    /// actually typed.
    pub(super) fn expand(&self, question: &TermCounts) -> BTreeMap<String, f64> {
        let mut weights = question
            .terms()
            .map(|term| (term.clone(), 1.0))
            .collect::<BTreeMap<_, _>>();
        for term in question.terms() {
            for (neighbour, _) in self.neighbours.get(term).into_iter().flatten() {
                weights.entry(neighbour.clone()).or_insert(EXPANSION_WEIGHT);
            }
        }
        weights
    }
}

/// Pointwise mutual information: how much more often these two appear
/// together than two unrelated words of the same frequency would. One
/// expression for both builders, so their floats are the same bits.
fn association(together: usize, left_count: f64, right_count: f64, total: f64) -> f64 {
    ((together as f64 / total) / ((left_count / total) * (right_count / total))).ln()
}

/// Strongest association first, ties by word, at most `MAX_NEIGHBOURS`.
fn strongest_first(entries: &mut Vec<(String, f64)>) {
    entries.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    entries.truncate(MAX_NEIGHBOURS);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(terms: &[&str]) -> TermCounts {
        terms.iter().map(|term| (*term).to_string()).collect()
    }

    fn store_where_cache_means_valkey() -> Vec<TermCounts> {
        let mut documents = (0..6)
            .map(|_| counts(&["cache", "valkey", "staging"]))
            .collect::<Vec<_>>();
        documents.extend((0..8).map(|index| match index % 2 {
            0 => counts(&["meeting", "agenda"]),
            _ => counts(&["invoice", "supplier"]),
        }));
        documents
    }

    #[test]
    fn a_memory_learns_which_of_its_own_words_go_together() {
        let index = AssociationIndex::build(store_where_cache_means_valkey().iter());
        let expanded = index.expand(&counts(&["cache"]));

        assert!(!index.is_silent());
        assert_eq!(expanded.get("cache"), Some(&1.0));
        assert_eq!(expanded.get("valkey"), Some(&EXPANSION_WEIGHT));
    }

    #[test]
    fn a_word_the_store_never_paired_brings_nothing() {
        let index = AssociationIndex::build(store_where_cache_means_valkey().iter());
        let expanded = index.expand(&counts(&["supplier"]));

        assert_eq!(expanded.get("supplier"), Some(&1.0));
        assert!(!expanded.contains_key("valkey"));
    }

    /// The cold start is the honest common case for a store of decisions.
    #[test]
    fn a_memory_too_small_to_have_a_pattern_declines() {
        let index = AssociationIndex::build(
            [
                counts(&["cache", "valkey"]),
                counts(&["cache", "valkey"]),
                counts(&["cache", "valkey"]),
            ]
            .iter(),
        );

        assert!(index.is_silent());
        assert_eq!(index.expand(&counts(&["cache"])).len(), 1);
    }

    #[test]
    fn a_pair_seen_once_is_a_coincidence_and_not_a_meaning() {
        let mut documents = (0..14)
            .map(|index| counts(&[if index % 2 == 0 { "meeting" } else { "invoice" }, "note"]))
            .collect::<Vec<_>>();
        documents.push(counts(&["cache", "valkey"]));
        let index = AssociationIndex::build(documents.iter());

        assert!(!index.expand(&counts(&["cache"])).contains_key("valkey"));
    }

    #[test]
    fn one_term_cannot_widen_a_question_without_limit() {
        let documents = (0..14)
            .map(|_| counts(&["hub", "one", "two", "three", "four", "five"]))
            .collect::<Vec<_>>();
        let index = AssociationIndex::build(documents.iter());

        let expanded = index.expand(&counts(&["hub"]));
        assert!(expanded.len() <= 1 + MAX_NEIGHBOURS);
    }

    #[test]
    fn a_term_the_reader_wrote_keeps_its_full_weight_even_when_also_expanded() {
        let index = AssociationIndex::build(store_where_cache_means_valkey().iter());
        let expanded = index.expand(&counts(&["cache", "valkey"]));

        assert_eq!(expanded.get("cache"), Some(&1.0));
        assert_eq!(expanded.get("valkey"), Some(&1.0));
    }
}
