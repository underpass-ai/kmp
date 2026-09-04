//! The proper names a text carries: the tokens a reader takes for the name
//! of a thing rather than a word of the language — `Valkey`, `Rachel`,
//! `Denver` — read off their shape, since nothing here knows the world.

use std::collections::BTreeSet;

/// The proper names of a text: a word written with an initial capital and
/// the rest in lower case, standing anywhere but at the start of a sentence,
/// where every word is capitalised and the shape says nothing. Tokens an
/// identifier already claims — acronyms, versions, tickets, compounds — are
/// left to the identifier reading.
pub fn proper_names(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut sentence_start = true;
    for raw in text.split_whitespace() {
        let token = raw.trim_matches(|character: char| {
            !(character.is_alphanumeric() || matches!(character, '#' | '@' | '-' | '_' | '/'))
        });
        if token.is_empty() {
            continue;
        }
        if !sentence_start && is_proper_name(token) {
            names.insert(token.to_string());
        }
        sentence_start = raw.ends_with(['.', '!', '?', ':']);
    }
    names
}

fn is_proper_name(token: &str) -> bool {
    let mut characters = token.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    let rest = characters.as_str();
    first.is_uppercase()
        && !rest.is_empty()
        && rest
            .chars()
            .all(|character| character.is_lowercase() || character == '\'')
}

#[cfg(test)]
mod tests {
    use super::proper_names;

    #[test]
    fn a_capitalised_word_inside_a_sentence_is_a_name_and_the_first_word_is_not() {
        let names = proper_names(
            "The shared cache runs on Valkey for the checkout service. Rachel moved to Denver: Austin came later.",
        );
        // `Rachel` opens a sentence and `Austin` follows a colon: at a
        // sentence start the capital says nothing, so neither is read.
        assert_eq!(
            names.into_iter().collect::<Vec<_>>(),
            vec!["Denver", "Valkey"]
        );
    }

    #[test]
    fn acronyms_versions_and_tickets_are_identifiers_not_names() {
        let names = proper_names("We adopted ADR-018 and v0.7.0 after the KMP review of #469.");
        assert!(names.is_empty(), "{names:?}");
    }
}
