use std::collections::BTreeSet;

use kmp_domain::RelationSemanticClass;

use super::answer_ranker::fold_search_term;

/// What kind of connection a question is asking about.
///
/// KMP stores thirty-one relation types, each with a validated semantic
/// class, and the ranker used to reduce them to words: `chosen_because`
/// tokenized to `chosen`, because `because` is a stop word. A question that
/// starts with *why* is asking for a motivational or causal edge; one that
/// asks what replaced something is asking for `supersedes`; one that asks
/// what a claim rests on is asking for `verified_by`. That is a routing
/// decision the stored vocabulary can answer directly, without guessing at
/// synonyms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum QuestionFamily {
    Why,
    Lifecycle,
    Evidence,
    Constraint,
    Process,
    Composition,
}

impl QuestionFamily {
    /// The semantic classes an edge must belong to for this family.
    fn classes(self) -> &'static [RelationSemanticClass] {
        match self {
            Self::Why => &[
                RelationSemanticClass::Causal,
                RelationSemanticClass::Motivational,
            ],
            Self::Lifecycle => &[
                RelationSemanticClass::Causal,
                RelationSemanticClass::Evidential,
            ],
            Self::Evidence => &[RelationSemanticClass::Evidential],
            Self::Constraint => &[RelationSemanticClass::Constraint],
            Self::Process => &[RelationSemanticClass::Procedural],
            Self::Composition => &[RelationSemanticClass::Structural],
        }
    }

    /// The stored relation types this family asks for by name.
    fn relation_types(self) -> &'static [&'static str] {
        match self {
            Self::Why => &[
                "chosen_because",
                "triggers",
                "authorizes",
                "depends_on",
                "contributes_to",
            ],
            Self::Lifecycle => &[
                "supersedes",
                "corrects",
                "semantic_delta_from",
                "updates_state",
                "restates",
                "contradicts",
            ],
            Self::Evidence => &[
                "verified_by",
                "supports",
                "checked_against",
                "derived_from",
                "confirms_selection",
                "answers",
            ],
            Self::Constraint => &[
                "satisfies_constraint",
                "violates_constraint",
                "matches_requirement",
                "excluded_from",
                "authorizes",
            ],
            Self::Process => &["follows", "contributes_to", "uses_background"],
            Self::Composition => &["component_of", "total_of", "contains", "member_of"],
        }
    }
}

/// The families one question asks for. Empty means the question named no
/// connection, and every relation is then equally welcome — which is the
/// behaviour that existed before this type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct QuestionIntent {
    families: BTreeSet<QuestionFamily>,
}

impl QuestionIntent {
    pub(super) fn read(question: &str) -> Self {
        let tokens = question
            .split(|character: char| !character.is_alphanumeric())
            .map(fold_search_term)
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();

        let mut families = BTreeSet::new();
        for (index, token) in tokens.iter().enumerate() {
            if let Some(family) = family_for(token) {
                families.insert(family);
            }
            // Spanish writes the commonest of these questions as two words.
            // `por qué` and `por que` both have to reach the same family the
            // single-word `porque` reaches.
            if let Some(previous) = index.checked_sub(1)
                && let Some(family) = family_for(&format!("{}{token}", tokens[previous]))
            {
                families.insert(family);
            }
        }
        Self { families }
    }

    pub(super) fn is_unspecific(&self) -> bool {
        self.families.is_empty()
    }

    /// Whether a stored relation answers what the question is asking for.
    ///
    /// Either half is enough: the exact stored type, or its semantic class.
    /// A memory written with `triggers` and a memory written with an
    /// extension type of the same causal class both answer *why*.
    pub(super) fn matches(&self, relation_type: &str, class: &RelationSemanticClass) -> bool {
        self.families.iter().any(|family| {
            family.relation_types().contains(&relation_type) || family.classes().contains(class)
        })
    }
}

fn family_for(token: &str) -> Option<QuestionFamily> {
    if let Some(family) = EXACT_TOKENS
        .iter()
        .find_map(|(word, family)| (*word == token).then_some(*family))
    {
        return Some(family);
    }
    PREFIXES
        .iter()
        .find_map(|(prefix, family)| token.starts_with(prefix).then_some(*family))
}

