//! Store migration, composed.
//!
//! The machinery lives in the storage adapter; what belongs here is the one
//! decision it deliberately does not make — how a context event becomes
//! projection mutations. That derivation is the application's, and injecting
//! it here is what keeps the adapter free of the layer above it and the MCP
//! binary free of the layer below.

use std::path::Path;

use kmp_adapter_embedded::{EmbeddedKernelStore, StoreMigrationReceipt};
use kmp_domain::PortError;

/// Migrates `source_dir` into `destination_dir`, returning what was moved.
///
/// The source is never opened for writing and its bytes are verified
/// unchanged when the migration finishes; the destination must not already
/// hold a store.
pub async fn migrate_data_dir(
    source_dir: &Path,
    destination_dir: &Path,
) -> Result<StoreMigrationReceipt, PortError> {
    let (_store, receipt) = EmbeddedKernelStore::migrate_data_dir(
        source_dir,
        destination_dir,
        kmp_application::projection_mutations_for_context_event,
    )
    .await?;
    Ok(receipt)
}

/// Migrate once, reopen afterwards. Safe on every start: a destination that
/// already holds a store is opened as it is, and the returned receipt says
/// whether this call was the one that migrated it.
pub async fn open_or_migrate_data_dir(
    source_dir: &Path,
    destination_dir: &Path,
) -> Result<Option<StoreMigrationReceipt>, PortError> {
    let (_store, receipt) = EmbeddedKernelStore::open_or_migrate_data_dir(
        source_dir,
        destination_dir,
        kmp_application::projection_mutations_for_context_event,
    )
    .await?;
    Ok(receipt)
}
