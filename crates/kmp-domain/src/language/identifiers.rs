//! The tokens a faithful rendering in another language keeps exactly as they
//! are: a number, a version, a ticket, a path, an acronym. They are what the
//! lint of a search summary and the lint of a rendered question both check
//! for, so they are read in one place.

use std::collections::BTreeSet;

use super::fold_search_term;

/// The punctuation identifiers are joined with.
const JOINERS: &[char] = &['-', '_', '/', '.', ':'];

/// How many capitalized words in a row read as a writer raising their voice
/// rather than as a run of acronyms. `ADR`, `QA` and `PASS` arrive one or two
/// at a time inside ordinary prose; `BLOQUEANTE ANTES DE MIGRAR NADA` is
/// emphasis, and a faithful rendering translates it like any other words.
const EMPHASIS_RUN: usize = 3;

/// Identifier fidelity after folding dates and English possessives.
/// Keep this out of retrieval tokenization: stored words and search stay intact.
pub(crate) fn dropped_identifiers(text: &str, rendering: &str) -> Vec<String> {
    let normalized = |text: &str| {
        super::date_tokens::canonical_dates(
            fidelity_tokens(text)
                .into_iter()
                .map(fold_search_term)
                .collect(),
        )
    };
    // Decide whether a literal is an identifier before case folding (acronyms).
    let literal_ids = identifiers_in(&fidelity_tokens(text));
    let source = normalized(text);
    let carried = normalized(rendering)
        .into_iter()
        .flat_map(carried_forms)
        .collect::<BTreeSet<_>>();
    source
        .into_iter()
        .filter(|token| {
            literal_ids.contains(token)
                || token.bytes().any(|b| b.is_ascii_digit()) && is_identifier(token, false)
        })
        .filter(|identifier| {
            !carried.contains(identifier)
                && (!identifier.starts_with("--")
                    || !carried.iter().any(|candidate| {
                        super::date_tokens::carries_partial_date(identifier, candidate)
                    }))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// A text's tokens as fidelity reads them, in the order they were written.
fn fidelity_tokens(text: &str) -> Vec<&str> {
    text.split_whitespace().map(fidelity_token).collect()
}

/// A terminal possessive carries its whole identifier, never a substring.
/// Only fidelity uses this view; public tokens and stored text stay unchanged.
fn fidelity_token(token: &str) -> &str {
    let token = trim_edge_punctuation(token);
    ["'s", "’s", "'S", "’S"]
        .into_iter()
        .find_map(|suffix| token.strip_suffix(suffix))
        .filter(|base| is_identifier(base, false))
        .unwrap_or(token)
}

/// The identifiers a text carries, folded: the tokens a faithful rendering in
/// another language keeps exactly as they are.
pub fn identifiers(text: &str) -> BTreeSet<String> {
    identifiers_in(
        &text
            .split_whitespace()
            .map(trim_edge_punctuation)
            .collect::<Vec<_>>(),
    )
}

/// The identifiers of a text's tokens, read together rather than one at a
/// time. Whether capitals are an abbreviation or a raised voice is a fact
/// about a token's neighbours and not about its shape, so the reading needs
/// the sequence.
fn identifiers_in(tokens: &[&str]) -> BTreeSet<String> {
    tokens
        .iter()
        .zip(emphasis_flags(tokens))
        .filter(|(token, emphasis)| is_identifier(token, *emphasis))
        .map(|(token, _)| fold_search_term(token))
        .collect()
}

/// Which tokens sit inside a run of [`EMPHASIS_RUN`] or more capitalized words.
fn emphasis_flags(tokens: &[&str]) -> Vec<bool> {
    let shouted = tokens
        .iter()
        .map(|token| is_shouted(token))
        .collect::<Vec<_>>();
    let mut flags = vec![false; tokens.len()];
    let mut start = 0;
    while start < shouted.len() {
        if !shouted[start] {
            start += 1;
            continue;
        }
        let end = shouted[start..]
            .iter()
            .position(|shouted| !shouted)
            .map_or(shouted.len(), |offset| start + offset);
        if end - start >= EMPHASIS_RUN {
            flags[start..end].fill(true);
        }
        start = end;
    }
    flags
}

/// A word written entirely in capitals, with no digit and no joiner of its own
/// to say it is an identifier whatever its neighbours are doing.
fn is_shouted(token: &str) -> bool {
    token.chars().count() >= 2
        && token
            .chars()
            .all(|character| character.is_alphabetic() && character.is_uppercase())
}

/// Whether a token is something a translation copies rather than renders.
///
/// `emphasis` says the token sits inside a shouted phrase, where capitals are
/// the writer's voice. Everything else a token can be — a number, a tag, a
/// compound — it is on its own evidence, whatever its neighbours are.
fn is_identifier(token: &str, emphasis: bool) -> bool {
    if token.chars().count() < 2 {
        return false;
    }
    let has_digit = token.chars().any(|character| character.is_ascii_digit());
    let is_tagged = token.starts_with('#') || token.starts_with('@');
    let is_acronym = !emphasis
        && token
            .chars()
            .all(|character| character.is_ascii_uppercase())
        && !is_shouted_function_word(token);
    has_digit || is_tagged || is_acronym || is_compound(token)
}

/// `PERO`, `DE`: a word the kernel already reads as one a writer does not
/// choose is that word shouted, not an abbreviation that happens to spell it.
/// It is the one dictionary the kernel ships, so the reading stays the same on
/// every machine whether or not a lexical bridge is installed.
fn is_shouted_function_word(token: &str) -> bool {
    super::LanguageVocabulary::shipped().is_function_word(&fold_search_term(token))
}

/// `kmp-mcp`, `ref_boundary.rs`, `feat/lexical-bridge`: alphanumeric runs of
/// at least two characters joined by the punctuation identifiers are joined
/// with. An abbreviation such as `e.g.` or `p.ej.` is not one, because a
/// translation renders those, and neither is a slashed pair of ordinary words.
fn is_compound(token: &str) -> bool {
    let inner = token.trim_matches(JOINERS);
    let Some(runs) = compound_runs(inner) else {
        return false;
    };
    !is_slashed_word_pair(inner, &runs)
}

/// The runs of a compound, or nothing when the token is not one.
fn compound_runs(inner: &str) -> Option<Vec<&str>> {
    if !inner.contains(JOINERS) {
        return None;
    }
    let runs = inner.split(JOINERS).collect::<Vec<_>>();
    (runs.len() >= 2
        && runs.iter().all(|run| {
            run.chars().count() >= 2 && run.chars().all(|character| character.is_alphanumeric())
        }))
    .then_some(runs)
}

/// `entradas/salidas`, `cleanup/binding`, `input/output`, `and/or`: two
/// ordinary words either side of a slash are prose in both shipped languages,
/// and a faithful rendering translates them. What a translation copies says
/// more about itself than shape alone: a third run (`feat/valkey-store`), a
/// digit, or a run written as an acronym (`PASS/FAIL`).
fn is_slashed_word_pair(inner: &str, runs: &[&str]) -> bool {
    runs.len() == 2
        && inner.contains('/')
        && runs.iter().all(|run| {
            run.chars().all(char::is_alphabetic)
                && !run.chars().all(|character| character.is_ascii_uppercase())
        })
}

/// What a rendering token carries for fidelity: itself, and, when it is a
/// number joined to a unit word, that number.
///
/// English builds `90-minute` out of the very number a Spanish source wrote as
/// `90 minutos`, and refusing that rendering asks a writer to be less faithful,
/// not more. Only a whole run at the joiner is read and only in that one
/// grammatical shape, so nothing here matches a substring: `1990-minute`
/// carries `1990` and not `90`, a date keeps its parts to itself, and a ticket
/// such as `INC-42` still carries `INC-42` alone.
fn carried_forms(token: String) -> Vec<String> {
    match unit_adjective_number(&token) {
        Some(number) => vec![number.to_string(), token],
        None => vec![token],
    }
}

fn unit_adjective_number(token: &str) -> Option<&str> {
    let (number, unit) = token.split_once('-')?;
    let counted = number.len() >= 2 && number.bytes().all(|byte| byte.is_ascii_digit());
    let unit_word = unit.chars().count() >= 2 && unit.chars().all(char::is_alphabetic);
    (counted && unit_word).then_some(number)
}

fn trim_edge_punctuation(token: &str) -> &str {
    const EDGE_PUNCTUATION: &[char] = &[
        '.', ',', ';', ':', '!', '?', '¡', '¿', '(', ')', '[', ']', '{', '}', '"', '\'', '«', '»',
        '“', '”', '‘', '’', '`',
    ];
    token.trim_matches(EDGE_PUNCTUATION)
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
