//! Store migration (ADR-012): move a data directory this binary refuses to
//! open into one it does.
//!
//! The fail-fast rule says a `FORMAT_VERSION` older than the binary supports
//! must be rejected rather than opened as empty memory. This module is the
//! way out of that rejection, and it is deliberately built on the event log
//! rather than on the store file: projections are derived state, so a
//! migration replays history into a fresh store and rebuilds them, instead
//! of copying materialized tables whose shape is exactly what a format bump
//! is likely to change.
//!
//! Guarantees, in the order they matter:
//!
//!   * The source is never opened for writing. It is hashed, copied, and the
//!     *copy* is what gets opened — so even redb's own recovery after an
//!     unclean shutdown cannot touch the operator's evidence. The hash is
//!     checked again at the end.
//!   * The destination cannot already hold a store. A migration that could
//!     overwrite memory would be a worse failure than the one it fixes.
//!   * The result carries a receipt, persisted in the destination: what was
//!     migrated, from which format, from which bytes.
//!
//! What this module does **not** claim: that any particular older format is
//! translatable. Today one format exists (`1`), so migration is a faithful
//! replay. When a format bump lands, the translation step belongs here, in
//! `translate_event`, and the compatibility matrix in
//! `docs/operations/embedded-release.md` moves in the same pull request.

use std::fs;
use std::path::{Path, PathBuf};

use kmp_domain::{ContextUpdatedEvent, PortError, ProjectionMutation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::engine::{Key, Table};
use super::format_version::{self, SUPPORTED_FORMAT_VERSION};
use super::store::EmbeddedKernelStore;

/// The scratch copy the migration reads. Lives inside the destination so a
/// half-finished migration leaves nothing behind in the source directory.
const SOURCE_COPY_FILE: &str = "migration-source.redb";

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
    ///
    /// `derive` is the projection derivation the composition root owns
    /// (`kmp_application::projection_mutations_for_context_event`), kept
    /// injected so this adapter stays free of the application layer.
    pub async fn migrate_data_dir<F>(
        source_dir: &Path,
        destination_dir: &Path,
        derive: F,
    ) -> Result<(Self, StoreMigrationReceipt), PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> + Send + 'static,
    {
        let source_store_file = format_version::store_file_path(source_dir);
        let destination_store_file = format_version::store_file_path(destination_dir);

        if same_file(&source_store_file, &destination_store_file) {
            return Err(PortError::InvalidState(
                "migration source and destination are the same data directory".to_string(),
            ));
        }
        if !source_store_file.exists() {
            return Err(PortError::InvalidState(format!(
                "migration source `{}` holds no store file at `{}`",
                source_dir.display(),
                source_store_file.display()
            )));
        }
        let source_format = format_version::read_stamped_version(source_dir)?;
        if source_format > SUPPORTED_FORMAT_VERSION {
            return Err(PortError::InvalidState(format!(
                "migration source `{}` uses format version {source_format}, newer than this \
                 binary supports ({SUPPORTED_FORMAT_VERSION}); upgrade the binary",
                source_dir.display()
            )));
        }
        let source_sha256 = sha256_of(&source_store_file)?;

        if destination_store_file.exists() {
            // Re-running a migration is a normal operator reflex, and
            // "already holds a store" is a frightening thing to read when
            // the truth is that the work is already done. Say which it is.
            let already = match Self::open(destination_dir) {
                Ok(store) => store.migration_receipt().await.ok().flatten(),
                Err(_) => None,
            };
            if let Some(receipt) = already
                && receipt.source_sha256 == source_sha256
            {
                return Err(PortError::Conflict(format!(
                    "migration destination `{}` was already migrated from this exact \
                     source ({} events, source sha256 {}); nothing to do",
                    destination_dir.display(),
                    receipt.events_migrated,
                    receipt.source_sha256
                )));
            }
            return Err(PortError::Conflict(format!(
                "migration destination `{}` already holds a store; migrate into a new \
                 directory rather than over existing memory",
                destination_dir.display()
            )));
        }
        let events = read_source_events(&source_store_file, destination_dir)?;

        let destination = Self::open(destination_dir)?;
        let events_migrated = destination.replay_event_stream(events).await?;
        let rebuild = destination.rebuild_projections(derive).await?;

        // The source must be exactly what it was. Anything else means the
        // read-only path leaked, and the operator deserves to hear it from
        // the migration rather than from a later diff.
        let source_sha256_after = sha256_of(&source_store_file)?;
        if source_sha256_after != source_sha256 {
            return Err(PortError::InvalidState(format!(
                "migration modified its source `{}`; refusing to report success",
                source_store_file.display()
            )));
        }

        let receipt = StoreMigrationReceipt {
            source_format,
            source_sha256,
            destination_format: SUPPORTED_FORMAT_VERSION,
            events_migrated,
            mutations_applied: rebuild.mutations_applied,
            kernel_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        destination.write_migration_receipt(&receipt).await?;
        Ok((destination, receipt))
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
        if format_version::store_file_path(destination_dir).exists() {
            let store = Self::open(destination_dir)?;
            let receipt = store.migration_receipt().await?;
            return Ok((store, receipt));
        }
        let (store, receipt) = Self::migrate_data_dir(source_dir, destination_dir, derive).await?;
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

    async fn write_migration_receipt(
        &self,
        receipt: &StoreMigrationReceipt,
    ) -> Result<(), PortError> {
        let encoded = serde_json::to_vec(receipt).map_err(|error| {
            PortError::InvalidState(format!("migration receipt is not encodable: {error}"))
        })?;
        self.run(move |store| {
            let mut tx = store.begin_write()?;
            tx.insert(
                Table::Migrations,
                Key::Str(StoreMigrationReceipt::MIGRATION_ID),
                &encoded,
            )?;
            tx.commit()
        })
        .await
    }
}

/// Reads the source event log without ever opening the source for writing.
///
/// redb may need to recover a file left by an unclean shutdown, and recovery
/// writes. So the file is copied first and the copy is what gets opened; the
/// copy is removed before the destination store is created.
fn read_source_events(
    source_store_file: &Path,
    destination_dir: &Path,
) -> Result<Vec<ContextUpdatedEvent>, PortError> {
    fs::create_dir_all(destination_dir).map_err(|error| {
        PortError::Unavailable(format!(
            "migration could not create destination `{}`: {error}",
            destination_dir.display()
        ))
    })?;
    let copy_path: PathBuf = destination_dir.join(SOURCE_COPY_FILE);
    fs::copy(source_store_file, &copy_path).map_err(|error| {
        PortError::Unavailable(format!(
            "migration could not copy the source store to `{}`: {error}",
            copy_path.display()
        ))
    })?;

    let events = {
        let source = EmbeddedKernelStore::open_store_file(&copy_path)?;
        source.read_event_log_blocking()
    };

    // Best effort: a leftover copy is inert, but leaving it would make the
    // destination directory lie about what it contains.
    let _ = fs::remove_file(&copy_path);
    events
}

fn sha256_of(path: &Path) -> Result<String, PortError> {
    let bytes = fs::read(path).map_err(|error| {
        PortError::Unavailable(format!(
            "migration could not read `{}`: {error}",
            path.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn same_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        // Unresolvable paths (a destination that does not exist yet) fall
        // back to the literal comparison, which is what the caller wrote.
        _ => left == right,
    }
}
