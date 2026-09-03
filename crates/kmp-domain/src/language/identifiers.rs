//! The tokens a faithful rendering in another language keeps exactly as they
//! are: a number, a version, a ticket, a path, an acronym. They are what the
//! lint of a search summary and the lint of a rendered question both check
//! for, so they are read in one place.

use std::collections::BTreeSet;

use super::fold_search_term;

/// The identifiers a text carries, folded: the tokens a faithful rendering in
/// another language keeps exactly as they are.
pub fn identifiers(text: &str) -> BTreeSet<String> {
    text.split_whitespace()
        .map(trim_edge_punctuation)
        .filter(|token| is_identifier(token))
        .map(fold_search_term)
        .collect()
}

/// Every whitespace-delimited token of a text, trimmed of the punctuation
/// that closes a clause and folded, so `#469,` and `(#469)` both carry `#469`.
pub fn surface_tokens(text: &str) -> BTreeSet<String> {
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

    #[test]
    fn identifiers_are_the_tokens_a_translation_copies() {
        let found = identifiers(
            "Se adoptó Valkey 7.2 en la rama feat/valkey-store (#469, ADR-018); ver kmp-mcp.",
        );

        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            ["#469", "7.2", "adr-018", "feat/valkey-store", "kmp-mcp"]
        );
    }

    #[test]
    fn an_abbreviation_or_a_single_character_is_not_one() {
        assert!(
            identifiers("Los adaptadores (p.ej. el de Valkey) se registran al arrancar, a las 19.")
                .contains("19")
        );
        // A single character is never one, digit or not: `x`, `y`, `9`.
        assert!(!identifiers("Se apagó a las 9.").contains("9"));
        assert!(
            !identifiers("Los adaptadores, p.ej. el de Valkey, se registran.")
                .iter()
                .any(|id| id.contains("ej"))
        );
        assert!(identifiers("Un solo carácter: x, y o z.").is_empty());
    }

    #[test]
    fn surface_tokens_drop_clause_punctuation_and_fold() {
        let tokens = surface_tokens("The valve (#469) froze, ADR-018 says.");

        assert!(tokens.contains("#469"));
        assert!(tokens.contains("adr-018"));
        assert!(tokens.contains("valve"));
    }
}
