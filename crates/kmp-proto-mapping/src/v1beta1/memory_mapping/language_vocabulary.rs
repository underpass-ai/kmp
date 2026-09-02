use std::collections::BTreeSet;
use std::sync::OnceLock;

use serde::Deserialize;

/// The words that give a memory's language away.
///
/// Function words are the right signal because a writer does not choose them:
/// a Spanish sentence carries `de`, `que` and `la` whatever it is about, and
/// its nouns may well be English product names. A word that appears in both
/// languages cannot tell them apart and is left out of the data.
///
/// It lives in `language/function_words.json` rather than in two arrays of
/// string literals, so widening the evidence for a language is a data change
/// somebody who knows the language can review.
#[derive(Debug, Deserialize)]
pub(super) struct LanguageVocabulary {
    languages: Vec<Language>,
}

#[derive(Debug, Deserialize)]
struct Language {
    id: String,
    function_words: BTreeSet<String>,
}

const SOURCE: &str = include_str!("../../../language/function_words.json");

/// Enough evidence to be reading a language rather than guessing at one, and a
/// clear enough majority — two in three — that one language is not having the
/// other's rules applied to it.
const MINIMUM_SIGNALS: usize = 3;

impl LanguageVocabulary {
    pub(super) fn shipped() -> &'static Self {
        static SHIPPED: OnceLock<LanguageVocabulary> = OnceLock::new();
        SHIPPED
            .get_or_init(|| serde_json::from_str(SOURCE).expect("the shipped function words parse"))
    }

    /// Which language a body of text is written in, or none when it cannot be
    /// read or is too evenly mixed to call.
    ///
    /// Returning nothing is a real answer and the common one for a small
    /// store: it leaves every word exactly as written.
    pub(super) fn read<'a>(&self, tokens: impl IntoIterator<Item = &'a str>) -> Option<&str> {
        let mut counts = vec![0usize; self.languages.len()];
        for token in tokens {
            for (index, language) in self.languages.iter().enumerate() {
                if language.function_words.contains(token) {
                    counts[index] += 1;
                    break;
                }
            }
        }
        let total = counts.iter().sum::<usize>();
        let (index, winner) = counts
            .iter()
            .enumerate()
            .max_by_key(|(index, count)| (**count, std::cmp::Reverse(*index)))
            .map(|(index, count)| (index, *count))?;
        if winner < MINIMUM_SIGNALS || winner * 3 < total * 2 {
            return None;
        }
        Some(self.languages[index].id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<&str> {
        text.split(' ').collect()
    }

    #[test]
    fn the_shipped_vocabulary_parses_and_keeps_its_languages_apart() {
        let vocabulary = LanguageVocabulary::shipped();
        let mut seen = BTreeSet::new();

        for language in &vocabulary.languages {
            assert!(seen.insert(language.id.as_str()), "duplicate language");
            assert!(!language.function_words.is_empty());
        }
        // A word in both lists could not tell the two apart and would only add
        // noise to the count.
        for (index, language) in vocabulary.languages.iter().enumerate() {
            for other in vocabulary.languages.iter().skip(index + 1) {
                let shared = language
                    .function_words
                    .intersection(&other.function_words)
                    .collect::<Vec<_>>();
                assert!(shared.is_empty(), "`{shared:?}` cannot decide a language");
            }
        }
    }

    #[test]
    fn a_clear_majority_names_the_language() {
        let vocabulary = LanguageVocabulary::shipped();

        assert_eq!(
            vocabulary.read(words(
                "el despliegue de la pasarela se congelo por la auditoria"
            )),
            Some("spanish")
        );
        assert_eq!(
            vocabulary.read(words(
                "the deployment of the gateway was frozen by the audit"
            )),
            Some("english")
        );
    }

    #[test]
    fn too_little_or_too_even_reads_as_nothing() {
        let vocabulary = LanguageVocabulary::shipped();

        assert_eq!(vocabulary.read(words("valkey")), None);
        assert_eq!(
            vocabulary.read(words(
                "the deployment of the gateway el despliegue de la pasarela"
            )),
            None
        );
    }
}
