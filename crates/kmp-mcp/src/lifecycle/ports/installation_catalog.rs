use std::path::{Path, PathBuf};

use crate::lifecycle::domain::store_size::StoreSize;

/// Outbound port for what the filesystem can say about an installation.
///
/// Every method is an observation or one bounded mutation of exactly the
/// path it is given. Where to look, what a finding means and what may be
/// removed are policy, and policy stays in the use cases.
pub trait InstallationCatalog: Send + Sync {
    fn is_file(&self, path: &Path) -> bool;

    fn is_directory(&self, path: &Path) -> bool;

    fn exists(&self, path: &Path) -> bool;

    /// One canonical identity for a path, or the io error's own words.
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, String>;

    /// Bytes under the path; files count themselves, symlinks are not walked.
    fn size_of(&self, path: &Path) -> StoreSize;

    /// The store's FORMAT_VERSION stamp, trimmed. `None` when unreadable.
    fn store_stamp(&self, store: &Path) -> Option<String>;

    /// The names of a directory's entries, sorted. Empty when unreadable.
    fn entry_names(&self, directory: &Path) -> Vec<String>;

    /// The plain files directly inside a directory, sorted.
    fn files_in(&self, directory: &Path) -> Vec<PathBuf>;

    fn file_mentions(&self, path: &Path, needle: &str) -> bool;

    /// The event count a bundle's header declares; reading it never opens
    /// the store.
    fn bundle_event_count(&self, bundle: &Path) -> Option<u64>;

    /// Remove exactly this path — a file or a whole directory — and never
    /// walk outward from it.
    fn remove_path(&self, path: &Path) -> Result<(), String>;
}
