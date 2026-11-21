use std::sync::{Arc, OnceLock};

use super::lexical_row::LexicalRow;
use super::seed_documents::SeedDocuments;

/// The seed rows' direct fields of one indexed ask, read once per reading
/// and shared by every ranking of the ask (`SeedDocuments`).
///
/// A memo, not part of what the ask stands on: two of them always compare
/// equal, and a clone shares what was read.
#[derive(Clone, Default)]
pub struct IndexedSeeds {
    readings: Arc<[OnceLock<Arc<SeedDocuments>>; 2]>,
}

impl IndexedSeeds {
    /// The direct fields of `rows` under one reading, read the first time
    /// they are asked for.
    pub(super) fn of(&self, rows: &[LexicalRow], aliased: bool) -> Arc<SeedDocuments> {
        Arc::clone(self.readings[usize::from(aliased)].get_or_init(|| {
            Arc::new(SeedDocuments::new(
                rows.iter().map(|row| row.direct_counts(aliased)).collect(),
            ))
        }))
    }
}

impl PartialEq for IndexedSeeds {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for IndexedSeeds {}

impl std::fmt::Debug for IndexedSeeds {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("IndexedSeeds")
    }
}
