use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use super::compound_identifiers::compound_forms;

/// The words of a text that could carry a search, in the form they are
/// compared in.
///
/// Stop words in either shipped language are dropped, single characters are
/// dropped unless they are digits, and what remains is folded. Stored text
/// is never changed by this; it only decides which of its words count.
///
/// An identifier that carries a digit or a `#` is also yielded whole, after
/// its parts (see [`compound_identifiers`](super::compound_identifiers)):
/// `C6.24` yields `c6`, `24` and `c6.24`, so it no longer meets `C6.4` on
/// everything it is made of. A whole form always keeps a joiner, which is how
/// the ranker tells it from a word and leaves it unstemmed.
pub fn informative_tokens(value: &str) -> impl Iterator<Item = String> + '_ {
    const STOP_WORDS: &[&str] = &[
        "a", "against", "an", "and", "are", "as", "at", "be", "because", "by", "came", "did", "do",
        "does", "earlier", "for", "from", "he", "how", "i", "if", "in", "is", "it", "me", "more",
        "my", "of", "on", "one", "or", "plus", "same", "should", "than", "the", "this", "to", "us",
        "use", "used", "uses", "was", "we", "were", "what", "when", "where", "which", "who", "why",
        "will", "with", "el", "la", "los", "las", "de", "al", "del", "donde", "en", "es", "lo",
        "no", "por", "para", "que", "se", "su", "un", "ya", "como", "cual", "cuando",
    ];
    value.split_whitespace().flat_map(|token| {
        token
            .split(|character: char| !character.is_alphanumeric())
            .map(fold_search_term)
            .filter(|term| {
                !term.is_empty()
                    && !STOP_WORDS.contains(&term.as_str())
                    && (term.chars().all(|character| character.is_ascii_digit()) || term.len() >= 2)
            })
            .chain(compound_forms(token))
    })
}

/// Produces the comparison form only. Stored evidence and returned query text
/// stay byte-exact; the ranker indexes this folded sibling so a phone or
/// foreign keyboard does not turn `válvula`, `arrêt`, `refrigeração`,
/// `Straße`, or `Kühlventil` into an unreachable memory.
pub fn fold_search_term(value: &str) -> String {
    let mut folded = String::with_capacity(value.len());
    for character in value
        .nfkd()
        .filter(|character| !is_combining_mark(*character))
    {
        match character {
            'ß' | 'ẞ' => folded.push_str("ss"),
            _ => folded.extend(character.to_lowercase()),
        }
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_removes_diacritics_and_case_without_touching_the_source() {
        let source = "Válvula Straße";

        assert_eq!(fold_search_term(source), "valvula strasse");
        assert_eq!(source, "Válvula Straße");
    }

    #[test]
    fn informative_tokens_drop_stop_words_and_keep_digits() {
        let tokens = informative_tokens("The valve #469 of la pasarela was 2 minutes late.")
            .collect::<Vec<_>>();

        assert_eq!(tokens, ["valve", "469", "pasarela", "2", "minutes", "late"]);
    }

    #[test]
    fn identifiers_are_yielded_whole_after_their_parts() {
        let tokens =
            informative_tokens("C6.24 cites C6.8+C6.9, v0.7.0 and #188.").collect::<Vec<_>>();

        assert_eq!(
            tokens,
            [
                "c6", "24", "c6.24", "cites", "c6", "8", "c6", "9", "c6.8", "c6.9", "v0", "7", "0",
                "0.7.0", "188"
            ]
        );
    }
}
