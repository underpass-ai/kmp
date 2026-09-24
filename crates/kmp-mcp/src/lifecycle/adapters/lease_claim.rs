use std::path::PathBuf;

use super::store_lease_registry::release;

/// One claim on a store's lease, released when it is dropped.
pub(crate) struct LeaseClaim {
    pub(super) path: PathBuf,
}

impl Drop for LeaseClaim {
    fn drop(&mut self) {
        release(&self.path);
    }
}
