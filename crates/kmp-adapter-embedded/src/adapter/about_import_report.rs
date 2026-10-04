use std::collections::BTreeMap;

use super::about_import_outcome::AboutImportOutcome;

/// Outcome of incorporating exact abouts into a live store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutImportReport {
    /// Events appended to the destination log. Zero on a repeated run.
    pub events_imported: u64,
    /// Projection mutations applied for those events, in the same
    /// transaction that appended them.
    pub mutations_applied: u64,
    /// One outcome per requested about, keyed by the about exactly as given.
    pub abouts: BTreeMap<String, AboutImportOutcome>,
}
