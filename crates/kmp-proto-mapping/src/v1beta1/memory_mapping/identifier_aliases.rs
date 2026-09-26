use std::collections::BTreeSet;

use serde::Deserialize;

use super::identifier_alias::IdentifierAlias;
use super::search_terms::fold_search_term;

/// Every kind of identifier a guide word introduces, read the same way in a
/// question and in the memories it is compared with.
///
/// A question that says `corte 10` asks about what the store writes as
/// `C10`, `corte 10` or `cut 10`; a question that says `ADR 18` asks about
/// `ADR-018`. Each spelling reads as one term (`c10`, `adr18`), added beside
/// the words the text already reads as, never in their place. Only the
/// anchored ask gate reads it: without the gate no text gains a term.
#[derive(Debug, Default, Deserialize)]
#[serde(transparent)]
pub(super) struct IdentifierAliases {
    aliases: Vec<IdentifierAlias>,
}

impl IdentifierAliases {
    /// The term `guide number` names, if `guide` introduces an identifier.
    pub(super) fn bound(&self, guide: &str, number: &str) -> Option<String> {
        self.aliases
            .iter()
            .find_map(|alias| alias.bound(guide, number))
    }

    /// The term one folded token names when it writes a kind and its number
    /// together (`adr-018`).
    pub(super) fn joined(&self, token: &str) -> Option<String> {
        self.aliases.iter().find_map(|alias| alias.joined(token))
    }

    /// Whether a folded word introduces an identifier of some kind.
    #[cfg(test)]
    pub(super) fn introduces(&self, word: &str) -> bool {
        self.aliases
            .iter()
            .any(|alias| alias.guide_words().contains(word))
    }

    /// The alias terms a text names, in the order it names them, once each:
    /// what indexing adds to a memory's words so the question's anchor finds
    /// it however it was spelled.
    pub(super) fn text_terms(&self, text: &str) -> Vec<String> {
        let tokens = text
            .split_whitespace()
            .map(|token| {
                let keep = |character: char| character.is_alphanumeric() || character == '#';
                fold_search_term(token.trim_matches(|character: char| !keep(character)))
            })
            .collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        let mut terms = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            let bound = index
                .checked_sub(1)
                .and_then(|previous| self.bound(&tokens[previous], token))
                .filter(|_| !is_year(token));
            let joined = self.joined(token).filter(|term| term != token);
            // A term the text reads as already (`185`, `c10`) is not added
            // again: counting it twice would weigh it twice.
            for term in bound.into_iter().chain(joined) {
                if term.chars().any(char::is_alphabetic) && seen.insert(term.clone()) {
                    terms.push(term);
                }
            }
        }
        terms
    }
}

/// A bare four-digit year is a date, whatever word precedes it.
pub(super) fn is_year(digits: &str) -> bool {
    digits.len() == 4
        && digits
            .parse::<u32>()
            .is_ok_and(|year| (1900..=2099).contains(&year))
}

#[cfg(test)]
mod tests {
    use super::super::question_contract_vocabulary::QuestionContractVocabulary;

    #[test]
    fn every_spelling_of_an_identifier_reads_as_one_term() {
        let aliases = QuestionContractVocabulary::shipped().identifier_aliases();
        assert_eq!(aliases.text_terms("El corte 10 cerró"), ["c10"]);
        assert_eq!(aliases.text_terms("Cut 10, then C10."), ["c10"]);
        assert_eq!(aliases.text_terms("Per ADR-018 and ADR 18"), ["adr18"]);
        assert_eq!(aliases.text_terms("Fase 3 y phase 3"), ["phase3"]);
        assert!(aliases.text_terms("PR 185 closed issue #185").is_empty());
        // A year is a date, and a word that introduces nothing binds nothing.
        assert!(aliases.text_terms("corte 2026 and page 10").is_empty());
        assert!(aliases.text_terms("C10 shipped").is_empty());
        assert!(aliases.introduces("corte"));
        assert!(!aliases.introduces("page"));
    }
}
