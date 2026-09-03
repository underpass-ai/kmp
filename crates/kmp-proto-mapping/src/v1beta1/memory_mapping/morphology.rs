use std::borrow::Cow;

use kmp_domain::language::LanguageVocabulary;
use rust_stemmers::{Algorithm, Stemmer};

use super::search_terms::fold_search_term;

/// Folds a word onto its stem, so a memory written once can be found by the
/// other shapes of the same word.
///
/// Before this, `desplegamos` did not reach `desplegado`, and `deployed` did
/// not reach `deployment`. What did work was whatever someone had written into
/// the hand-kept concept table — seventeen families, all of them English. The
/// table works exactly where somebody thought of it and nowhere else.
///
/// Two decisions keep this from breaking what already works.
///
/// It runs *under* the concept table, on the words the table leaves alone, so
/// every family the table already unifies keeps behaving exactly as it did.
///
/// And the language is decided once, from the stored memory, then used for
/// both sides of every comparison. Choosing per word does not work: the
/// Spanish rules reduce `release` to `rel` while both languages reduce
/// `released` to `releas`, so picking a stemmer word by word splits families
/// instead of joining them.
///
/// Two limits are worth knowing rather than discovering. Snowball strips
/// suffixes and does not undo the Spanish diphthong, so `despliegue` and
/// `desplegamos` stay apart. And terms arrive here with their diacritics
/// already folded away — which is what lets someone type `valvula` for
/// `válvula` — so the Spanish suffixes written with an accent, `-ción` above
/// all, are no longer there for the algorithm to find. Both gaps are
/// synonymy-shaped, and belong to a layer above this one.
pub(super) struct Morphology {
    stemmer: Option<Stemmer>,
}

/// A memory nobody has read yet stems nothing, which is the same answer as a
/// memory whose language cannot be read.
impl Default for Morphology {
    fn default() -> Self {
        Self::none()
    }
}

impl std::fmt::Debug for Morphology {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Morphology")
            .field("reading_a_language", &self.stemmer.is_some())
            .finish()
    }
}

impl Morphology {
    /// Nothing is stemmed. What a store whose language cannot be read gets,
    /// and what every caller got before this existed.
    pub(super) fn none() -> Self {
        Self { stemmer: None }
    }

