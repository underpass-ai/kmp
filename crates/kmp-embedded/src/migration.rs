//! Store-migration compatibility API, composed over the embedded adapter.
//!
//! The live CLI does not expose a migration command because SQLite format 2
//! is the only supported layout. These functions remain for downstream Rust
//! API compatibility and always preserve unsupported sources.

use std::path::Path;

use kmp_adapter_embedded::{EmbeddedKernelStore, StorageEngine, StoreMigrationReceipt};
use kmp_domain::PortError;

pub async fn migrate_data_dir(
    source_dir: &Path,
    destination_dir: &Path,
) -> Result<StoreMigrationReceipt, PortError> {
    migrate_data_dir_to(source_dir, destination_dir, StorageEngine::Sqlite).await
}

pub async fn migrate_data_dir_to(
    source_dir: &Path,
    destination_dir: &Path,
    destination_engine: StorageEngine,
) -> Result<StoreMigrationReceipt, PortError> {
    let (_store, receipt) = EmbeddedKernelStore::migrate_data_dir_to(
        source_dir,
        destination_dir,
        destination_engine,
        kmp_application::projection_mutations_for_context_event,
    )
    .await?;
    Ok(receipt)
}

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
