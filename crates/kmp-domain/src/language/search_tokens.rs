use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// The words of a text that could carry a search, in the form they are
/// compared in.
///
/// Stop words in either shipped language are dropped, single characters are
/// dropped unless they are digits, and what remains is folded. Stored text
/// is never changed by this; it only decides which of its words count.
pub fn informative_tokens(value: &str) -> impl Iterator<Item = String> + '_ {
    const STOP_WORDS: &[&str] = &[
        "a", "against", "an", "and", "are", "as", "at", "be", "because", "by", "came", "did", "do",
        "does", "earlier", "for", "from", "he", "how", "i", "if", "in", "is", "it", "me", "more",
        "my", "of", "on", "one", "or", "plus", "same", "should", "than", "the", "this", "to", "us",
        "use", "used", "uses", "was", "we", "were", "what", "when", "where", "which", "who", "why",
        "will", "with", "el", "la", "los", "las", "de", "al", "del", "donde", "en", "es", "lo",
        "no", "por", "para", "que", "se", "su", "un", "ya", "como", "cual", "cuando",
    ];
    value
        .split(|character: char| !character.is_alphanumeric())
        .map(fold_search_term)
        .filter(|term| {
            !term.is_empty()
                && !STOP_WORDS.contains(&term.as_str())
                && (term.chars().all(|character| character.is_ascii_digit()) || term.len() >= 2)
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
}
