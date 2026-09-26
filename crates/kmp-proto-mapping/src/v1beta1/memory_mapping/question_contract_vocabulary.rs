use std::collections::BTreeSet;
use std::sync::OnceLock;

use serde::Deserialize;

use super::identifier_aliases::IdentifierAliases;

/// The words a question's contract is read with: what negates, what names a
/// unit or a kind of identifier, what only asks whether something exists and
/// what carries no subject.
///
/// Data, like the relation families: `language/question_contract.json` is
/// reviewable without knowing Rust. Only the anchored ask gate reads it, so a
/// store that did not opt into the gate answers exactly as it did.
#[derive(Debug, Deserialize)]
pub(super) struct QuestionContractVocabulary {
    /// Single words and two-word phrases (`fuera de`) that exclude what
    /// follows them in their clause.
    negations: BTreeSet<String>,
    /// Words that end a negated stretch the way punctuation does.
    clause_breaks: BTreeSet<String>,
    coordinations: BTreeSet<String>,
    /// Words that say the number after them is an identifier: `issue 18`.
    guide_words: BTreeSet<String>,
    /// Words that say the number before them is a quantity: `30 seconds`.
    unit_words: BTreeSet<String>,
    /// The same, written onto the number: `30s`, `80%`.
    unit_suffixes: BTreeSet<String>,
    /// Words a quantity range is written with: `from 300 to 30 seconds`.
    range_connectors: BTreeSet<String>,
    existence_predicates: BTreeSet<String>,
    generic_predicates: BTreeSet<String>,
    filler_words: BTreeSet<String>,
    state_words: BTreeSet<String>,
    /// An anchor carried by more than this share of the admitted candidates
    /// names a hub rather than a subject.
    hub_document_share: f64,
    /// ... and by more than this many of them: in a small selection every
    /// anchor is a large share of it.
    hub_min_documents: usize,
    /// The one term each spelling of an introduced identifier reads as.
    identifier_aliases: IdentifierAliases,
}

const SOURCE: &str = include_str!("../../../language/question_contract.json");

