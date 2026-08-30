use super::store_size::StoreSize;
use super::store_storage::StoreStorage;

/// What a survey can report about one store by looking at it: what is inside,
/// how much of it, and when a session last started against it.
///
/// Reach is deliberately absent — it is decided by the survey from where the
/// path was found, never by looking at the store itself.
#[derive(Debug, Clone)]
pub struct StoreFacts {
    pub storage: Option<StoreStorage>,
    pub size: StoreSize,
    /// From the store's own rotating log; `None` when it never started.
    pub last_opened: Option<String>,
}
