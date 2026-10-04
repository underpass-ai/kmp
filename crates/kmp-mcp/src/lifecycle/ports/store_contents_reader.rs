use std::path::Path;

use crate::lifecycle::domain::store_contents::StoreContents;

/// Outbound port for what is inside a store, read without using it.
///
/// An implementation must never take the store's session lease, migrate it,
/// or write to it: the inventory looks at every store on the machine,
/// including ones a live session holds and ones this engine cannot open.
pub trait StoreContentsReader: Send + Sync {
    fn contents(&self, store: &Path) -> StoreContents;
}
