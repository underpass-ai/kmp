use std::collections::BTreeSet;

use crate::language::{KERNEL_LANGUAGE, LanguageVocabulary, fold_search_term, informative_tokens};

use super::search_summary_fault::SearchSummaryFault;

/// An English rendering of a memory, attached by its writer so that a
/// question in the kernel's language reaches the memory whatever language it
/// was written in.
///
/// The kernel matches words and never translates. A memory written in
/// Spanish is reachable from a Spanish question, and from an English one only
/// through a table of single words. What a writer can do at the one moment
/// the kernel admits a model — the write — is say the same thing in English,
/// canonically: `el despliegue se retrasó` becomes `the launch was postponed`,
/// and the jargon a table of single vectors cannot reach (`rollout slipped`)
/// becomes the plain phrase a reader will ask with. The summary is
/// **searched and never cited**: the citation stays the text as stored, byte
/// for byte, and the summary is visible beside it as what it is.
///
/// It travels as the reserved entry metadata key [`Self::METADATA_KEY`]. It is
/// linted, not trusted: the same deterministic reading is made when the
/// memory is ingested, so the writer hears at once what it will not carry,
/// and again when a question is ranked, so a summary that fails it carries no
/// retrieval whoever wrote it and however it arrived. That mirrors how a
/// relation's quality is judged: reported at write time, recomputed at read
/// time, never stored as a verdict.
///
/// The lint checks form and fidelity of identifiers, not meaning. A fluent,
/// wrong summary passes; one that dropped the version or the issue number it
/// summarised does not, and that catches the coarse failures of a model that
/// rewrote instead of restating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSummary {
    summary: String,
}

impl SearchSummary {
    /// The entry metadata key the summary travels under.
    pub const METADATA_KEY: &'static str = "summary_en";

    /// How many informative words a summary must carry to be worth
    /// searching. One word is a tag, not a rendering.
    pub const MINIMUM_INFORMATIVE_TERMS: usize = 2;

