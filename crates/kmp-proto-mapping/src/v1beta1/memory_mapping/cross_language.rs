use kmp_domain::language::LanguageVocabulary;

/// Whether the lexical bridge may vouch for a memory: the language the
/// question leans in, read once, against the language a memory's text leans
/// in.
///
/// The table is a translation table. Its vectors come from a multilingual
/// embedding model, so it also puts words of one language near each other —
/// `deadline≈final`, `prefix≈prefer`, `primary≈main` — and a pair like that
/// is not the same word in another language but a neighbour in the same
/// one. Credited as stating a concept, such a neighbour answered questions
/// whose answer is not stored: with the table `kmp-mcp setup` installs, 13
/// of 360 synthetic "attribute not stored" questions were answered at `high`
/// (0 without it), and every zero-overlap paraphrase the table answered cited
/// the wrong memory. Inside one language a memory still has to share the
/// reader's words, as `LexicalBridge` has always said.
///
/// So a pair counts as stating what the question asked only when the
/// question and the memory lean in two different languages, each read from
/// its own function words (`LanguageVocabulary::leans_in`). A side that leans
/// in no language — no function word, a tie, a language the kernel has no
/// words for — gives the table nothing to translate between, and it vouches
/// for nothing. Ranking is untouched: a bridged word still weighs in BM25
/// and a memory it reached still arrives in the proof, marked; what changes
/// is only whether the table may stand in for a concept when the kernel
/// decides to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct CrossLanguage {
    asked: Option<&'static str>,
}

impl CrossLanguage {
    /// Reads the language the question leans in.
    pub(super) fn of_question(question: &str) -> Self {
        Self {
            asked: LanguageVocabulary::shipped().leans_in(question),
        }
    }

    /// Whether a memory with this text is written in another language than
    /// the question, both read.
    pub(super) fn crosses_to(&self, text: &str) -> bool {
        let Some(asked) = self.asked else {
            return false;
        };
        LanguageVocabulary::shipped()
            .leans_in(text)
            .is_some_and(|written| written != asked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_memory_in_the_other_language_is_crossed_to() {
        let english =
            CrossLanguage::of_question("What did the harbour crew clean the windows with?");
        assert!(english.crosses_to("La cuadrilla del puerto limpió las ventanas con Corvalis."));
        let spanish =
            CrossLanguage::of_question("¿Con qué limpió la cuadrilla del puerto las ventanas?");
        assert!(spanish.crosses_to("The harbour crew cleaned the windows with Valdemar."));
    }

    #[test]
    fn a_memory_in_the_same_language_is_not() {
        let question = CrossLanguage::of_question("What deadline does #10132 set for svc-013?");
        assert!(!question.crosses_to("#10132 sets the log retention of svc-013 to 14 days."));
        let question = CrossLanguage::of_question("Was kernel prefix #7 a deliberate decision?");
        assert!(
            !question.crosses_to(
                "The kernel prefer mode #7 delivered a decision from another workflow."
            )
        );
    }

    #[test]
    fn a_side_that_leans_nowhere_crosses_nothing() {
        // No function word in the question: nothing to translate from.
        let terse = CrossLanguage::of_question("deadline #10132 svc-013");
        assert!(!terse.crosses_to("El plazo de svc-013 vence el viernes."));
        // No function word in the memory: nothing to translate into.
        let english = CrossLanguage::of_question("What did the crew clean the windows with?");
        assert!(!english.crosses_to("ventanas limpias, Corvalis"));
        assert!(!CrossLanguage::default().crosses_to("La cuadrilla limpió las ventanas."));
    }
}
