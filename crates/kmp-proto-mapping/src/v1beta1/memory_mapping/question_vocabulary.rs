use std::collections::BTreeSet;
use std::sync::OnceLock;

use kmp_domain::RelationSemanticClass;
use serde::Deserialize;

/// The words that ask for a kind of connection, and the stored vocabulary that
/// answers it.
///
/// This used to be three `match` arms and two arrays of string literals, which
/// meant that teaching KMP one more way of asking *why* was a code change and a
/// release. It is data now: `language/question_families.json` is reviewable on
/// its own terms, and a reader who knows the domain can extend it without
/// knowing Rust.
#[derive(Debug, Deserialize)]
pub(super) struct QuestionVocabulary {
    families: Vec<QuestionFamily>,
}

#[derive(Debug, Deserialize)]
struct QuestionFamily {
    id: String,
    semantic_classes: Vec<String>,
    relation_types: BTreeSet<String>,
    /// Written folded and without diacritics, because that is the form a
    /// question reaches the reader in.
    tokens: BTreeSet<String>,
    /// For the verb families a list of exact forms cannot enumerate.
    token_prefixes: Vec<String>,
}

const SOURCE: &str = include_str!("../../../language/question_families.json");

impl QuestionVocabulary {
    /// The shipped vocabulary, parsed once.
    ///
    /// It is compiled in rather than read from disk: retrieval must not gain a
    /// missing-file path, and a kernel that answers identically on every run
    /// cannot depend on what is beside it.
    pub(super) fn shipped() -> &'static Self {
        static SHIPPED: OnceLock<QuestionVocabulary> = OnceLock::new();
        SHIPPED.get_or_init(|| {
            serde_json::from_str(SOURCE).expect("the shipped question vocabulary parses")
        })
    }

    /// The family a single word asks for, if any.
    pub(super) fn family_of(&self, token: &str) -> Option<&str> {
        self.families
            .iter()
            .find(|family| family.tokens.contains(token))
            .or_else(|| {
                self.families.iter().find(|family| {
                    family
                        .token_prefixes
                        .iter()
                        .any(|prefix| token.starts_with(prefix.as_str()))
                })
            })
            .map(|family| family.id.as_str())
    }

    /// Whether a stored relation answers what one of these families asked for.
    ///
    /// Either half is enough: the exact stored type, or its semantic class. A
    /// memory written with `triggers` and one written with an extension type of
    /// the same causal class both answer *why*.
    pub(super) fn answers(
        &self,
        families: &BTreeSet<String>,
        relation_type: &str,
        class: &RelationSemanticClass,
    ) -> bool {
        self.families
            .iter()
            .filter(|family| families.contains(&family.id))
            .any(|family| {
                family.relation_types.contains(relation_type)
                    || family
                        .semantic_classes
                        .iter()
                        .any(|named| named == class.as_str())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_vocabulary_parses_and_names_every_family_once() {
        let vocabulary = QuestionVocabulary::shipped();
        let ids = vocabulary
            .families
            .iter()
            .map(|family| family.id.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(ids.len(), vocabulary.families.len());
        assert!(ids.contains("why"));
    }

    /// A class named in the data must be a class the domain has, or the family
    /// would quietly answer nothing.
    #[test]
    fn every_named_semantic_class_exists() {
        for family in &QuestionVocabulary::shipped().families {
            for class in &family.semantic_classes {
                RelationSemanticClass::parse(class).unwrap_or_else(|_| {
                    panic!("family `{}` names unknown class `{class}`", family.id)
                });
            }
        }
    }

    #[test]
    fn a_word_reaches_its_family_by_exact_form_or_by_prefix() {
        let vocabulary = QuestionVocabulary::shipped();

        assert_eq!(vocabulary.family_of("why"), Some("why"));
        assert_eq!(vocabulary.family_of("reemplazamos"), Some("lifecycle"));
        assert_eq!(vocabulary.family_of("tuesday"), None);
    }

    #[test]
    fn a_family_answers_by_stored_type_or_by_class() {
        let vocabulary = QuestionVocabulary::shipped();
        let why = BTreeSet::from(["why".to_string()]);

        assert!(vocabulary.answers(&why, "chosen_because", &RelationSemanticClass::Motivational));
        assert!(vocabulary.answers(&why, "an_extension", &RelationSemanticClass::Causal));
        assert!(!vocabulary.answers(&why, "contains_entry", &RelationSemanticClass::Structural));
        assert!(!vocabulary.answers(&BTreeSet::new(), "triggers", &RelationSemanticClass::Causal));
    }
}
