//! Compatibility API for store-layout migration receipts.
//!
//! Current KMP has one supported layout, SQLite format 2, so there is no live
//! in-process migration path. These APIs remain available to avoid breaking
//! downstream Rust callers: a current source reports that migration is
//! unnecessary, and an unsupported source is preserved with the same generic
//! external export/import recovery contract as the open gate.

use std::fs;
use std::path::Path;

use kmp_domain::{ContextUpdatedEvent, PortError, ProjectionMutation};
use serde::{Deserialize, Serialize};

use super::engine::{Key, Table};
use super::format_version::{self, StorageEngine};
use super::store::EmbeddedKernelStore;

/// What a completed historical store migration recorded in its destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreMigrationReceipt {
    pub source_format: u32,
    pub source_sha256: String,
    pub destination_format: u32,
    pub events_migrated: u64,
    pub mutations_applied: u64,
    pub kernel_version: String,
}

impl StoreMigrationReceipt {
    /// Single key used by historical migration receipts.
    pub const MIGRATION_ID: &'static str = "store-format-migration";
}

impl EmbeddedKernelStore {
    /// Compatibility entry point. No current layout requires migration.
    pub async fn migrate_data_dir<F>(
        source_dir: &Path,
        destination_dir: &Path,
        derive: F,
    ) -> Result<(Self, StoreMigrationReceipt), PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> + Send + 'static,
    {
        Self::migrate_data_dir_to(source_dir, destination_dir, StorageEngine::Sqlite, derive).await
    }

    /// Compatibility entry point with an explicit destination engine.
    pub async fn migrate_data_dir_to<F>(
        source_dir: &Path,
        destination_dir: &Path,
        destination_engine: StorageEngine,
        _derive: F,
    ) -> Result<(Self, StoreMigrationReceipt), PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> + Send + 'static,
    {
        if same_file(source_dir, destination_dir) {
            return Err(PortError::InvalidState(
                "migration source and destination are the same data directory".to_string(),
            ));
        }
        let source_format = format_version::read_stamped_version(source_dir)?;
        if source_format > StorageEngine::NEWEST_KNOWN_FORMAT_VERSION {
            return Err(PortError::InvalidState(format!(
                "migration source `{}` uses format version {source_format}, newer than this \
                 binary supports ({}); upgrade the binary",
                source_dir.display(),
                StorageEngine::NEWEST_KNOWN_FORMAT_VERSION
            )));
        }
        if source_format == StorageEngine::Sqlite.format_version() {
            return Err(PortError::Unavailable(format!(
                "migration from a SQLite format-2 store is unnecessary and unsupported; the \
                 source at `{}` is left untouched",
                source_dir.display()
            )));
        }
        let _ = destination_engine;
        Err(PortError::InvalidState(format!(
            "migration source `{}` uses unsupported format version {source_format}; current \
             KMP left it untouched. Preserve the source, use an explicitly archived compatible \
             exporter to create `.kmp/memory.jsonl`, then import that bundle into an empty \
             current store",
            source_dir.display()
        )))
    }

    /// Reopen a completed destination or apply the compatibility migration.
    pub async fn open_or_migrate_data_dir<F>(
        source_dir: &Path,
        destination_dir: &Path,
        derive: F,
    ) -> Result<(Self, Option<StoreMigrationReceipt>), PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> + Send + 'static,
    {
        Self::open_or_migrate_data_dir_to(
            source_dir,
            destination_dir,
            StorageEngine::Sqlite,
            derive,
        )
        .await
    }

    /// Reopen a completed destination or apply the compatibility migration.
    pub async fn open_or_migrate_data_dir_to<F>(
        source_dir: &Path,
        destination_dir: &Path,
        destination_engine: StorageEngine,
        derive: F,
    ) -> Result<(Self, Option<StoreMigrationReceipt>), PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> + Send + 'static,
    {
        if format_version::existing_store_file(destination_dir).is_some() {
            let store = Self::open(destination_dir)?;
            let receipt = store.migration_receipt().await?;
            return Ok((store, receipt));
        }
        let (store, receipt) =
            Self::migrate_data_dir_to(source_dir, destination_dir, destination_engine, derive)
                .await?;
        Ok((store, Some(receipt)))
    }

    /// A historical receipt stored by the migration that produced this store.
    pub async fn migration_receipt(&self) -> Result<Option<StoreMigrationReceipt>, PortError> {
        self.run(|store| {
            let tx = store.begin_read()?;
            let Some(raw) = tx.get(
                Table::Migrations,
                Key::Str(StoreMigrationReceipt::MIGRATION_ID),
            )?
            else {
                return Ok(None);
            };
            let receipt = serde_json::from_slice(&raw).map_err(|error| {
                PortError::InvalidState(format!("migration receipt is unreadable: {error}"))
            })?;
            Ok(Some(receipt))
        })
        .await
    }
}

fn same_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
