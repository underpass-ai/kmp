use crate::language::{
    KERNEL_LANGUAGE, LanguageVocabulary, identifiers, informative_tokens, surface_tokens,
};

use super::question_rendering_fault::QuestionRenderingFault;

/// A question in the kernel's search language, rendered by the agent from the
/// user's own words.
///
/// The kernel matches words and never translates. What reaches a memory
/// written in any language is its English search summary, so the agent asks
/// in English and passes what the user actually said as `asked_as`. The
/// kernel searches the rendering, echoes the user's words for the audit
/// trail, and reads the rendering against them the way it reads a search
/// summary against its text: form and fidelity of identifiers, never meaning.
///
/// A fault is a warning on the answer, not a refusal. The kernel accepts a
/// question in any language — a question in the store's own language reaches
/// the stored text directly — so a rendering that failed is still searched,
/// and the warning says what it lost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionRendering {
    question: String,
}

impl QuestionRendering {
    /// Reads a question against the words it renders.
    ///
    /// Every fault is reported, in the order they are reported: the question
    /// leans to a language other than the kernel's; it carries no informative
    /// word at all; it drops an identifier the user's words carry — a token
    /// with a digit, a tagged one (`#469`), a compound one (`kmp-mcp`) or an
    /// acronym. A question that repeats the user's words exactly is not a
    /// fault: the user may have asked in English already.
    pub fn lint(asked_as: &str, question: &str) -> Result<Self, Vec<QuestionRenderingFault>> {
        let mut faults = Vec::new();

        if let Some(language) = LanguageVocabulary::shipped().leans_in(question)
            && language != KERNEL_LANGUAGE
        {
            faults.push(QuestionRenderingFault::NotEnglish {
                read: language.to_string(),
            });
        }

        if informative_tokens(question).next().is_none() {
            faults.push(QuestionRenderingFault::Empty);
        }

        let carried = surface_tokens(question);
        let dropped = identifiers(asked_as)
            .into_iter()
            .filter(|identifier| !carried.contains(identifier))
            .collect::<Vec<_>>();
        if !dropped.is_empty() {
            faults.push(QuestionRenderingFault::DropsIdentifiers(dropped));
        }

        if faults.is_empty() {
            Ok(Self {
                question: question.to_string(),
            })
        } else {
            Err(faults)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.question
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASKED: &str = "¿Por qué se retrasó el despliegue de v0.7.0 (#469)?";

    fn faults(asked_as: &str, question: &str) -> Vec<QuestionRenderingFault> {
        QuestionRendering::lint(asked_as, question).expect_err("the rendering should be faulted")
    }

    #[test]
    fn a_faithful_english_rendering_carries() {
        let rendering =
            QuestionRendering::lint(ASKED, "Why was the v0.7.0 launch (#469) postponed?")
                .expect("identifiers kept, English, informative");

        assert_eq!(
            rendering.as_str(),
            "Why was the v0.7.0 launch (#469) postponed?"
        );
    }

    #[test]
    fn a_question_that_repeats_the_users_english_is_not_a_fault() {
        QuestionRendering::lint(
            "Which valve froze during the night shift?",
            "Which valve froze during the night shift?",
        )
        .expect("the user may already have asked in English");
    }

    #[test]
    fn a_rendering_that_drops_an_identifier_says_which() {
        let faults = faults(ASKED, "Why was the launch postponed?");

        assert_eq!(
            faults,
            [QuestionRenderingFault::DropsIdentifiers(vec![
                "#469".to_string(),
                "v0.7.0".to_string()
            ])]
        );
        assert_eq!(
            faults[0].to_string(),
            "drops identifiers the user's words carry: #469, v0.7.0"
        );
    }

    #[test]
    fn a_rendering_in_the_users_language_is_faulted() {
        let faults = faults(ASKED, "¿Por qué se retrasó el despliegue de v0.7.0 (#469)?");

        assert_eq!(
            faults,
            [QuestionRenderingFault::NotEnglish {
                read: "spanish".to_string()
            }]
        );
    }

    #[test]
    fn a_rendering_with_no_informative_word_is_faulted() {
        let faults = faults("¿Qué?", "What?");

        assert_eq!(faults, [QuestionRenderingFault::Empty]);
    }

    #[test]
    fn every_fault_is_reported_at_once() {
        let faults = faults(ASKED, "¿Por qué?");

        assert_eq!(
            QuestionRenderingFault::describe(&faults),
            "leans to spanish, not to English; carries no informative word; \
             drops identifiers the user's words carry: #469, v0.7.0"
        );
    }
}