impl QuestionContractVocabulary {
    /// The shipped vocabulary, parsed once and compiled in, like the
    /// relation families.
    pub(super) fn shipped() -> &'static Self {
        static SHIPPED: OnceLock<QuestionContractVocabulary> = OnceLock::new();
        SHIPPED.get_or_init(|| {
            serde_json::from_str(SOURCE).expect("the shipped question contract vocabulary parses")
        })
    }

    /// Whether the word at `index` opens a negation, alone or with the word
    /// after it. Returns how many words the negation spans.
    pub(super) fn negation_at(&self, words: &[String], index: usize) -> Option<usize> {
        if self.negations.contains(&words[index]) {
            return Some(1);
        }
        let pair = format!("{} {}", words[index], words.get(index + 1)?);
        self.negations.contains(&pair).then_some(2)
    }

    pub(super) fn breaks_clause(&self, word: &str) -> bool {
        self.clause_breaks.contains(word)
    }

    pub(super) fn is_coordination(&self, word: &str) -> bool {
        self.coordinations.contains(word)
    }

    pub(super) fn is_guide_word(&self, word: &str) -> bool {
        self.guide_words.contains(word)
    }

    /// How `corte 10`, `C10` and `ADR-018` read as one term each.
    pub(super) fn identifier_aliases(&self) -> &IdentifierAliases {
        &self.identifier_aliases
    }

    pub(super) fn is_unit(&self, word: &str) -> bool {
        self.unit_words.contains(word)
    }

    /// Whether `token` is a number with a unit written onto it (`30s`).
    pub(super) fn is_quantity(&self, token: &str) -> bool {
        let digits = token
            .char_indices()
            .find(|(_, character)| !character.is_ascii_digit())
            .map_or(token.len(), |(index, _)| index);
        digits > 0 && self.unit_suffixes.contains(&token[digits..])
    }

    pub(super) fn connects_range(&self, word: &str) -> bool {
        self.range_connectors.contains(word)
    }

    pub(super) fn asks_existence(&self, word: &str) -> bool {
        self.existence_predicates.contains(word)
    }

    /// A word that carries no subject of its own: a generic predicate, a
    /// filler, a guide word, an existence predicate or a word about time.
    pub(super) fn carries_no_subject(&self, word: &str) -> bool {
        self.generic_predicates.contains(word)
            || self.filler_words.contains(word)
            || self.guide_words.contains(word)
            || self.existence_predicates.contains(word)
            || self.state_words.contains(word)
    }

    pub(super) fn asks_state(&self, word: &str) -> bool {
        self.state_words.contains(word)
    }

    /// Whether an anchor carried by `document_frequency` of `documents`
    /// admitted candidates names a hub.
    pub(super) fn is_hub(&self, document_frequency: usize, documents: usize) -> bool {
        document_frequency > self.hub_min_documents
            && document_frequency as f64 > self.hub_document_share * documents as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn the_shipped_vocabulary_parses_with_folded_words() {
        let vocabulary = QuestionContractVocabulary::shipped();
        for list in [
            &vocabulary.negations,
            &vocabulary.guide_words,
            &vocabulary.unit_words,
            &vocabulary.existence_predicates,
            &vocabulary.filler_words,
        ] {
            for word in list {
                assert_eq!(
                    word,
                    &super::super::search_terms::fold_search_term(word),
                    "`{word}` is not folded"
                );
            }
        }
    }

    #[test]
    fn every_word_that_introduces_an_alias_is_a_guide_word() {
        let vocabulary = QuestionContractVocabulary::shipped();
        for word in [
            "corte", "cut", "fase", "phase", "adr", "inc", "issue", "pr", "release",
        ] {
            assert!(vocabulary.identifier_aliases().introduces(word), "{word}");
        }
        let source: serde_json::Value = serde_json::from_str(SOURCE).expect("json");
        for alias in source["identifier_aliases"].as_array().expect("aliases") {
            for word in alias["guide_words"].as_array().expect("words") {
                let word = word.as_str().expect("a word");
                assert!(
                    vocabulary.is_guide_word(word),
                    "`{word}` introduces an alias"
                );
                assert_eq!(word, super::super::search_terms::fold_search_term(word));
            }
        }
    }

    #[test]
    fn a_negation_spans_one_word_or_a_phrase() {
        let vocabulary = QuestionContractVocabulary::shipped();

        assert_eq!(vocabulary.negation_at(&words("excluding c7"), 0), Some(1));
        assert_eq!(vocabulary.negation_at(&words("fuera de c7"), 0), Some(2));
        assert_eq!(vocabulary.negation_at(&words("fuera"), 0), None);
        assert_eq!(vocabulary.negation_at(&words("scope of c7"), 0), None);
    }

    #[test]
    fn quantities_and_hubs_are_read_from_the_data() {
        let vocabulary = QuestionContractVocabulary::shipped();

        assert!(vocabulary.is_quantity("30s"));
        assert!(vocabulary.is_quantity("80%"));
        assert!(!vocabulary.is_quantity("c6"));
        assert!(!vocabulary.is_quantity("30"));
        assert!(vocabulary.is_unit("seconds"));
        assert!(vocabulary.connects_range("to"));
        assert!(vocabulary.is_guide_word("issue"));
        assert!(vocabulary.is_coordination("and"));
        assert!(vocabulary.breaks_clause("but"));
        assert!(vocabulary.asks_existence("stored"));
        assert!(vocabulary.asks_state("current"));
        assert!(vocabulary.carries_no_subject("exact"));
        assert!(!vocabulary.carries_no_subject("database"));

        // Thirty-one candidates of a hundred is a hub; eleven of a hundred,
        // fifty of four hundred or two of twelve is not, however large a
        // share of so small a selection it is: an identifier that `corte 4`,
        // `C4` and a `corte4` branch all name is still one subject.
        assert!(vocabulary.is_hub(31, 100));
        assert!(!vocabulary.is_hub(11, 100));
        assert!(!vocabulary.is_hub(50, 400));
        assert!(!vocabulary.is_hub(2, 12));
        assert!(!vocabulary.is_hub(31, 1000));
    }
}
