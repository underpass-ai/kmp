use std::collections::HashMap;

use kmp_domain::language::informative_tokens;

/// The lexical field of one about: the words its entries are searched by,
/// and how many of those entries each word reaches.
///
/// It is built over the same tokens the ranker builds a question's field
/// from — informative words, folded — and over both halves of what a
/// question can land on, the stored text and the English summary attached to
/// it. That is what makes "this summary singles out nothing" a statement
/// about retrieval rather than about prose.
#[derive(Debug, Default, Clone)]
pub(crate) struct AboutLexicon {
    entries: usize,
    /// Folded informative term to the number of entries it occurs in.
    document_frequency: HashMap<String, usize>,
    /// A stored summary, byte for byte, to the number of entries carrying it.
    summary_copies: HashMap<String, usize>,
}

impl AboutLexicon {
    /// Adds one entry's searchable words to the field.
    pub(crate) fn admit(&mut self, text: &str, summary: Option<&str>) {
        self.entries += 1;
        let mut seen = std::collections::BTreeSet::new();
        for term in informative_tokens(text).chain(informative_tokens(summary.unwrap_or_default()))
        {
            seen.insert(term);
        }
        for term in seen {
            *self.document_frequency.entry(term).or_default() += 1;
        }
        if let Some(summary) = summary {
            *self.summary_copies.entry(summary.to_string()).or_default() += 1;
        }
    }

    pub(crate) fn entries(&self) -> usize {
        self.entries
    }

    /// How many other entries of this about carry this exact summary.
    pub(crate) fn other_entries_repeating(&self, summary: &str) -> usize {
        self.summary_copies
            .get(summary)
            .copied()
            .unwrap_or_default()
            .saturating_sub(1)
    }

    /// How many entries of the about, other than this one, a term reaches.
    ///
    /// The entry being judged is discounted, so a word it alone carries
    /// reaches nobody else and separates it perfectly.
    pub(crate) fn other_entries_reached(&self, term: &str, carried_here: bool) -> usize {
        self.document_frequency
            .get(term)
            .copied()
            .unwrap_or_default()
            .saturating_sub(usize::from(carried_here))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexicon(entries: &[(&str, Option<&str>)]) -> AboutLexicon {
        let mut lexicon = AboutLexicon::default();
        for (text, summary) in entries {
            lexicon.admit(text, *summary);
        }
        lexicon
    }

    #[test]
    fn a_word_repeated_across_entries_reaches_all_of_them() {
        let lexicon = lexicon(&[
            ("The valve froze.", None),
            ("The valve was replaced.", None),
            ("The auditors signed off.", None),
        ]);

        assert_eq!(lexicon.entries(), 3);
        assert_eq!(lexicon.other_entries_reached("valve", true), 1);
        assert_eq!(lexicon.other_entries_reached("auditors", true), 0);
        assert_eq!(lexicon.other_entries_reached("valve", false), 2);
    }

    #[test]
    fn a_summary_attached_to_several_entries_is_counted_once_per_entry() {
        let lexicon = lexicon(&[
            ("Primer hecho.", Some("The rollout was reviewed.")),
            ("Segundo hecho.", Some("The rollout was reviewed.")),
            ("Tercer hecho.", Some("The auditors signed off.")),
        ]);

        assert_eq!(
            lexicon.other_entries_repeating("The rollout was reviewed."),
            1
        );
        assert_eq!(
            lexicon.other_entries_repeating("The auditors signed off."),
            0
        );
        assert_eq!(lexicon.other_entries_repeating("never written"), 0);
    }

    /// The field counts entries, not occurrences: a word said twice in one
    /// memory still reaches one memory.
    #[test]
    fn a_word_repeated_inside_one_entry_still_reaches_one_entry() {
        let lexicon = lexicon(&[("The valve, the same valve, froze.", None)]);

        assert_eq!(lexicon.other_entries_reached("valve", false), 1);
    }
}
