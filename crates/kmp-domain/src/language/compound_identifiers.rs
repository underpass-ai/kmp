//! Identifiers read as whole search terms.
//!
//! Splitting `C6.24` at the dot leaves `c6` and `24`, and splitting `C6.4`
//! leaves `c6` and `4`: two different identifiers that share a prefix and
//! differ only in a number the search has no way to weigh as part of a name.
//! A question about `C6.24` then meets an entry about `C6.4` on the prefix and
//! is answered with confidence. Keeping the identifier whole as well gives the
//! ranker one more term, rare and exact, that only the right entry carries.
//!
//! The parts stay: a writer who typed `c6` still reaches every `C6.x`. The
//! whole form is added beside them, never in their place.

use std::collections::BTreeSet;

use super::identifiers;

/// Joins two identifiers that are named together, `C6.8+C6.9` or `C6.8,C6.9`.
const LIST_SEPARATORS: &[char] = &['+', ','];

/// The whole search terms a text's identifiers read as, folded.
///
/// This is the vocabulary [`informative_tokens`](super::informative_tokens)
/// adds to the parts it already yields, collected as a set: what a benchmark
/// probes and what an anchored question compares by document frequency.
pub fn compound_identifiers(text: &str) -> BTreeSet<String> {
    text.split_whitespace().flat_map(compound_forms).collect()
}

/// The whole search terms one whitespace-delimited token reads as.
///
/// Only an identifier that carries a digit or a `#` has one. Its folded form
/// is taken without the `#`, without the `v` that opens a version, split at
/// `+` and `,` (and at a slash between identifiers, `C6.4/C6.5`) into the
/// identifiers it names, and a range `C6.1-C6.4` keeps its two ends without
/// enumerating what lies between them. A hyphen left inside an identifier
/// reads as a dot: `c6-4` is how a slug writes `C6.4`, so both name `c6.4`. A form with no
/// joiner left in it is one of the token's own parts already (`#469` is
/// `469`) and is not repeated, so no word is counted twice.
pub(super) fn compound_forms(token: &str) -> Vec<String> {
    identifier_terms(token)
        .into_iter()
        .filter(|form| form.chars().any(|character| !character.is_alphanumeric()))
        .collect()
}

/// The search terms one whitespace-delimited token names as an identifier,
/// whole or not: `C6.8+C6.9` names `c6.8` and `c6.9`, `#188` names `188`,
/// `v0.7.0` names `0.7.0`, and a word without a digit or a `#` names none.
///
/// Every one of them is a term the ranker indexes, which is what lets a
/// question's anchor be looked up by document frequency: the whole forms are
/// the ones `compound_forms` adds, and a form with no joiner is one of the
/// token's own parts.
pub fn identifier_terms(token: &str) -> Vec<String> {
    let Some(identifier) = identifiers(token).into_iter().next() else {
        return Vec::new();
    };
    let identifier = without_possessive(&identifier);
    if !identifier.contains('#') && !identifier.bytes().any(|byte| byte.is_ascii_digit()) {
        return Vec::new();
    }
    identifier
        .split(LIST_SEPARATORS)
        .flat_map(slashed_list)
        .map(|named| named.trim_start_matches('#'))
        .flat_map(range_ends)
        .map(without_version_prefix)
        .filter(|form| !form.is_empty())
        .map(|form| form.replace('-', "."))
        .collect()
}

/// The identifiers a slash lists, `c6.4/c6.5` or `c6.1-c6.4/c6.8-c6.9`, or
/// the token itself when it is a path or a branch: a slashed list is one whose
/// every side carries a digit and opens with the same letters, the rule a
/// range follows, so `feat/p3-p4` and `crates/v1beta1/lib.rs` stay whole.
fn slashed_list(identifier: &str) -> Vec<&str> {
    let sides = identifier.split('/').collect::<Vec<_>>();
    let is_list = sides.len() >= 2
        && sides.iter().all(|side| has_digit(side))
        && sides
            .iter()
            .all(|side| letter_prefix(side) == letter_prefix(sides[0]));
    if is_list { sides } else { vec![identifier] }
}

/// `c6.4's` names `c6.4`.
fn without_possessive(identifier: &str) -> &str {
    ["'s", "’s"]
        .into_iter()
        .find_map(|suffix| identifier.strip_suffix(suffix))
        .unwrap_or(identifier)
}

/// The two ends of a range, or the identifier itself when it is not one.
///
/// A range is exactly two sides of a hyphen, each with a digit, opening with
/// the same letters: `c6.1-c6.4` and `1.2-1.4` are ranges; `kmp-469`,
/// `adr-018`, `c6-4`, `x86-64` and a date `2026-09-26` are identifiers of
/// their own.
fn range_ends(identifier: &str) -> Vec<&str> {
    let sides = identifier.split('-').collect::<Vec<_>>();
    let is_range = matches!(sides.as_slice(), [low, high]
        if has_digit(low) && has_digit(high) && letter_prefix(low) == letter_prefix(high));
    if is_range { sides } else { vec![identifier] }
}

