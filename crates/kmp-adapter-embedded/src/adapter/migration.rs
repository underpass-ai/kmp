//! Migration receipt compatibility and fail-safe handling for retired store
//! layouts.
//!
//! Current binaries contain only SQLite. They can still read receipts left by
//! older successful migrations, but they never open format-1 redb bytes. A
//! migration request against that layout fails before creating a destination
//! and tells the operator to export with the last compatible KMP release.

use std::fs;
use std::path::Path;

use kmp_domain::{ContextUpdatedEvent, PortError, ProjectionMutation};
use serde::{Deserialize, Serialize};

use super::engine::{Key, Table};
use super::format_version::{self, LEGACY_REDB_FORMAT_VERSION, StorageEngine};
use super::store::EmbeddedKernelStore;

/// What a migration did, kept in the store it produced.
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
    /// Single key: a store is the product of one migration, or of none.
    pub const MIGRATION_ID: &'static str = "store-format-migration";
}

impl EmbeddedKernelStore {
    /// Migrates `source_dir` into `destination_dir` and opens the result.
    /// The destination is created with the default engine.
    ///
    /// `derive` is retained in the API for source compatibility. It will be
    /// used again when a migration between supported SQLite layouts exists.
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

    /// [`migrate_data_dir`](Self::migrate_data_dir) with the destination
    /// engine chosen. There is currently no supported source layout that
    /// needs migration: format 2 opens directly, while format 1 requires an
    /// export made with KMP 0.3.2 because this crate has no redb dependency.
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
        if source_format == LEGACY_REDB_FORMAT_VERSION {
            return Err(PortError::InvalidState(format!(
                "migration source `{}` uses retired format 1 (redb); this binary contains no \
                 redb reader and left the source untouched. Use KMP 0.3.2 to export \
                 `.kmp/memory.jsonl`, then import that bundle with the current KMP",
                source_dir.display()
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
            "migration source `{}` uses unsupported format version {source_format}; this \
             binary left it untouched",
            source_dir.display()
        )))
    }

    /// Migrate once, reopen afterwards: safe to call on every start.
    ///
    /// A destination that already holds a store is opened as it is — the
    /// migration is not repeated, and the receipt (when there is one) says
    /// where that memory came from.
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

    /// [`open_or_migrate_data_dir`](Self::open_or_migrate_data_dir) with the
    /// destination engine chosen. The engine only matters on the call that
    /// migrates; a destination that already holds a store opens as whatever
    /// it is.
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

    /// The receipt of the migration that produced this store, if any.
    pub async fn migration_receipt(&self) -> Result<Option<StoreMigrationReceipt>, PortError> {
        self.run(|store| {
            let tx = store.begin_read()?;
            // A store nobody migrated has no such table; the seam reads a
            // never-written table as empty.
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
        // Unresolvable paths (a destination that does not exist yet) fall
        // back to the literal comparison, which is what the caller wrote.
        _ => left == right,
    }
}
