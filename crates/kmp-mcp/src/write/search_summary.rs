//! The writer's English rendering of a memory, judged where it is written.
//!
//! One concept: whether the summary a writer attached will carry retrieval,
//! said to the writer now, while the writer can still fix it. The kernel
//! makes the same reading at ingest and again at ranking; this module is
//! the strict writer's version of it, which refuses instead of warning.
//! It judges what the caller supplied and never rewrites it.

use super::validation_error::WriteValidationError;

use kmp_domain::language::{KERNEL_LANGUAGE, LanguageVocabulary};
use kmp_domain::{SearchSummary, SearchSummaryFault};

/// What the writer decided about the summary: the value to store, if any,
/// and what to tell the caller about it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct SearchSummaryDecision {
    pub(super) stored: Option<String>,
    pub(super) diagnostics: Vec<String>,
}

/// Judges `summary_en` against `summary`.
///
/// Strict mode is where the writer is told rather than warned: a memory that
/// does not read as English must carry a rendering, because that is the only
/// way an English question reaches it, and a rendering that fails the lint is
/// refused with every fault named so it is fixed in one pass. Outside strict
/// mode the summary is stored as written and the caller is told what it will
/// not carry; the kernel's own ingest warning says the same.
///
/// A rendering that is byte for byte the summary of a memory that needs none
/// is the one case settled without the writer: an English memory is searched
/// by its own text, so the copy is dropped, the write goes on and the caller
/// is told why. Nothing searchable is lost and the citation is untouched.
pub(super) fn decide_search_summary(
    text: &str,
    summary: Option<&str>,
    strict: bool,
) -> Result<SearchSummaryDecision, WriteValidationError> {
    let foreign = LanguageVocabulary::shipped()
        .leans_in(text)
        .filter(|language| *language != KERNEL_LANGUAGE);
    let Some(summary) = summary else {
        return match foreign {
            Some(language) if strict => Err(WriteValidationError::new(format!(
                "strict kmp_write_memory requires summary_en: summary leans to {language}, and \
                 an English rendering is what an English question lands on. Add summary_en in \
                 plain English{}; never alter summary to fit it. Removing summary_en never \
                 passes while summary is not English",
                keep_verbatim(text)
            ))
            .at("summary_en")
            .code("SEARCH_SUMMARY_REQUIRED")),
            _ => Ok(SearchSummaryDecision::default()),
        };
    };
    if foreign.is_none() && summary == text {
        return Ok(SearchSummaryDecision {
            stored: None,
            diagnostics: vec![IDENTICAL_COPY_DROPPED.to_string()],
        });
    }
    match SearchSummary::lint(text, summary) {
        Ok(_) => Ok(SearchSummaryDecision {
            stored: Some(summary.to_string()),
            diagnostics: Vec::new(),
        }),
        Err(faults) if strict => Err(WriteValidationError::new(format!(
            "strict kmp_write_memory refuses summary_en: {}. Fix summary_en; never change \
             summary.{}",
            SearchSummaryFault::describe(&faults),
            repair_hint(&faults, foreign)
        ))
        .at("summary_en")
        .code("INVALID_SEARCH_SUMMARY")),
        Err(faults) => Ok(SearchSummaryDecision {
            stored: Some(summary.to_string()),
            diagnostics: vec![format!(
                "summary_en is stored but will not carry retrieval: {}",
                SearchSummaryFault::describe(&faults)
            )],
        }),
    }
}

/// Said when a copy of an English summary is dropped instead of refused.
pub(super) const IDENTICAL_COPY_DROPPED: &str = "summary_en omitted: identical to an English \
     summary, which is searched as written. Omit summary_en when summary is English";

/// The tokens a rendering of `text` must copy, as a clause of the request.
fn keep_verbatim(text: &str) -> String {
    let required = SearchSummary::required_identifiers(text);
    if required.is_empty() {
        String::new()
    } else {
        format!(
            " and keep these tokens exactly as written: {}",
            required.join(", ")
        )
    }
}

/// What to do about the faults beyond fixing the rendering: copy what was
/// dropped, and never answer a refused rendering by removing it.
fn repair_hint(faults: &[SearchSummaryFault], foreign: Option<&str>) -> String {
    let mut hint = String::new();
    if faults
        .iter()
        .any(|fault| matches!(fault, SearchSummaryFault::DropsIdentifiers(_)))
    {
        hint.push_str(" Copy the dropped identifiers into summary_en as written.");
    }
    if let Some(language) = foreign {
        hint.push_str(&format!(
            " Do not remove summary_en: summary leans to {language}, so it is required"
        ));
    }
    hint
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPANISH: &str = "El despliegue de v0.7.0 se retrasó porque los auditores no firmaron.";

    #[test]
    fn a_strict_write_of_a_memory_not_in_english_requires_the_rendering() {
        let error = decide_search_summary(SPANISH, None, true)
            .expect_err("a Spanish memory without a rendering is refused in strict mode");

        assert!(error.message.contains("requires summary_en"), "{error}");
        assert!(error.message.contains("leans to spanish"), "{error}");
        assert!(error.message.contains("never alter summary"), "{error}");
        assert!(
            error.message.contains("Removing summary_en never passes"),
            "{error}"
        );
        assert!(
            error
                .message
                .contains("keep these tokens exactly as written: v0.7.0"),
            "{error}"
        );
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

        assert!(error.message.contains("refuses summary_en"), "{error}");
        assert!(
            error.message.contains("leans to spanish, not to English"),
            "{error}"
        );
        assert!(
            error
                .message
                .contains("drops identifiers the text carries: v0.7.0"),
            "{error}"
        );
        assert!(
            error
                .message
                .contains("Fix summary_en; never change summary"),
            "{error}"
        );
        assert!(
            error.message.contains("Copy the dropped identifiers"),
            "{error}"
        );
        assert!(
            error.message.contains("Do not remove summary_en"),
            "{error}"
        );
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
                "summary_en is stored but will not carry retrieval: drops identifiers the \
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

    #[test]
    fn a_copy_of_an_english_summary_is_dropped_and_the_write_goes_on() {
        const ENGLISH: &str = "The rollout slipped because the auditors had not signed off.";
        for strict in [true, false] {
            let decision = decide_search_summary(ENGLISH, Some(ENGLISH), strict)
                .expect("an identical copy of an English summary is not refused");

            assert_eq!(decision.stored, None);
            assert_eq!(decision.diagnostics, [IDENTICAL_COPY_DROPPED]);
        }
    }

    #[test]
    fn a_copy_that_differs_by_a_byte_or_of_a_spanish_summary_is_still_linted() {
        const ENGLISH: &str = "The rollout slipped because the auditors had not signed off.";
        let error = decide_search_summary(
            ENGLISH,
            Some("The rollout slipped because the auditors had not signed off"),
            true,
        )
        .expect_err("a near copy is not the byte-for-byte case");
        assert!(error.message.contains("repeats the text"), "{error}");
        assert!(!error.message.contains("Do not remove"), "{error}");

        let error = decide_search_summary(SPANISH, Some(SPANISH), true)
            .expect_err("a Spanish memory still needs an English rendering");
        assert!(
            error.message.contains("Do not remove summary_en"),
            "{error}"
        );
    }

    #[test]
    fn a_required_rendering_without_identifiers_names_none() {
        let error = decide_search_summary("La válvula se congeló por la noche.", None, true)
            .expect_err("Spanish without a rendering is refused");

        assert!(!error.message.contains("keep these tokens"), "{error}");
        assert!(
            error.message.contains("Add summary_en in plain English;"),
            "{error}"
        );
    }
}
