use std::borrow::Cow;

use rust_stemmers::{Algorithm, Stemmer};

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
#[derive(Default)]
pub(super) struct Morphology {
    stemmer: Option<Stemmer>,
}

impl std::fmt::Debug for Morphology {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Morphology")
            .field("reading_a_language", &self.stemmer.is_some())
            .finish()
    }
}

/// Enough evidence to be reading a language rather than guessing at one, and
/// a clear enough majority — two in three — that one language is not having
/// the other's rules applied to it.
const MINIMUM_SIGNALS: usize = 3;

impl Morphology {
    /// Nothing is stemmed. What a store whose language cannot be read gets,
    /// and what every caller got before this existed.
    pub(super) fn none() -> Self {
        Self { stemmer: None }
    }

    /// Reads the language of a memory from the function words in it.
    ///
    /// Function words are the right signal because they are the ones a writer
    /// does not choose: a Spanish sentence carries `de`, `que`, `la` whatever
    /// it is about, and its nouns may well be English product names.
    pub(super) fn read<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        let mut spanish = 0usize;
        let mut english = 0usize;
        for text in texts {
            for token in text
                .split(|character: char| !character.is_alphanumeric())
                .map(super::answer_ranker::fold_search_term)
            {
                if SPANISH_FUNCTION_WORDS.contains(&token.as_str()) {
                    spanish += 1;
                } else if ENGLISH_FUNCTION_WORDS.contains(&token.as_str()) {
                    english += 1;
                }
            }
        }

        let (winner, total) = (spanish.max(english), spanish + english);
        if winner < MINIMUM_SIGNALS || winner * 3 < total * 2 {
            // Too little to read, or too even to call. A store that mixes two
            // languages evenly keeps exact matching rather than having one of
            // them stemmed by the other's rules.
            return Self::none();
        }
        Self {
            stemmer: Some(Stemmer::create(if spanish > english {
                Algorithm::Spanish
            } else {
                Algorithm::English
            })),
        }
    }

    /// The stem of a word, or the word itself when no language was read.
    pub(super) fn stem<'a>(&self, term: &'a str) -> Cow<'a, str> {
        match &self.stemmer {
            Some(stemmer) => stemmer.stem(term),
            None => Cow::Borrowed(term),
        }
    }

    /// Whether a language was read at all. The behaviour it gates is visible
    /// in what gets matched, so only the tests ask directly.
    #[cfg(test)]
    fn is_reading_a_language(&self) -> bool {
        self.stemmer.is_some()
    }
}

/// Function words that belong to one language and not the other. `a` and `no`
/// are in both and are left out; a word that cannot tell the two apart cannot
/// help decide between them.
const SPANISH_FUNCTION_WORDS: &[&str] = &[
    "al", "como", "con", "cual", "cuando", "de", "del", "donde", "el", "en", "es", "esta", "este",
    "fue", "ha", "hay", "la", "las", "lo", "los", "mas", "para", "pero", "por", "porque", "que",
    "se", "ser", "si", "sin", "sobre", "su", "sus", "un", "una", "y", "ya",
];

const ENGLISH_FUNCTION_WORDS: &[&str] = &[
    "about", "after", "against", "an", "and", "are", "as", "at", "be", "because", "been", "before",
    "but", "by", "did", "does", "for", "from", "had", "has", "have", "how", "if", "in", "into",
    "is", "it", "its", "of", "on", "or", "our", "than", "that", "the", "their", "then", "there",
    "this", "to", "was", "were", "what", "when", "where", "which", "who", "why", "will", "with",
];

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
    fn nothing_is_stemmed_without_a_language() {
        let morphology = Morphology::none();

        assert!(!morphology.is_reading_a_language());
        assert_eq!(morphology.stem("deployments"), "deployments");
        assert_eq!(Morphology::default().stem("valvulas"), "valvulas");
    }
}