fn has_digit(side: &str) -> bool {
    side.bytes().any(|byte| byte.is_ascii_digit())
}

fn letter_prefix(side: &str) -> &str {
    let end = side
        .char_indices()
        .find(|(_, character)| !character.is_alphabetic())
        .map_or(side.len(), |(index, _)| index);
    &side[..end]
}

/// `v0.7.0` is the version `0.7.0`.
fn without_version_prefix(identifier: &str) -> &str {
    identifier
        .strip_prefix('v')
        .filter(|rest| rest.starts_with(|character: char| character.is_ascii_digit()))
        .unwrap_or(identifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(token: &str) -> Vec<String> {
        compound_forms(token)
    }

    #[test]
    fn a_dotted_identifier_is_kept_whole() {
        assert_eq!(forms("C6.24"), ["c6.24"]);
        assert_eq!(forms("C6.4,"), ["c6.4"]);
        assert_eq!(forms("(C6.4)"), ["c6.4"]);
        assert_ne!(forms("C6.24"), forms("C6.4"));
    }

    #[test]
    fn a_hash_or_a_version_prefix_is_dropped() {
        assert_eq!(forms("v0.7.0"), ["0.7.0"]);
        assert_eq!(forms("0.7.0"), ["0.7.0"]);
        assert_eq!(forms("#c6.4"), ["c6.4"]);
        // `#469` is the part `469` already.
        assert!(forms("#469").is_empty());
        assert!(forms("v2").is_empty());
    }

    #[test]
    fn a_list_names_each_identifier() {
        assert_eq!(forms("C6.8+C6.9"), ["c6.8", "c6.9"]);
        assert_eq!(forms("C6.8,C6.9"), ["c6.8", "c6.9"]);
        // A decimal comma splits into parts the tokenizer already yields.
        assert!(forms("0,6").is_empty());
    }

    #[test]
    fn a_slash_between_identifiers_lists_them() {
        assert_eq!(forms("C6.4/C6.5"), ["c6.4", "c6.5"]);
        assert_eq!(
            forms("C6.1-C6.4/C6.8-C6.9"),
            ["c6.1", "c6.4", "c6.8", "c6.9"]
        );
        assert_eq!(forms("feat/p3-p4"), ["feat/p3.p4"]);
        assert_eq!(forms("crates/v1beta1/lib.rs"), ["crates/v1beta1/lib.rs"]);
    }

    #[test]
    fn a_range_keeps_its_ends_without_enumerating() {
        assert_eq!(forms("C6.1-C6.4"), ["c6.1", "c6.4"]);
        assert_eq!(forms("v0.7-v0.8"), ["0.7", "0.8"]);
        assert!(forms("10-20").is_empty());
    }

    #[test]
    fn a_hyphenated_identifier_is_not_a_range() {
        assert_eq!(forms("KMP-469"), ["kmp.469"]);
        assert_eq!(forms("ADR-018"), ["adr.018"]);
        assert_eq!(forms("c6-4"), forms("C6.4"));
        assert_eq!(forms("x86-64"), ["x86.64"]);
        assert_eq!(forms("2026-09-26"), ["2026.09.26"]);
    }

    #[test]
    fn words_without_a_digit_or_a_hash_have_none() {
        assert!(forms("kmp-mcp").is_empty());
        assert!(forms("feat/valkey-store").is_empty());
        assert!(forms("ADR").is_empty());
        assert!(forms("C6").is_empty());
        assert!(forms("9").is_empty());
    }

    #[test]
    fn a_possessive_names_its_identifier() {
        assert_eq!(forms("C6.4's"), ["c6.4"]);
    }

    #[test]
    fn an_identifier_names_terms_the_tokenizer_indexes() {
        assert_eq!(identifier_terms("#188"), ["188"]);
        assert_eq!(identifier_terms("C4"), ["c4"]);
        assert_eq!(identifier_terms("C6.8+C6.9"), ["c6.8", "c6.9"]);
        assert_eq!(identifier_terms("v0.7.0"), ["0.7.0"]);
        assert!(identifier_terms("adapter").is_empty());
        for token in ["#188", "C4", "C6.24", "v0.7.0", "KMP-469"] {
            let indexed = super::super::informative_tokens(token).collect::<Vec<_>>();
            for term in identifier_terms(token) {
                assert!(indexed.contains(&term), "{token} indexes {term}");
            }
        }
    }

    #[test]
    fn a_text_collects_every_identifier_it_names() {
        let found = compound_identifiers("C6.24 local execution adapter, see C6.8+C6.9 and #188.");
        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            ["c6.24", "c6.8", "c6.9"]
        );
    }
}
