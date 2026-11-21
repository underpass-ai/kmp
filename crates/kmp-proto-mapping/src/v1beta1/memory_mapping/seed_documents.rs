use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::association_counts::AssociationCounts;
use super::association_index::AssociationIndex;
use super::lexical_field::LexicalField;
use super::term_counts::TermCounts;

/// What one question counted over the seed rows: its pairs, and the
/// associations weighed from them once the partners' df was read.
type Counted = (
    Arc<Option<AssociationCounts>>,
    Option<Arc<AssociationIndex>>,
);

/// The direct fields of an indexed ask's seed rows under one reading (P14),
/// and the associations already counted over them.
///
/// Every reading of one ask counts the store's associations over the same
/// seed rows, against the whole about's N and df: the plan, its prefixes,
/// and each top-k round. Counting the pairs reads only N and the df of the
/// question's words, and weighing them the df of the partners, which every
/// field of the ask holds once the plan has read them; so a question is
/// counted once and weighed once, and every later reading takes both. A
/// `SeedDocuments` belongs to one [`super::IndexedAsk`] and one reading.
#[derive(Debug, Default)]
pub(super) struct SeedDocuments {
    documents: Vec<TermCounts>,
    counted: Mutex<BTreeMap<Vec<(String, u32)>, Counted>>,
}

impl SeedDocuments {
    pub(super) fn new(documents: Vec<TermCounts>) -> Self {
        Self {
            documents,
            counted: Mutex::default(),
        }
    }

    fn key(question: &TermCounts) -> Vec<(String, u32)> {
        question
            .terms()
            .map(|term| (term.clone(), question.count(term)))
            .collect()
    }

    /// The pairs `question` counts over the seed rows
    /// ([`AssociationCounts::count`]), counted once; `field` must hold the
    /// whole about's N and the df of the question's words.
    pub(super) fn counts(
        &self,
        question: &TermCounts,
        field: &LexicalField,
    ) -> Arc<Option<AssociationCounts>> {
        let key = Self::key(question);
        if let Some((counts, _)) = self
            .counted
            .lock()
            .ok()
            .and_then(|counted| counted.get(&key).cloned())
        {
            return counts;
        }
        let counts = Arc::new(AssociationCounts::count(question, field, &self.documents));
        if let Ok(mut held) = self.counted.lock() {
            held.entry(key)
                .or_insert_with(|| (Arc::clone(&counts), None));
        }
        counts
    }

    /// [`AssociationIndex::for_question`] of `question` over the seed rows
    /// and `field`, which must also hold the df of every partner the
    /// question's pairs name; weighed once.
    pub(super) fn associations(
        &self,
        question: &TermCounts,
        field: &LexicalField,
    ) -> Arc<AssociationIndex> {
        let key = Self::key(question);
        if let Some(index) = self
            .counted
            .lock()
            .ok()
            .and_then(|counted| counted.get(&key).and_then(|(_, index)| index.clone()))
        {
            return index;
        }
        let counts = self.counts(question, field);
        let index = Arc::new(AssociationIndex::from_counts(
            counts.as_ref().as_ref(),
            field,
        ));
        if let Ok(mut held) = self.counted.lock()
            && let Some((_, kept)) = held.get_mut(&key)
        {
            *kept = Some(Arc::clone(&index));
        }
        index
    }
}
