use std::collections::BTreeSet;

use kmp_domain::RelationSemanticClass;

use super::question_vocabulary::QuestionVocabulary;
use super::search_terms::fold_search_term;

/// What kind of connection one question is asking about.
///
/// KMP stores thirty-one relation types, each with a validated semantic class,
/// and the ranker used to reduce them to words: `chosen_because` tokenized to
/// `chosen`, because `because` is a stop word. A question that starts with
/// *why* is asking for a motivational or causal edge; one that asks what
/// replaced something is asking for `supersedes`; one that asks what a claim
/// rests on is asking for `verified_by`. That is a routing decision the stored
/// vocabulary can answer directly, without guessing at synonyms.
///
/// Which words ask for which family is data, in
/// `language/question_families.json`. This is only what one question turned
/// out to be asking for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct QuestionIntent {
    families: BTreeSet<String>,
}

impl QuestionIntent {
    pub(super) fn read(question: &str) -> Self {
        let vocabulary = QuestionVocabulary::shipped();
        let tokens = question
            .split(|character: char| !character.is_alphanumeric())
            .map(fold_search_term)
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();

        let mut families = BTreeSet::new();
        for (index, token) in tokens.iter().enumerate() {
            if let Some(family) = vocabulary.family_of(token) {
                families.insert(family.to_string());
            }
            // Spanish writes the commonest of these questions as two words.
            // `por qué` and `por que` both have to reach the family the
            // single-word `porque` reaches.
            if let Some(previous) = index.checked_sub(1)
                && let Some(family) = vocabulary.family_of(&format!("{}{token}", tokens[previous]))
            {
                families.insert(family.to_string());
            }
        }
        Self { families }
    }

    /// Empty means the question named no connection, and every relation is
    /// then equally welcome — the behaviour that existed before this type.
    pub(super) fn is_unspecific(&self) -> bool {
        self.families.is_empty()
    }

    pub(super) fn matches(&self, relation_type: &str, class: &RelationSemanticClass) -> bool {
        QuestionVocabulary::shipped().answers(&self.families, relation_type, class)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_why_question_asks_for_causal_and_motivational_edges() {
        let intent = QuestionIntent::read("Why did the gate reject the release?");

        assert!(intent.matches("chosen_because", &RelationSemanticClass::Motivational));
        assert!(intent.matches("some_extension", &RelationSemanticClass::Causal));
        assert!(!intent.matches("contains_entry", &RelationSemanticClass::Structural));
    }

    #[test]
    fn spanish_reaches_the_same_families_without_diacritics() {
        let accented = QuestionIntent::read("¿Por qué se eligió ese motor?");
        let plain = QuestionIntent::read("Por que se eligio ese motor");

        assert_eq!(accented, plain);
        assert!(accented.matches("triggers", &RelationSemanticClass::Causal));
    }

    #[test]
    fn a_replacement_question_asks_for_lifecycle_edges() {
        let intent = QuestionIntent::read("Que motor reemplazo al anterior?");

        assert!(intent.matches("supersedes", &RelationSemanticClass::Evidential));
        assert!(!intent.matches("follows", &RelationSemanticClass::Procedural));
    }

    #[test]
    fn an_evidence_question_asks_for_evidential_edges() {
        let intent = QuestionIntent::read("What evidence verified the migration?");

        assert!(intent.matches("verified_by", &RelationSemanticClass::Evidential));
        assert!(!intent.matches("component_of", &RelationSemanticClass::Structural));
    }

    #[test]
    fn a_constraint_question_asks_for_constraint_edges() {
        let intent = QuestionIntent::read("Que requisito impide desplegar?");

        assert!(intent.matches("violates_constraint", &RelationSemanticClass::Constraint));
    }

    #[test]
    fn a_question_that_names_no_connection_prefers_nothing() {
        let intent = QuestionIntent::read("Which engine is current for the shared store?");

        assert!(intent.is_unspecific());
        assert!(!intent.matches("supersedes", &RelationSemanticClass::Evidential));
    }

    #[test]
    fn composition_and_process_questions_reach_their_own_families() {
        assert!(
            QuestionIntent::read("What are the components of the total?")
                .matches("component_of", &RelationSemanticClass::Structural)
        );
        assert!(
            QuestionIntent::read("Cuales son los pasos del proceso?")
                .matches("follows", &RelationSemanticClass::Procedural)
        );
    }
}
