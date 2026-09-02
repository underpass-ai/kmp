use std::collections::BTreeSet;

use super::answer_candidate_terms::AnswerCandidateTerms;
use super::bridged_term::BridgedTerm;
use super::lexical_bridge::LexicalBridge;
use super::morphology::Morphology;
use super::search_terms::{concept_key, informative_tokens, matching_terms, search_key};
use super::term_counts::TermCounts;

/// A bridged pair, carried with the search keys both of its words rank under.
///
/// The table speaks in folded surface words; the ranker counts concepts and
/// stems. A pair is only useful once both sides are named the way the
/// candidate's term counts name them.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BridgedKey {
    pub(super) term: BridgedTerm,
    pub(super) question_key: String,
    pub(super) candidate_key: String,
}

impl BridgedKey {
    /// Bridges a question to the words the candidates actually use.
    ///
    /// The vocabulary is the candidates', never the table's: a question is
    /// compared against the few hundred words that live in this
    /// neighbourhood, which is what keeps the cost proportional to the memory
    /// and makes a bridge as local as the dimension the load was narrowed to.
    pub(super) fn read<'a>(
        question: &str,
        morphology: &Morphology,
        candidate_texts: impl IntoIterator<Item = &'a str>,
        bridge: &LexicalBridge,
    ) -> Vec<Self> {
        if bridge.is_silent() {
            return Vec::new();
        }
        let question_words = informative_tokens(question).collect::<Vec<_>>();
        let vocabulary = candidate_texts
            .into_iter()
            .flat_map(informative_tokens)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        bridge
            .bridge(&question_words, &vocabulary)
            .into_iter()
            .map(|term| Self {
                question_key: search_key(&term.question, morphology),
                candidate_key: search_key(&term.candidate, morphology),
                term,
            })
            // Two words that fold onto one key are an exact match already,
            // and BM25 has weighed it.
            .filter(|bridged| bridged.question_key != bridged.candidate_key)
            .collect()
    }

    /// The pairs that land on this candidate.
    pub(super) fn landing_on<'a>(
        bridged: &'a [Self],
        terms: &AnswerCandidateTerms,
    ) -> Vec<&'a BridgedTerm> {
        bridged
            .iter()
            .filter(|pair| terms.direct_counts.count(&pair.candidate_key) > 0)
            .map(|pair| &pair.term)
            .collect()
    }

    /// The share of the question's concepts a candidate covers, in its own
    /// words or through the table.
    ///
    /// A bridged concept counts as covered, not as a fraction of one: the
    /// pair already paid for its uncertainty at the bar it had to clear, and
    /// charging it again would let one known miss — `almacén` does not reach
    /// `store` — sink an answer that bridged everything else.
    ///
    /// This is the second signal abstention has been waiting for. Exact
    /// coverage says whether the candidate shares the reader's words; this
    /// says whether it shares their meaning by the table's lights. The two
    /// disagreeing — no words, most of the meaning — is precisely "found it,
    /// in other words", and it is reported as such rather than as UNKNOWN.
    pub(super) fn coverage(
        bridged: &[Self],
        question: &TermCounts,
        terms: &AnswerCandidateTerms,
    ) -> f64 {
        let concepts = question.terms().collect::<Vec<_>>();
        if concepts.is_empty() {
            return 0.0;
        }
        let covered = concepts
            .iter()
            .filter(|concept| {
                terms.direct_counts.count(concept) > 0
                    || bridged.iter().any(|pair| {
                        &pair.question_key == **concept
                            && terms.direct_counts.count(&pair.candidate_key) > 0
                    })
            })
            .count();
        covered as f64 / concepts.len() as f64
    }

    /// How many of the focus concepts a candidate answers, in its own words
    /// or in the table's. A strict policy asks for most of what the question
    /// is about; a memory in another language is about the same thing.
    pub(super) fn focus_matches(
        bridged: &[Self],
        focus_terms: &BTreeSet<String>,
        terms: &AnswerCandidateTerms,
    ) -> usize {
        let mut covered = matching_terms(focus_terms, &terms.searchable)
            .iter()
            .map(|term| concept_key(term).to_string())
            .collect::<BTreeSet<_>>();
        for pair in bridged {
            if focus_terms.contains(&pair.question_key)
                && terms.direct_counts.count(&pair.candidate_key) > 0
            {
                covered.insert(concept_key(&pair.question_key).to_string());
            }
        }
        covered.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1beta1::memory_mapping::answer_recall_context::AnswerRecallContext;
    use crate::v1beta1::memory_mapping::lexical_bridge::tests::spanish_english_toy;
    use crate::v1beta1::memory_mapping::search_terms::informative_term_counts;
    use kmp_proto::v1beta1::MemoryEvidence;

    fn terms_of(text: &str) -> AnswerCandidateTerms {
        let item = MemoryEvidence {
            text: text.to_string(),
            ..Default::default()
        };
        AnswerCandidateTerms::from_evidence(&item, &AnswerRecallContext::default())
    }

    #[test]
    fn a_pair_carries_the_keys_the_ranker_counts_under() {
        let morphology = Morphology::read([
            "The reserve valve failed during the night shift.",
            "The weekly meeting moved to ten in the morning.",
        ]);
        let bridged = BridgedKey::read(
            "Que valvula fallo durante la noche?",
            &morphology,
            ["The reserve valve failed during the night shift."],
            &spanish_english_toy(),
        );

        let keys = bridged
            .iter()
            .map(|pair| (pair.question_key.as_str(), pair.candidate_key.as_str()))
            .collect::<Vec<_>>();
        // The store reads as English, so its stemmer folds both sides: the
        // Spanish question word loses its vowel the same way a candidate would.
        assert_eq!(keys, vec![("valvula", "valv"), ("noch", "night")]);
    }

    #[test]
    fn coverage_counts_a_bridged_concept_as_covered_and_an_absent_one_as_not() {
        let morphology = Morphology::none();
        let candidate = "The valve failed during the night";
        let bridged = BridgedKey::read(
            "valvula night canteen",
            &morphology,
            [candidate],
            &spanish_english_toy(),
        );
        let question = informative_term_counts("valvula night canteen", &morphology);
        let terms = terms_of(candidate);

        // `valvula` bridged, `night` said outright, `canteen` nowhere.
        let coverage = BridgedKey::coverage(&bridged, &question, &terms);
        assert!((coverage - 2.0 / 3.0).abs() < 1e-9, "{coverage}");
        assert_eq!(
            BridgedKey::coverage(&bridged, &TermCounts::default(), &terms),
            0.0
        );
    }

    #[test]
    fn focus_matches_count_a_concept_once_however_it_was_reached() {
        let morphology = Morphology::none();
        let candidate = "The valve failed during the night";
        let bridged = BridgedKey::read(
            "valvula night",
            &morphology,
            [candidate],
            &spanish_english_toy(),
        );
        let focus = ["valvula", "night", "canteen"]
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>();

        assert_eq!(
            BridgedKey::focus_matches(&bridged, &focus, &terms_of(candidate)),
            2
        );
    }

    #[test]
    fn without_a_table_nothing_is_bridged() {
        assert!(
            BridgedKey::read(
                "valvula",
                &Morphology::none(),
                ["valve"],
                &LexicalBridge::none()
            )
            .is_empty()
        );
    }
}
