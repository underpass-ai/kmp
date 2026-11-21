use super::indexed_field_stats::IndexedFieldStats;
use super::indexed_lifecycle::IndexedLifecycle;

/// What an ask answered from the lexical index stands on beside the
/// candidates it read (DESIGN L6, P13): the about's language, its collection
/// statistics under both readings (plain, and with the alias terms the
/// anchored gate reads) and its lifecycle. With these, ranking the
/// candidates the postings reached gives the bytes ranking the whole about
/// gives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexedAsk {
    pub language: Option<String>,
    pub plain: IndexedFieldStats,
    pub aliased: IndexedFieldStats,
    pub lifecycle: IndexedLifecycle,
    /// The whole about's vocabulary, when the installation bridges
    /// languages: what the table bridges the question against.
    pub vocabulary: Option<std::sync::Arc<Vec<String>>>,
    /// The row of every candidate that carries a word of the question in
    /// any form (P14): the store's associations are counted over them, so
    /// leaving a candidate below the floor unread changes no weight.
    pub seed_rows: Option<std::sync::Arc<Vec<super::lexical_row::LexicalRow>>>,
    /// The row of every candidate the plan may read, by candidate id: the
    /// counts its surfaces were indexed with, which are the counts the
    /// ranker reads (both come from `surface_counts`), so the ranker takes
    /// them from here instead of tokenizing the candidate again.
    pub rows:
        Option<std::sync::Arc<std::collections::BTreeMap<String, super::lexical_row::LexicalRow>>>,
    /// The seed rows' direct fields and associations, read once for every
    /// ranking of the ask.
    pub seeds: super::indexed_seeds::IndexedSeeds,
}

impl IndexedAsk {
    /// The direct field of every seed row under one reading, with the
    /// associations counted over them.
    pub(super) fn seed_documents(
        &self,
        aliased: bool,
    ) -> Option<std::sync::Arc<super::seed_documents::SeedDocuments>> {
        self.seed_rows
            .as_ref()
            .map(|rows| self.seeds.of(rows, aliased))
    }

    pub(super) fn stats(&self, aliased: bool) -> &IndexedFieldStats {
        if aliased { &self.aliased } else { &self.plain }
    }
}
