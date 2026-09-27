use std::collections::BTreeSet;

use crate::language::{dropped_identifiers, informative_tokens};

use super::search_expansion_fault::SearchExpansionFault;
use super::search_summary::SearchSummary;

/// The short ways a later reader may ask for a memory, proposed by its
/// writer and kept only when a judge read them as belonging to it
/// (Doc2Query--, arXiv 2301.03266).
///
/// An expansion is a question the memory answers, a paraphrase of it, or a
/// key in the other language (Spanish or English). It is a **search surface
/// and never a citation**: a question that reaches a memory only through its
/// expansions brings the memory back outside the answer core, marked
/// `reached_by: expansion`, and what is cited is the memory's own text, byte
/// for byte. An expansion never names an anchor, never answers, and never
/// raises confidence.
///
/// Expansions travel as reserved entry metadata. They are stored only after
/// a judge accepted them, bound to the exact text it read
/// ([`Self::SOURCE_FINGERPRINT_METADATA_KEY`]): a memory whose text moved
/// since is searched without them, whoever wrote the metadata and however it
/// arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchExpansions;

impl SearchExpansions {
    /// The metadata key the accepted expansions travel under, one per line.
    pub const METADATA_KEY: &'static str = "search_expansions";

    /// The fingerprint of the stored text the judge read the expansions
    /// against ([`SearchSummary::source_fingerprint`]).
    pub const SOURCE_FINGERPRINT_METADATA_KEY: &'static str = "search_expansions_source_sha256";

    /// Which judge accepted them, and at what bar.
    pub const JUDGED_BY_METADATA_KEY: &'static str = "search_expansions_judged_by";

    /// The most expansions one memory keeps.
    pub const MAX_EXPANSIONS: usize = 6;

    /// The longest expansion kept, in characters: a question, not a passage.
    pub const MAX_CHARS: usize = 120;

    /// Whether a metadata key belongs to the expansions, which the ranker
    /// reads as their own surface and never as the memory's words.
    pub fn is_metadata_key(key: &str) -> bool {
        key.starts_with(Self::METADATA_KEY)
    }

    /// Reads one proposed expansion against the memory it expands and the
    /// expansions already kept before it. Every fault is reported.
    pub fn lint(
        text: &str,
        expansion: &str,
        kept: &[String],
    ) -> Result<String, Vec<SearchExpansionFault>> {
        let expansion = expansion.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut faults = Vec::new();
        let chars = expansion.chars().count();
        if chars > Self::MAX_CHARS {
            faults.push(SearchExpansionFault::TooLong { chars });
        }
        let words = informative_tokens(&expansion).collect::<BTreeSet<_>>();
        if words.is_empty() {
            faults.push(SearchExpansionFault::Empty);
        } else if words.is_subset(&informative_tokens(text).collect()) {
            faults.push(SearchExpansionFault::RepeatsText);
        }
        let folded = expansion.to_lowercase();
        if kept.iter().any(|earlier| earlier.to_lowercase() == folded) {
            faults.push(SearchExpansionFault::Duplicate);
        }
        let added = dropped_identifiers(&expansion, text);
        if !added.is_empty() {
            faults.push(SearchExpansionFault::AddsIdentifiers(added));
        }
        if faults.is_empty() {
            Ok(expansion)
        } else {
            Err(faults)
        }
    }

    /// The metadata value of kept expansions: one per line.
    pub fn render(expansions: &[String]) -> String {
        expansions.join("\n")
    }

    /// The expansions a stored memory may be searched by: those whose
    /// judgement was bound to exactly this text. None when the text moved,
    /// the binding is missing or nothing was kept.
    pub fn stored<'a>(text: &str, metadata: impl Fn(&str) -> Option<&'a str>) -> Vec<&'a str> {
        let bound = metadata(Self::SOURCE_FINGERPRINT_METADATA_KEY)
            .is_some_and(|fingerprint| fingerprint == SearchSummary::source_fingerprint(text));
        if !bound {
            return Vec::new();
        }
        metadata(Self::METADATA_KEY)
            .map(|value| {
                value
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .take(Self::MAX_EXPANSIONS)
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const TEXT: &str = "The rollout slipped because the auditors had not signed off.";

    #[test]
    fn a_question_in_other_words_is_kept_trimmed() {
        let kept = SearchExpansions::lint(TEXT, "  Why was the  launch postponed? ", &[])
            .expect("a paraphrase in other words is kept");
        assert_eq!(kept, "Why was the launch postponed?");
    }

    #[test]
    fn a_key_in_the_other_language_is_kept() {
        SearchExpansions::lint(TEXT, "¿Por qué se retrasó el lanzamiento?", &[])
            .expect("a Spanish key is a search surface");
    }

    #[test]
    fn an_expansion_that_adds_nothing_or_answers_is_refused() {
        let faults = |expansion: &str, kept: &[String]| {
            SearchExpansions::lint(TEXT, expansion, kept).expect_err("refused")
        };
        assert_eq!(
            faults("the rollout slipped", &[]),
            [SearchExpansionFault::RepeatsText]
        );
        assert_eq!(faults("   ", &[]), [SearchExpansionFault::Empty]);
        assert_eq!(
            faults("launch postponed until v0.9.0", &[]),
            [SearchExpansionFault::AddsIdentifiers(vec!["v0.9.0".into()])]
        );
        assert_eq!(
            faults("Launch postponed", &["launch postponed".to_string()]),
            [SearchExpansionFault::Duplicate]
        );
        let long = "launch postponed ".repeat(10);
        assert!(matches!(
            faults(&long, &[])[0],
            SearchExpansionFault::TooLong { .. }
        ));
    }

    #[test]
    fn stored_expansions_count_only_for_the_text_they_were_judged_against() {
        let mut metadata = BTreeMap::new();
        metadata.insert(
            SearchExpansions::METADATA_KEY.to_string(),
            "Why was the launch postponed?\n\n launch delay ".to_string(),
        );
        metadata.insert(
            SearchExpansions::SOURCE_FINGERPRINT_METADATA_KEY.to_string(),
            SearchSummary::source_fingerprint(TEXT),
        );
        fn read(metadata: &BTreeMap<String, String>, text: &str) -> Vec<String> {
            SearchExpansions::stored(text, |key| metadata.get(key).map(String::as_str))
                .into_iter()
                .map(str::to_string)
                .collect()
        }
        assert_eq!(
            read(&metadata, TEXT),
            ["Why was the launch postponed?", "launch delay"]
        );
        assert!(
            read(&metadata, "The rollout slipped.").is_empty(),
            "the text moved"
        );
        metadata.remove(SearchExpansions::SOURCE_FINGERPRINT_METADATA_KEY);
        assert!(read(&metadata, TEXT).is_empty(), "no binding, no search");
    }

    #[test]
    fn every_expansion_key_is_recognised_and_rendering_is_one_per_line() {
        assert!(SearchExpansions::is_metadata_key("search_expansions"));
        assert!(SearchExpansions::is_metadata_key(
            SearchExpansions::JUDGED_BY_METADATA_KEY
        ));
        assert!(!SearchExpansions::is_metadata_key("summary_en"));
        assert_eq!(
            SearchExpansions::render(&["a b".into(), "c d".into()]),
            "a b\nc d"
        );
    }
}
