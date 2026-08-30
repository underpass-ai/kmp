use std::path::{Path, PathBuf};

use crate::lifecycle::domain::store_facts::StoreFacts;

/// Outbound port for what the filesystem can say about memory stores.
///
/// User-scope stores all live under one directory, so finding every one of
/// them — including the orphans no rule reaches — costs a directory read and
/// no new state. Everything an implementation reports is an observation;
/// deciding what a store's placement means belongs to the survey.
pub trait StoreCatalog: Send + Sync {
    /// The one path the per-user default rule resolves to.
    fn user_default_store(&self) -> PathBuf;

    /// Every store directory under the user data home, orphans included.
    fn user_scope_stores(&self) -> Vec<PathBuf>;

    /// Whether a store exists at this path right now. A registry that lists
    /// a path that is gone is its own bug, so callers prune on this answer.
    fn is_store(&self, path: &Path) -> bool;

    /// The facts a survey reports about one store.
    fn store_facts(&self, path: &Path) -> StoreFacts;
}
