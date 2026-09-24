use std::path::Path;

use super::lease_claim::LeaseClaim;
use super::lease_claim_error::LeaseClaimError;
use super::store_lease_registry::claim;

/// A shared claim held for the lifetime of one embedded MCP host.
///
/// SQLite deliberately lets several hosts share a store. The shared file lock
/// preserves that contract while giving selective uninstall one
/// cross-platform operation that can prove no host still owns the path
/// before removing it.
pub struct StoreSessionLease {
    _claim: LeaseClaim,
}

impl StoreSessionLease {
    /// Claim a store for this host without excluding other hosts.
    pub fn acquire(data_home: &Path, store: &Path) -> Result<Self, String> {
        match claim(data_home, store, false) {
            Ok(claim) => Ok(Self { _claim: claim }),
            Err(LeaseClaimError::Busy) => Err(format!(
                "store `{}` is being removed; this host did not open it. Retry after uninstall finishes",
                store.display()
            )),
            Err(LeaseClaimError::Io(error)) => Err(error),
        }
    }
}
