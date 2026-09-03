use std::collections::BTreeSet;
use std::sync::OnceLock;

use serde::Deserialize;

use super::search_tokens::fold_search_term;

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
pub struct LanguageVocabulary {
    languages: Vec<Language>,
}

#[derive(Debug, Deserialize)]
struct Language {
    id: String,
    function_words: BTreeSet<String>,
}

const SOURCE: &str = include_str!("../../language/function_words.json");

/// Enough evidence to be reading a language rather than guessing at one, and a
/// clear enough majority — two in three — that one language is not having the
/// other's rules applied to it.
const MINIMUM_SIGNALS: usize = 3;

/// The language a question is expected to arrive in and a search summary is
/// expected to be written in. The kernel matches words; it does not
/// translate, so one side of every comparison has to name the language the
/// other side will be read in.
pub const KERNEL_LANGUAGE: &str = "english";

impl LanguageVocabulary {
    pub fn shipped() -> &'static Self {
        static SHIPPED: OnceLock<LanguageVocabulary> = OnceLock::new();
        SHIPPED
            .get_or_init(|| serde_json::from_str(SOURCE).expect("the shipped function words parse"))
    }

    /// Which language a body of text is written in, or none when it cannot be
    /// read or is too evenly mixed to call.
    ///
    /// Returning nothing is a real answer and the common one for a small
    /// store: it leaves every word exactly as written.
    pub fn read<'a>(&self, tokens: impl IntoIterator<Item = &'a str>) -> Option<&str> {
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

    /// Which way a short text leans: the language with more function words
    /// than any other, from however few, or none when there is no signal or
    /// a tie. The text is split and folded the way the retrieval layer
    /// splits and folds it, so the two readings agree.
    ///
    /// This is a different question from [`Self::read`]. Reading a memory's
    /// language decides which stemmer is applied to every word in it, and a
    /// wrong answer splits families across the whole store, so it demands
    /// evidence and a clear majority. A single sentence rarely has three
    /// function words at all, and the question asked of it is only whether
    /// it was written in the language it was supposed to be — where one
    /// `porque` is already an answer.
    pub fn leans_in(&self, text: &str) -> Option<&str> {
        let tokens = text
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| !token.is_empty())
            .map(fold_search_term)
            .collect::<Vec<_>>();
        self.leans(tokens.iter().map(String::as_str))
    }

    /// The same, over tokens the caller has already split and folded.
    pub fn leans<'a>(&self, tokens: impl IntoIterator<Item = &'a str>) -> Option<&str> {
        let mut counts = vec![0usize; self.languages.len()];
        for token in tokens {
            for (index, language) in self.languages.iter().enumerate() {
                if language.function_words.contains(token) {
                    counts[index] += 1;
                    break;
                }
            }
        }
        let (index, winner) = counts
            .iter()
            .enumerate()
            .max_by_key(|(index, count)| (**count, std::cmp::Reverse(*index)))
            .map(|(index, count)| (index, *count))?;
        let tied = counts
            .iter()
            .enumerate()
            .any(|(other, count)| other != index && *count == winner);
        if winner == 0 || tied {
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
        assert!(
            seen.contains(KERNEL_LANGUAGE),
            "the kernel's own language must be readable"
        );
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
    fn a_sentence_leans_on_one_function_word() {
        let vocabulary = LanguageVocabulary::shipped();

        assert_eq!(
            vocabulary.leans(words("lanzamiento pospuesto por la auditoria")),
            Some("spanish")
        );
        assert_eq!(
            vocabulary.leans(words("the reserve valve failed during the night shift")),
            Some("english")
        );
        assert_eq!(vocabulary.leans(words("launch postponed audit")), None);
        assert_eq!(vocabulary.leans(words("the launch fue postponed")), None);
        assert_eq!(
            vocabulary.leans_in("El despliegue de v0.7.0 se retrasó."),
            Some("spanish")
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