    /// Reads the language of a memory, and takes the stemmer for it.
    ///
    /// A language the vocabulary can name but this has no stemmer for reads as
    /// no language at all, which leaves the memory exactly as written rather
    /// than stemmed by somebody else's rules. Callers in the ranker read the
    /// three fields separately; this stays as the single-field reader the
    /// tests drive.
    #[cfg(test)]
    pub(super) fn read<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        Self::for_language(Self::read_language(texts).as_deref())
    }

    /// Which language a body of text is written in, for a caller that reads
    /// several fields and must decide the stemmer of each one separately.
    pub(super) fn read_language<'a>(texts: impl IntoIterator<Item = &'a str>) -> Option<String> {
        let tokens = texts
            .into_iter()
            .flat_map(|text| text.split(|character: char| !character.is_alphanumeric()))
            .map(fold_search_term)
            .collect::<Vec<_>>();
        LanguageVocabulary::shipped()
            .read(tokens.iter().map(String::as_str))
            .map(str::to_string)
    }

    /// The stemmer for a named language, or none for an unnamed one or one
    /// with no stemmer. This is how a caller stems one field in a language it
    /// read elsewhere: the English search summary in English, whatever the
    /// store's own language, and the question in the store's language or, when
    /// the store reads as none, in the kernel's search language.
    pub(super) fn for_language(language: Option<&str>) -> Self {
        Self {
            stemmer: language.and_then(Self::stemmer_for).map(Stemmer::create),
        }
    }

    fn stemmer_for(language: &str) -> Option<Algorithm> {
        match language {
            "spanish" => Some(Algorithm::Spanish),
            "english" => Some(Algorithm::English),
            _ => None,
        }
    }

    /// The stem of a word, or the word itself when no language was read.
    pub(super) fn stem<'a>(&self, term: &'a str) -> Cow<'a, str> {
        match &self.stemmer {
            Some(stemmer) => stemmer.stem(term),
            None => Cow::Borrowed(term),
        }
    }

    /// Whether a language was read at all. A caller reads it to decide the
    /// fallback for another field: a question folds in the kernel's search
    /// language exactly when the store's own language could not be read.
    #[cfg(test)]
    pub(super) fn is_reading_a_language(&self) -> bool {
        self.stemmer.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_memory_reaches_the_other_shapes_of_a_regular_verb() {
        let morphology = Morphology::read([
            "El despliegue de la pasarela se congelo durante la auditoria.",
            "La reunion semanal se movio a las diez de la manana.",
        ]);

        assert!(morphology.is_reading_a_language());
        assert_eq!(morphology.stem("congelar"), morphology.stem("congelo"));
        assert_eq!(morphology.stem("congelado"), morphology.stem("congelo"));
        assert_eq!(morphology.stem("valvulas"), morphology.stem("valvula"));
        assert_eq!(morphology.stem("reuniones"), morphology.stem("reunion"));
        assert_eq!(morphology.stem("auditorias"), morphology.stem("auditoria"));
    }

    /// Snowball strips suffixes; it does not undo the Spanish diphthong. This
    /// is the honest limit of stemming, and the reason a synonym layer above
    /// it still has work to do.
    #[test]
    fn a_stem_changing_spanish_verb_stays_split() {
        let morphology = Morphology::read([
            "El despliegue de la pasarela se congelo durante la auditoria.",
            "La reunion semanal se movio a las diez.",
        ]);

        assert_ne!(
            morphology.stem("despliegue"),
            morphology.stem("desplegamos")
        );
    }

    /// Diacritics are folded before a term reaches the stemmer, so a suffix
    /// the Spanish algorithm only recognises with its accent is gone by then.
    /// The trade is deliberate: folding is what lets a phone keyboard find
    /// `válvula`, and losing it would cost more than `-ción` does.
    #[test]
    fn a_spanish_suffix_written_with_an_accent_is_beyond_reach() {
        let morphology = Morphology::read([
            "El despliegue de la pasarela se congelo durante la auditoria.",
            "La reunion semanal se movio a las diez.",
        ]);

        assert_ne!(morphology.stem("congelacion"), morphology.stem("congelo"));
    }

    #[test]
    fn english_memory_reaches_the_noun_from_the_verb() {
        let morphology = Morphology::read([
            "The deployment of the gateway was frozen during the audit.",
            "The weekly meeting moved to ten in the morning.",
        ]);

        assert!(morphology.is_reading_a_language());
        assert_eq!(morphology.stem("deployment"), morphology.stem("deployed"));
        assert_eq!(morphology.stem("meetings"), morphology.stem("meeting"));
    }

    #[test]
    fn a_memory_too_short_to_read_is_left_exactly_as_written() {
        let morphology = Morphology::read(["Valkey."]);

        assert!(!morphology.is_reading_a_language());
        assert_eq!(morphology.stem("deployment"), "deployment");
        assert_eq!(morphology.stem("desplegamos"), "desplegamos");
    }

    #[test]
    fn an_evenly_mixed_memory_keeps_exact_matching() {
        let morphology = Morphology::read([
            "The deployment of the gateway was frozen and the audit was in the way.",
            "El despliegue de la pasarela se congelo por la auditoria de la semana.",
        ]);

        assert!(!morphology.is_reading_a_language());
        assert_eq!(morphology.stem("deployment"), "deployment");
    }

    #[test]
    fn a_named_stemmer_folds_that_language_whatever_the_store_reads() {
        let english = Morphology::for_language(Some("english"));

        assert!(english.is_reading_a_language());
        assert_eq!(english.stem("valves"), english.stem("valve"));
        assert_eq!(english.stem("shifts"), english.stem("shift"));
    }

    #[test]
    fn for_language_takes_a_language_read_from_another_field() {
        let language = Morphology::read_language(["El despliegue de la pasarela se congelo."]);
        assert_eq!(language.as_deref(), Some("spanish"));

        let spanish = Morphology::for_language(language.as_deref());
        assert_eq!(spanish.stem("valvulas"), spanish.stem("valvula"));

        let unread = Morphology::for_language(None);
        assert_eq!(unread.stem("valvulas"), "valvulas");
    }

    #[test]
    fn nothing_is_stemmed_without_a_language() {
        let morphology = Morphology::none();

        assert!(!morphology.is_reading_a_language());
        assert_eq!(morphology.stem("deployments"), "deployments");
        assert_eq!(Morphology::default().stem("valvulas"), "valvulas");
    }
}