    /// Reads a summary against the text it summarises.
    ///
    /// Every fault is reported, not only the first, so a writer can fix a
    /// summary in one pass. The faults are, in the order they are reported:
    ///
    /// - its function words lean to another language — one `porque` is
    ///   enough to say a summary was not written in English, while a list of
    ///   English keywords, which has no function words at all, is accepted
    ///   for what it is: words a question in English can land on;
    /// - it carries fewer than two informative words;
    /// - it repeats the text word for word, which adds nothing to search;
    /// - it drops an identifier the text carries: a token with a digit
    ///   (`v0.7.0`, `2026-09-03`), a tagged one (`#469`, `@rachel`), a
    ///   compound one (`kmp-mcp`, `ref_boundary.rs`) or an acronym (`ADR`).
    ///   Names written with an initial capital are deliberately not checked:
    ///   a document called *Plan de Lanzamiento* is faithfully rendered as
    ///   *launch plan*, and a number never is.
    pub fn lint(text: &str, summary: &str) -> Result<Self, Vec<SearchSummaryFault>> {
        let mut faults = Vec::new();

        if let Some(language) = LanguageVocabulary::shipped().leans_in(summary)
            && language != KERNEL_LANGUAGE
        {
            faults.push(SearchSummaryFault::NotEnglish {
                read: language.to_string(),
            });
        }

        let summary_terms = informative_tokens(summary).collect::<Vec<_>>();
        if summary_terms.len() < Self::MINIMUM_INFORMATIVE_TERMS {
            faults.push(SearchSummaryFault::Thin {
                informative_terms: summary_terms.len(),
            });
        } else if summary_terms == informative_tokens(text).collect::<Vec<_>>() {
            faults.push(SearchSummaryFault::RepeatsText);
        }

        let carried = surface_tokens(summary);
        let dropped = identifiers(text)
            .into_iter()
            .filter(|identifier| !carried.contains(identifier))
            .collect::<Vec<_>>();
        if !dropped.is_empty() {
            faults.push(SearchSummaryFault::DropsIdentifiers(dropped));
        }

        if faults.is_empty() {
            Ok(Self {
                summary: summary.to_string(),
            })
        } else {
            Err(faults)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.summary
    }
}

/// The identifiers a text carries, folded: the tokens a faithful rendering in
/// another language keeps exactly as they are.
fn identifiers(text: &str) -> BTreeSet<String> {
    text.split_whitespace()
        .map(trim_edge_punctuation)
        .filter(|token| is_identifier(token))
        .map(fold_search_term)
        .collect()
}

/// Every whitespace-delimited token of a text, trimmed of the punctuation
/// that closes a clause and folded, so `#469,` and `(#469)` both carry `#469`.
fn surface_tokens(text: &str) -> BTreeSet<String> {
    text.split_whitespace()
        .map(trim_edge_punctuation)
        .filter(|token| !token.is_empty())
        .map(fold_search_term)
        .collect()
}

fn trim_edge_punctuation(token: &str) -> &str {
    const EDGE_PUNCTUATION: &[char] = &[
        '.', ',', ';', ':', '!', '?', '¡', '¿', '(', ')', '[', ']', '{', '}', '"', '\'', '«', '»',
        '“', '”', '‘', '’', '`',
    ];
    token.trim_matches(EDGE_PUNCTUATION)
}

/// Whether a token is something a translation copies rather than renders.
fn is_identifier(token: &str) -> bool {
    if token.chars().count() < 2 {
        return false;
    }
    let has_digit = token.chars().any(|character| character.is_ascii_digit());
    let is_tagged = token.starts_with('#') || token.starts_with('@');
    let is_acronym = token
        .chars()
        .all(|character| character.is_ascii_uppercase());
    has_digit || is_tagged || is_acronym || is_compound(token)
}

/// `kmp-mcp`, `ref_boundary.rs`, `feat/lexical-bridge`: alphanumeric runs of
/// at least two characters joined by the punctuation identifiers are joined
/// with. An abbreviation such as `e.g.` or `p.ej.` is not one, because a
/// translation renders those.
fn is_compound(token: &str) -> bool {
    const JOINERS: &[char] = &['-', '_', '/', '.', ':'];
    let inner = token.trim_matches(JOINERS);
    if !inner.contains(JOINERS) {
        return false;
    }
    let runs = inner.split(JOINERS).collect::<Vec<_>>();
    runs.len() >= 2
        && runs.iter().all(|run| {
            run.chars().count() >= 2 && run.chars().all(|character| character.is_alphanumeric())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPANISH_TEXT: &str =
        "El despliegue de v0.7.0 se retrasó porque los auditores no habían firmado (#469).";

    fn faults(text: &str, summary: &str) -> Vec<SearchSummaryFault> {
        SearchSummary::lint(text, summary).expect_err("the summary should be refused")
    }

    #[test]
    fn an_english_sentence_that_keeps_the_identifiers_carries() {
        let summary = SearchSummary::lint(
            SPANISH_TEXT,
            "The v0.7.0 launch was postponed because the auditors had not signed off (#469).",
        )
        .expect("a faithful English rendering carries");

        assert!(summary.as_str().starts_with("The v0.7.0 launch"));
    }

    #[test]
    fn a_summary_in_the_texts_own_language_is_refused() {
        let faults = faults(
            SPANISH_TEXT,
            "El lanzamiento de v0.7.0 se pospuso porque los auditores no firmaron (#469).",
        );

        assert_eq!(
            faults,
            [SearchSummaryFault::NotEnglish {
                read: "spanish".to_string()
            }]
        );
        assert_eq!(faults[0].to_string(), "leans to spanish, not to English");
    }

    /// A list of English keywords has no function words to lean on. It is
    /// accepted for what it is: words an English question can land on.
    #[test]
    fn a_list_of_english_keywords_carries() {
        SearchSummary::lint(SPANISH_TEXT, "launch v0.7.0 postponed auditors #469")
            .expect("keywords in the kernel's language are a search surface");
    }

    #[test]
    fn a_summary_that_drops_an_identifier_is_refused_and_names_it() {
        let faults = faults(
            SPANISH_TEXT,
            "The launch was postponed because the auditors had not signed off.",
        );

        assert_eq!(
            faults,
            [SearchSummaryFault::DropsIdentifiers(vec![
                "#469".to_string(),
                "v0.7.0".to_string()
            ])]
        );
        assert_eq!(
            faults[0].to_string(),
            "drops identifiers the text carries: #469, v0.7.0"
        );
    }

    #[test]
    fn identifiers_are_matched_folded_and_without_clause_punctuation() {
        SearchSummary::lint(
            "Se adoptó Valkey 7.2 en la rama feat/valkey-store; ver ADR-018 y kmp-mcp.",
            "Valkey 7.2 was adopted on the feat/valkey-store branch; see ADR-018 and KMP-MCP.",
        )
        .expect("case and edge punctuation do not decide fidelity");
    }

    #[test]
    fn an_abbreviation_is_not_an_identifier() {
        SearchSummary::lint(
            "Los adaptadores (p.ej. el de Valkey) se registran al arrancar.",
            "The adapters, e.g. the Valkey one, are registered at start-up.",
        )
        .expect("`p.ej.` is rendered, not copied");
    }

    #[test]
    fn a_copy_of_the_text_adds_nothing_and_is_refused() {
        let text = "The reserve valve failed during the night shift.";

        assert_eq!(
            faults(text, "The reserve valve failed during the night shift."),
            [SearchSummaryFault::RepeatsText]
        );
    }

    #[test]
    fn a_thin_summary_is_refused() {
        let faults = faults("La válvula de reserva se congeló.", "The valve.");

        assert!(
            faults.contains(&SearchSummaryFault::Thin {
                informative_terms: 1
            }),
            "{faults:?}"
        );
    }

    #[test]
    fn every_fault_is_reported_at_once() {
        let faults = faults(SPANISH_TEXT, "Se pospuso.");

        assert_eq!(faults.len(), 3, "{faults:?}");
        assert_eq!(
            SearchSummaryFault::describe(&faults),
            "leans to spanish, not to English; \
             carries 1 informative word, at least 2 are needed; \
             drops identifiers the text carries: #469, v0.7.0"
        );
    }
}
