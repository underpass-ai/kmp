use std::fs;
use std::path::Path;
use std::sync::Arc;

use kmp_domain::PortError;

use super::engine::redb::RedbEngine;
use super::engine::{Engine, ReadTx, Table, WriteTx};
use super::format_version;

/// Every kernel persistence port on one local store.
///
/// The engine behind it is chosen at open time and hidden behind the storage
/// seam ([ADR-018](../../../../docs/adr/ADR-018-multi-process-embedded-store.md));
/// today that is redb. Cloning is cheap (shared engine handle). Commits are
/// fsync-durable, so each successful port write survives `kill -9`; a crash
/// mid-transaction loses only the in-flight transaction.
#[derive(Debug, Clone)]
pub struct EmbeddedKernelStore {
    engine: Arc<dyn Engine>,
}

impl EmbeddedKernelStore {
    /// Opens (or initializes) the store inside `data_dir`, applying the
    /// ADR-012 fail-fast rules before touching the engine.
    pub fn open(data_dir: &Path) -> Result<Self, PortError> {
        fs::create_dir_all(data_dir).map_err(|error| {
            PortError::Unavailable(format!(
                "embedded store could not create data dir `{}`: {error}",
                data_dir.display()
            ))
        })?;
        format_version::check_or_stamp(data_dir)?;

        let store_file = format_version::store_file_path(data_dir);
        fs::create_dir_all(store_file.parent().expect("store file has a parent")).map_err(
            |error| {
                PortError::Unavailable(format!(
                    "embedded store could not create store dir under `{}`: {error}",
                    data_dir.display()
                ))
            },
        )?;

        Self::open_store_file(&store_file)
    }

    /// Opens a bare store file, without the data-directory layout or its
    /// format gate. Only two callers may want this: `open`, which has just
    /// applied the gate itself, and the migration, which reads a *copy* of a
    /// store whose format this binary refuses to open in place.
    pub(crate) fn open_store_file(store_file: &Path) -> Result<Self, PortError> {
        Ok(Self {
            engine: Arc::new(RedbEngine::open_file(store_file)?),
        })
    }

    pub(crate) fn begin_write(&self) -> Result<Box<dyn WriteTx + '_>, PortError> {
        self.engine.begin_write()
    }

    pub(crate) fn begin_read(&self) -> Result<Box<dyn ReadTx + '_>, PortError> {
        self.engine.begin_read()
    }

    /// Runs blocking engine work on the blocking thread pool so port calls
    /// never stall the async executor on fsync.
    pub(crate) async fn run<T, F>(&self, task: F) -> Result<T, PortError>
    where
        T: Send + 'static,
        F: FnOnce(&EmbeddedKernelStore) -> Result<T, PortError> + Send + 'static,
    {
        let store = self.clone();
        tokio::task::spawn_blocking(move || task(&store))
            .await
            .map_err(|error| {
                PortError::Unavailable(format!("embedded store worker failed: {error}"))
            })?
    }

    /// Number of events in the append-only log and the highest sequence —
    /// audit surface used by recovery checks and operational tooling.
    pub async fn event_log_stats(&self) -> Result<(u64, u64), PortError> {
        self.run(|store| {
            let tx = store.begin_read()?;
            let count = tx.count(Table::EventLog)?;
            let last_sequence = tx.last_u64(Table::EventLog)?.map_or(0, |(key, _)| key);
            Ok((count, last_sequence))
        })
        .await
    }

    /// Compacts the store file in place, reclaiming free pages left by
    /// past transactions (e.g. after a projection rebuild). Requires
    /// exclusive access: call it with no other store handle open on the
    /// same data directory.
    pub fn compact_data_dir(data_dir: &Path) -> Result<bool, PortError> {
        format_version::check_or_stamp(data_dir)?;
        let store_file = format_version::store_file_path(data_dir);
        RedbEngine::compact_file(&store_file)
    }
}

pub(crate) fn aggregate_key(root_node_id: &str, role: &str) -> String {
    format!("{root_node_id}\u{1f}{role}")
}
