use std::collections::{BTreeMap, BTreeSet};

use super::association_index::AssociationIndex;
use super::bridged_key::BridgedKey;
use super::indexed_field_stats::IndexedFieldStats;
use super::lexical_bridge::LexicalBridge;
use super::lexical_field::LexicalField;
use super::lexical_row::LexicalRow;
use super::morphology::Morphology;
use super::question_contract::QuestionContract;
use super::search_terms::{informative_term_counts, informative_terms};
use super::term_counts::TermCounts;

/// What the lexical index must reach for one question (DESIGN L6, P13).
///
/// An ask weighs the words of the question as the ranker reads it (as asked,
/// and under the anchored gate without its negated stretches and with its
/// alias terms, or without its facets), and the store's own associations of
/// those words. Every candidate that can score above zero carries one of
/// them in its content or direct field, so the postings of all of them hold
/// every candidate the ask can rank; outside them none can clear the floor.
pub struct IndexedQuestion {
    forms: Vec<String>,
    morphology: Morphology,
}

impl IndexedQuestion {
    pub fn read(question: &str, language: Option<&str>) -> Self {
        let morphology = Morphology::for_language(language);
        let contract = QuestionContract::read(question, &morphology);
        let mut forms = vec![question.to_string()];
        for form in [contract.asked(), contract.anchored_asked()]
            .into_iter()
            .flatten()
        {
            if !forms.iter().any(|known| known == form) {
                forms.push(form.to_string());
            }
        }
        Self { forms, morphology }
    }

    /// The words of every form the ranker may read the question in.
    pub fn terms(&self) -> BTreeSet<String> {
        self.forms
            .iter()
            .flat_map(|form| informative_terms(form, &self.morphology))
            .collect()
    }

    /// The words the table bridges the question's words to in `vocabulary`,
    /// the whole about's (at most three per word): candidates carrying them
    /// can clear the floor through the bridge, so their postings are read.
    pub fn bridged(&self, bridge: &LexicalBridge, vocabulary: &[String]) -> BTreeSet<String> {
        self.forms
            .iter()
            .flat_map(|form| BridgedKey::read_words(form, &self.morphology, vocabulary, bridge))
            .map(|pair| pair.candidate_key)
            .collect()
    }

    /// The words the store associates with those of the question under one
    /// reading: what [`AssociationIndex::for_question`] finds over
    /// `documents`, which must hold every candidate that carries a word of
    /// the question, with `stats` the whole about's direct field.
    pub fn associations(
        &self,
        stats: &IndexedFieldStats,
        aliased: bool,
        documents: &[LexicalRow],
    ) -> BTreeSet<String> {
        let field =
            LexicalField::from_stats(stats.documents, stats.direct_length, &stats.direct_df);
        let counts = documents
            .iter()
            .map(|row| {
                TermCounts::from_counts(
                    row.terms()
                        .iter()
                        .filter(|term| term.direct(aliased) > 0)
                        .map(|term| (term.term.clone(), term.direct(aliased) as u32))
                        .collect::<BTreeMap<_, _>>(),
                    row.direct_length(aliased).max(0) as usize,
                )
            })
            .collect::<Vec<_>>();
        let mut associated = BTreeSet::new();
        for form in &self.forms {
            let question = informative_term_counts(form, &self.morphology);
            associated.extend(
                AssociationIndex::for_question(&question, &field, counts.iter())
                    .expand(&question)
                    .into_keys(),
            );
        }
        associated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1beta1::memory_mapping::lexical_bridge::tests::spanish_english_toy;

    #[test]
    fn the_words_of_every_form_are_what_the_postings_are_read_for() {
        let question = IndexedQuestion::read("which valve froze during the night?", None);
        let terms = question.terms();
        for word in ["valve", "froze", "night"] {
            assert!(terms.contains(word), "{terms:?}");
        }
        assert!(!terms.contains("the"));
    }

    #[test]
    fn the_bridge_reaches_the_words_of_the_whole_abouts_vocabulary() {
        let question = IndexedQuestion::read("valvula noche", None);
        let vocabulary = ["canteen", "night", "valve"].map(String::from);
        let bridged = question.bridged(&spanish_english_toy(), &vocabulary);
        assert!(
            bridged.contains("valve") && bridged.contains("night"),
            "{bridged:?}"
        );
        assert!(!bridged.contains("canteen"));
        // Without a table, or with none of its words in the about, nothing.
        assert!(
            question
                .bridged(&LexicalBridge::none(), &vocabulary)
                .is_empty()
        );
        assert!(question.bridged(&spanish_english_toy(), &[]).is_empty());
    }
}
