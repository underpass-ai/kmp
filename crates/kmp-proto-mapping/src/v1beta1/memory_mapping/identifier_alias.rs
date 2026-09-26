use std::collections::BTreeSet;

use serde::Deserialize;

/// One kind of identifier a guide word introduces, and the single term every
/// way of writing it reads as: `corte 10`, `cut 10` and `C10` are all `c10`.
///
/// Data, in `language/question_contract.json` (`identifier_aliases`). A
/// prefix of letters is written onto the number; `#` and `v` name the number
/// alone, which is the term `#185` and `v0.7.0` already read as.
#[derive(Debug, Deserialize)]
pub(super) struct IdentifierAlias {
    prefix: String,
    /// Folded words that say the number after them is one of these.
    guide_words: BTreeSet<String>,
}

impl IdentifierAlias {
    #[cfg(test)]
    pub(super) fn guide_words(&self) -> &BTreeSet<String> {
        &self.guide_words
    }

    /// Whether the prefix is written onto the number (`c10`, `adr18`) rather
    /// than dropped (`#185` is `185`).
    fn is_written_onto(&self) -> bool {
        self.prefix
            .chars()
            .all(|character| character.is_ascii_alphabetic())
            && !self.prefix.is_empty()
            && self.prefix != "v"
    }

    /// The term `guide number` names, when `guide` introduces this kind.
    pub(super) fn bound(&self, guide: &str, number: &str) -> Option<String> {
        if !self.guide_words.contains(guide) {
            return None;
        }
        let digits = number.trim_start_matches('#');
        let digits = if self.prefix == "v" {
            digits.strip_prefix('v').unwrap_or(digits)
        } else {
            digits
        };
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        Some(self.term(digits))
    }

    /// The term one folded token names when it writes the kind and its
    /// number together: `adr-018` and `adr018` are `adr18`, `c10` is `c10`.
    pub(super) fn joined(&self, token: &str) -> Option<String> {
        if !self.is_written_onto() {
            return None;
        }
        std::iter::once(self.prefix.as_str())
            .chain(self.guide_words.iter().map(String::as_str))
            .find_map(|head| {
                let rest = token.strip_prefix(head)?;
                let digits = rest.strip_prefix(['-', '.', '_']).unwrap_or(rest);
                (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
                    .then(|| self.term(digits))
            })
    }

    fn term(&self, digits: &str) -> String {
        if !self.is_written_onto() {
            return digits.to_string();
        }
        let significant = digits.trim_start_matches('0');
        let number = if significant.is_empty() {
            "0"
        } else {
            significant
        };
        format!("{}{number}", self.prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alias(prefix: &str, words: &[&str]) -> IdentifierAlias {
        IdentifierAlias {
            prefix: prefix.to_string(),
            guide_words: words.iter().map(|word| word.to_string()).collect(),
        }
    }

    #[test]
    fn a_guide_word_binds_the_number_after_it() {
        let corte = alias("c", &["corte", "cut"]);
        assert_eq!(corte.bound("corte", "10").as_deref(), Some("c10"));
        assert_eq!(corte.bound("cut", "05").as_deref(), Some("c5"));
        assert_eq!(corte.bound("issue", "10"), None);
        assert_eq!(corte.bound("corte", "c6.4"), None);

        let issue = alias("#", &["issue", "pr"]);
        assert_eq!(issue.bound("pr", "185").as_deref(), Some("185"));
        assert_eq!(issue.bound("issue", "#185").as_deref(), Some("185"));

        let release = alias("v", &["release"]);
        assert_eq!(release.bound("release", "v7").as_deref(), Some("7"));
    }

    #[test]
    fn a_joined_token_names_the_same_term() {
        let adr = alias("adr", &["adr"]);
        assert_eq!(adr.joined("adr-018").as_deref(), Some("adr18"));
        assert_eq!(adr.joined("adr018").as_deref(), Some("adr18"));
        assert_eq!(adr.joined("adr"), None);
        assert_eq!(adr.joined("adr-018b"), None);

        let corte = alias("c", &["corte"]);
        assert_eq!(corte.joined("c10").as_deref(), Some("c10"));
        assert_eq!(corte.joined("corte10").as_deref(), Some("c10"));
        assert_eq!(corte.joined("c6.4"), None);

        // `#` and `v` drop the prefix: nothing is joined to it.
        assert_eq!(alias("#", &["issue"]).joined("issue185"), None);
    }
}
