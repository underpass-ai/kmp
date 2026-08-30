use std::path::PathBuf;

use super::store_reach::StoreReach;
use super::store_size::StoreSize;
use super::store_storage::StoreStorage;

/// One memory this machine can be shown to hold: where it is, whether any
/// rule still reaches it, and the facts a person needs to decide its fate.
#[derive(Debug, Clone)]
pub struct MemoryRecord {
    pub path: PathBuf,
    pub reach: StoreReach,
    pub storage: Option<StoreStorage>,
    pub size: StoreSize,
    /// When a session last started against it, from the store's own log.
    pub last_opened: Option<String>,
}
