//! The writer's English rendering of a memory, judged where it is written.
//!
//! One concept: whether the summary a writer attached will carry retrieval,
//! said to the writer now, while the writer can still fix it. The kernel
//! makes the same reading at ingest and again at ranking; this module is
//! the strict writer's version of it, which refuses instead of warning.
//! It judges what the caller supplied and never rewrites it.

use kmp_domain::language::{KERNEL_LANGUAGE, LanguageVocabulary};
use kmp_domain::{SearchSummary, SearchSummaryFault};

/// What the writer decided about the summary: the value to store, if any,
/// and what to tell the caller about it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct SearchSummaryDecision {
    pub(super) stored: Option<String>,
    pub(super) diagnostics: Vec<String>,
}

/// Judges `current.summary_en` against `current.summary`.
///
/// Strict mode is where the writer is told rather than warned: a memory that
/// does not read as English must carry a rendering, because that is the only
/// way an English question reaches it, and a rendering that fails the lint is
/// refused with every fault named so it is fixed in one pass. Outside strict
/// mode the summary is stored as written and the caller is told what it will
/// not carry; the kernel's own ingest warning says the same.
pub(super) fn decide_search_summary(
    text: &str,
    summary: Option<&str>,
    strict: bool,
) -> Result<SearchSummaryDecision, String> {
    let Some(summary) = summary else {
        return match LanguageVocabulary::shipped().leans_in(text) {
            Some(language) if strict && language != KERNEL_LANGUAGE => Err(format!(
                "strict kmp_write_memory requires current.summary_en: current.summary leans to \
                 {language}, and an English rendering is what an English question lands on. Write \
                 it in plain English, keep every number, identifier and acronym exactly as \
                 written, and never alter current.summary to fit it"
            )),
            _ => Ok(SearchSummaryDecision::default()),
        };
    };
    match SearchSummary::lint(text, summary) {
        Ok(_) => Ok(SearchSummaryDecision {
            stored: Some(summary.to_string()),
            diagnostics: Vec::new(),
        }),
        Err(faults) if strict => Err(format!(
            "strict kmp_write_memory refuses current.summary_en: {}. Fix the summary, never \
             current.summary",
            SearchSummaryFault::describe(&faults)
        )),
        Err(faults) => Ok(SearchSummaryDecision {
            stored: Some(summary.to_string()),
            diagnostics: vec![format!(
                "current.summary_en is stored but will not carry retrieval: {}",
                SearchSummaryFault::describe(&faults)
            )],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPANISH: &str = "El despliegue de v0.7.0 se retrasó porque los auditores no firmaron.";

    #[test]
    fn a_strict_write_of_a_memory_not_in_english_requires_the_rendering() {
        let error = decide_search_summary(SPANISH, None, true)
            .expect_err("a Spanish memory without a rendering is refused in strict mode");

        assert!(error.contains("requires current.summary_en"), "{error}");
        assert!(error.contains("leans to spanish"), "{error}");
        assert!(error.contains("never alter current.summary"), "{error}");
    }

    #[test]
    fn an_english_memory_may_omit_the_rendering() {
        let decision = decide_search_summary(
            "The rollout slipped because the auditors had not signed off.",
            None,
            true,
        )
        .expect("English text needs no rendering to be reached in English");

        assert_eq!(decision, SearchSummaryDecision::default());
    }

    #[test]
    fn a_rendering_that_passes_is_stored_without_comment() {
        let decision = decide_search_summary(
            SPANISH,
            Some("The v0.7.0 launch was postponed because the auditors had not signed off."),
            true,
        )
        .expect("a faithful rendering is accepted");

        assert_eq!(
            decision.stored.as_deref(),
            Some("The v0.7.0 launch was postponed because the auditors had not signed off.")
        );
        assert!(decision.diagnostics.is_empty());
    }

    #[test]
    fn a_strict_write_refuses_a_rendering_that_fails_the_lint_and_names_every_fault() {
        let error = decide_search_summary(SPANISH, Some("Se pospuso."), true)
            .expect_err("a rendering that fails the lint is refused in strict mode");

        assert!(error.contains("refuses current.summary_en"), "{error}");
        assert!(
            error.contains("leans to spanish, not to English"),
            "{error}"
        );
        assert!(
            error.contains("drops identifiers the text carries: v0.7.0"),
            "{error}"
        );
        assert!(error.contains("never current.summary"), "{error}");
    }

    #[test]
    fn outside_strict_mode_a_failing_rendering_is_stored_and_said_not_to_carry() {
        let decision = decide_search_summary(
            SPANISH,
            Some("The launch was postponed because the auditors had not signed off."),
            false,
        )
        .expect("non-strict passes the rendering through");

        assert!(decision.stored.is_some());
        assert_eq!(
            decision.diagnostics,
            [
                "current.summary_en is stored but will not carry retrieval: drops identifiers the \
              text carries: v0.7.0"
            ]
        );
    }

    #[test]
    fn outside_strict_mode_a_memory_not_in_english_may_omit_the_rendering() {
        let decision =
            decide_search_summary(SPANISH, None, false).expect("non-strict does not require it");

        assert_eq!(decision, SearchSummaryDecision::default());
    }
}
