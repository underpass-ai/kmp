use std::collections::BTreeSet;

use super::morphology::Morphology;
use super::search_terms::{informative_tokens, is_whole_identifier, search_key};

/// The parts of a question's identifiers, each bound to the whole identifiers
/// it was read out of.
///
/// A question about `C6.24` yields `c6`, `24` and the whole `c6.24`. An entry
/// about `C6.4` shares `c6` with it, and counting that as a word the question
/// and the entry have in common is how `C6.24 local execution adapter` was
/// answered with the `C6.4` adapter at high confidence: the prefix covered a
/// share of the question it never earned. A part of an identifier says
/// something only about that identifier, so an entry covers it only when it
/// names that identifier:
///
/// - whole (`C6.24`, or `c6-24` as a slug writes it); or
/// - written apart, every part of it present (`issue 188`, or an entry ref
///   slug `c6-8-c6-9` for `C6.8+C6.9`), and no other identifier made of the
///   part beside them — no twin to mistake it for.
///
/// A part the question also writes on its own (`C6 and C6.24`) is a word of
/// the question in its own right and stays free.
#[derive(Debug, Default)]
pub(super) struct IdentifierBinding {
    /// Per identifier token of the question: its parts and its whole forms.
    identifiers: Vec<(BTreeSet<String>, BTreeSet<String>)>,
    free: BTreeSet<String>,
}

impl IdentifierBinding {
    /// Reads which of a question's search keys are parts of its identifiers.
    pub(super) fn read(question: &str, morphology: &Morphology) -> Self {
        let mut identifiers = Vec::new();
        let mut free = BTreeSet::new();
        for token in question.split_whitespace() {
            let (wholes, parts): (BTreeSet<_>, BTreeSet<_>) = informative_tokens(token)
                .map(|term| search_key(&term, morphology))
                .partition(|term| is_whole_identifier(term));
            if wholes.is_empty() {
                free.extend(parts);
            } else {
                identifiers.push((parts, wholes));
            }
        }
        Self { identifiers, free }
    }

    /// The matched question terms an entry is credited with: every one of
    /// them, except a part of an identifier the entry does not name.
    pub(super) fn credited(
        &self,
        matched: BTreeSet<String>,
        evidence_terms: &BTreeSet<String>,
    ) -> BTreeSet<String> {
        matched
            .into_iter()
            .filter(|term| self.covers(term, evidence_terms))
            .collect()
    }

    fn covers(&self, term: &str, evidence_terms: &BTreeSet<String>) -> bool {
        if self.free.contains(term) {
            return true;
        }
        let mut binding = self
            .identifiers
            .iter()
            .filter(|(parts, _)| parts.contains(term))
            .peekable();
        binding.peek().is_none()
            || binding.any(|(parts, wholes)| {
                wholes.iter().any(|whole| evidence_terms.contains(whole))
                    || parts.is_subset(evidence_terms) && !names_a_twin(term, evidence_terms)
            })
    }
}

/// Whether an entry names a whole identifier made of this part. `v0` is the
/// part of a version the whole writes without its `v`.
fn names_a_twin(part: &str, evidence_terms: &BTreeSet<String>) -> bool {
    let bare = part
        .strip_prefix('v')
        .filter(|rest| rest.starts_with(|character: char| character.is_ascii_digit()));
    evidence_terms
        .iter()
        .filter(|term| is_whole_identifier(term))
        .flat_map(|whole| whole.split(|character: char| !character.is_alphanumeric()))
        .any(|component| component == part || Some(component) == bare)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn a_part_counts_only_where_its_identifier_is_whole() {
        let binding = IdentifierBinding::read("C6.24 local adapter", &Morphology::none());
        let matched = terms(&["c6", "local", "adapter"]);

        let twin = binding.credited(matched.clone(), &terms(&["c6", "4", "c6.4", "local"]));
        assert_eq!(twin, terms(&["local", "adapter"]));

        let same = binding.credited(matched.clone(), &terms(&["c6", "24", "c6.24"]));
        assert_eq!(same, matched);
    }

    #[test]
    fn a_part_is_covered_where_no_twin_is_named() {
        let binding =
            IdentifierBinding::read("What is stored about C6.8+C6.9?", &Morphology::none());
        let matched = terms(&["c6", "8", "9"]);

        // The identifiers written apart, as an entry ref slug writes them.
        let slug = terms(&["c6", "8", "9", "review"]);
        assert_eq!(binding.credited(matched.clone(), &slug), matched);
        // Some parts loose in the entry are not the identifier.
        let loose = terms(&["c6", "8", "review"]);
        assert!(binding.credited(terms(&["c6", "8"]), &loose).is_empty());
        // An entry naming either identifier of the list names the list.
        let named = terms(&["c6", "8", "c6.8"]);
        assert_eq!(binding.credited(matched, &named), terms(&["c6", "8", "9"]));
    }

    #[test]
    fn a_version_twin_takes_its_parts() {
        let binding = IdentifierBinding::read("v0.18.12 status", &Morphology::none());
        let credited = binding.credited(
            terms(&["v0", "18", "status"]),
            &terms(&["v0", "18", "2", "0.18.2", "status"]),
        );

        assert_eq!(credited, terms(&["status"]));
    }

    #[test]
    fn a_part_the_question_also_writes_alone_stays_free() {
        let binding = IdentifierBinding::read("C6 and C6.24", &Morphology::none());
        let credited = binding.credited(terms(&["c6"]), &terms(&["c6", "4", "c6.4"]));

        assert_eq!(credited, terms(&["c6"]));
    }

    #[test]
    fn a_question_without_whole_identifiers_binds_nothing() {
        let binding = IdentifierBinding::read("issue #188 preflight", &Morphology::none());
        let matched = terms(&["issue", "188", "preflight"]);

        assert_eq!(binding.credited(matched.clone(), &BTreeSet::new()), matched);
    }

    #[test]
    fn a_twin_of_one_listed_identifier_takes_only_the_parts_it_shares() {
        let binding = IdentifierBinding::read("C6.8+C6.9 status", &Morphology::none());
        let credited =
            binding.credited(terms(&["c6", "8", "9"]), &terms(&["c6", "8", "9", "c6.18"]));

        assert_eq!(credited, terms(&["8", "9"]));
    }
}
