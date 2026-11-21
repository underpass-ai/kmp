use std::collections::BTreeMap;

use super::association_index::{
    BOUND_MARGIN, MINIMUM_ASSOCIATION, MINIMUM_DOCUMENTS, MINIMUM_PAIR_COUNT,
};
use super::lexical_field::LexicalField;
use super::term_counts::TermCounts;

/// The pairs [`super::association_index::AssociationIndex::for_question`]
/// counts for a question: for each question term whose bound can clear the
/// bar, every word it shares at least `MINIMUM_PAIR_COUNT` documents with,
/// and in how many, with the document total.
///
/// Counting needs only N and the df of the question's own terms; weighing
/// the pairs then needs the df of every partner. Kept apart, an ask answered
/// from the lexical index reads the df of exactly those partners (P14).
#[derive(Debug)]
pub(super) struct AssociationCounts {
    total: f64,
    pairs: BTreeMap<String, Vec<(String, usize)>>,
}

impl AssociationCounts {
    /// The pairs over `documents`; `None` when nothing is counted (too few
    /// documents, or no question term whose bound could clear the bar).
    ///
    /// A term is skipped before counting when no partner could clear the
    /// bar: with c(t,x) ≤ df(x), PMI(t,x) = ln(c·N / (df(t)·df(x))) ≤
    /// ln(N/df(t)). The margin keeps a rounding in either computation from
    /// skipping a term the whole-store build would keep.
    pub(super) fn count<'a>(
        question: &TermCounts,
        field: &LexicalField,
        documents: impl IntoIterator<Item = &'a TermCounts>,
    ) -> Option<Self> {
        if field.documents() < MINIMUM_DOCUMENTS {
            return None;
        }
        let total = field.documents() as f64;
        let asked = question
            .terms()
            .map(String::as_str)
            .filter(|term| {
                let frequency = field.document_frequency(term);
                frequency >= MINIMUM_PAIR_COUNT
                    && (total / frequency as f64).ln() >= MINIMUM_ASSOCIATION - BOUND_MARGIN
            })
            .collect::<Vec<_>>();
        if asked.is_empty() {
            return None;
        }
        let mut together = asked
            .iter()
            .map(|term| (*term, BTreeMap::<&str, usize>::new()))
            .collect::<BTreeMap<_, _>>();
        for document in documents {
            for (term, partners) in &mut together {
                if document.count(term) == 0 {
                    continue;
                }
                for other in document.terms() {
                    if other != *term {
                        *partners.entry(other.as_str()).or_default() += 1;
                    }
                }
            }
        }
        let pairs = together
            .into_iter()
            .map(|(term, partners)| {
                (
                    term.to_string(),
                    partners
                        .into_iter()
                        .filter(|(_, count)| *count >= MINIMUM_PAIR_COUNT)
                        .map(|(other, count)| (other.to_string(), count))
                        .collect(),
                )
            })
            .collect();
        Some(Self { total, pairs })
    }

    pub(super) fn total(&self) -> f64 {
        self.total
    }

    /// Each counted question term with its partners, ascending.
    pub(super) fn pairs(&self) -> &BTreeMap<String, Vec<(String, usize)>> {
        &self.pairs
    }

    /// Every partner: the words whose df weighing the pairs reads.
    pub(super) fn partners(&self) -> impl Iterator<Item = &String> {
        self.pairs
            .values()
            .flat_map(|partners| partners.iter().map(|(other, _)| other))
    }
}