/// Written folded: the reader compares against `fold_search_term` output, so
/// `por qué`, `razón` and `cómo` arrive here without their diacritics.
const EXACT_TOKENS: &[(&str, QuestionFamily)] = &[
    // why
    ("why", QuestionFamily::Why),
    ("reason", QuestionFamily::Why),
    ("rationale", QuestionFamily::Why),
    ("motive", QuestionFamily::Why),
    ("purpose", QuestionFamily::Why),
    ("cause", QuestionFamily::Why),
    ("caused", QuestionFamily::Why),
    ("causes", QuestionFamily::Why),
    ("porque", QuestionFamily::Why),
    ("razon", QuestionFamily::Why),
    ("razones", QuestionFamily::Why),
    ("motivo", QuestionFamily::Why),
    ("causa", QuestionFamily::Why),
    ("proposito", QuestionFamily::Why),
    // lifecycle
    ("replace", QuestionFamily::Lifecycle),
    ("replaced", QuestionFamily::Lifecycle),
    ("replaces", QuestionFamily::Lifecycle),
    ("supersede", QuestionFamily::Lifecycle),
    ("superseded", QuestionFamily::Lifecycle),
    ("supersedes", QuestionFamily::Lifecycle),
    ("previous", QuestionFamily::Lifecycle),
    ("prior", QuestionFamily::Lifecycle),
    ("former", QuestionFamily::Lifecycle),
    ("changed", QuestionFamily::Lifecycle),
    ("corrected", QuestionFamily::Lifecycle),
    ("anterior", QuestionFamily::Lifecycle),
    ("previo", QuestionFamily::Lifecycle),
    ("cambio", QuestionFamily::Lifecycle),
    ("corrigio", QuestionFamily::Lifecycle),
    // evidence
    ("evidence", QuestionFamily::Evidence),
    ("proof", QuestionFamily::Evidence),
    ("prove", QuestionFamily::Evidence),
    ("proves", QuestionFamily::Evidence),
    ("proved", QuestionFamily::Evidence),
    ("verify", QuestionFamily::Evidence),
    ("verified", QuestionFamily::Evidence),
    ("based", QuestionFamily::Evidence),
    ("supports", QuestionFamily::Evidence),
    ("source", QuestionFamily::Evidence),
    ("evidencia", QuestionFamily::Evidence),
    ("prueba", QuestionFamily::Evidence),
    ("basa", QuestionFamily::Evidence),
    ("apoya", QuestionFamily::Evidence),
    ("fuente", QuestionFamily::Evidence),
    ("respalda", QuestionFamily::Evidence),
    // constraint
    ("constraint", QuestionFamily::Constraint),
    ("require", QuestionFamily::Constraint),
    ("required", QuestionFamily::Constraint),
    ("requires", QuestionFamily::Constraint),
    ("requirement", QuestionFamily::Constraint),
    ("forbid", QuestionFamily::Constraint),
    ("forbids", QuestionFamily::Constraint),
    ("prevent", QuestionFamily::Constraint),
    ("prevents", QuestionFamily::Constraint),
    ("blocks", QuestionFamily::Constraint),
    ("allowed", QuestionFamily::Constraint),
    ("requisito", QuestionFamily::Constraint),
    ("requiere", QuestionFamily::Constraint),
    ("impide", QuestionFamily::Constraint),
    ("bloquea", QuestionFamily::Constraint),
    ("permite", QuestionFamily::Constraint),
    ("prohibe", QuestionFamily::Constraint),
    // process
    ("steps", QuestionFamily::Process),
    ("process", QuestionFamily::Process),
    ("procedure", QuestionFamily::Process),
    ("sequence", QuestionFamily::Process),
    ("pasos", QuestionFamily::Process),
    ("proceso", QuestionFamily::Process),
    ("procedimiento", QuestionFamily::Process),
    ("secuencia", QuestionFamily::Process),
    // composition
    ("component", QuestionFamily::Composition),
    ("components", QuestionFamily::Composition),
    ("total", QuestionFamily::Composition),
    ("includes", QuestionFamily::Composition),
    ("member", QuestionFamily::Composition),
    ("componente", QuestionFamily::Composition),
    ("componentes", QuestionFamily::Composition),
    ("incluye", QuestionFamily::Composition),
    ("miembro", QuestionFamily::Composition),
];

/// Spanish verb families the folded exact list cannot enumerate. Prefixes are
/// long enough that they do not collide with unrelated vocabulary.
const PREFIXES: &[(&str, QuestionFamily)] = &[
    ("reemplaz", QuestionFamily::Lifecycle),
    ("sustitu", QuestionFamily::Lifecycle),
    ("actualiz", QuestionFamily::Lifecycle),
    ("verific", QuestionFamily::Evidence),
    ("demuestr", QuestionFamily::Evidence),
    ("restricc", QuestionFamily::Constraint),
    ("autoriz", QuestionFamily::Constraint),
];

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
