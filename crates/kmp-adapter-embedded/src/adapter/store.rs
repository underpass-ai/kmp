use std::fs;
use std::path::Path;
use std::sync::Arc;

use kmp_domain::PortError;

use super::engine::{Engine, ReadTx, Table, WriteTx};
use super::format_version::{self, StorageEngine};

/// Every kernel persistence port on one local store.
///
/// The engine behind it is chosen when the data directory is created and
/// hidden behind the storage seam
/// ([historical ADR-018](https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/docs/adr/ADR-018-multi-process-embedded-store.md)):
/// SQLite for every store this binary can open. Cloning is cheap (shared
/// engine handle). Commits are fsync-durable, so
/// each successful port write survives `kill -9`; a crash mid-transaction
/// loses only the in-flight transaction.
#[derive(Debug, Clone)]
pub struct EmbeddedKernelStore {
    engine: Arc<dyn Engine>,
}

impl EmbeddedKernelStore {
    /// Opens (or initializes) the store inside `data_dir`, applying the
    /// ADR-012 fail-fast rules before touching the engine. A fresh directory
    /// gets the default engine; an existing one opens with the engine it was
    /// created with.
    pub fn open(data_dir: &Path) -> Result<Self, PortError> {
        Self::open_as(data_dir, None)
    }

    /// [`open`](Self::open) with the engine chosen: a fresh directory is
    /// created for SQLite, and an existing one must already be `engine` — a
    /// store is never reinterpreted as another engine's.
    pub fn open_with_engine(data_dir: &Path, engine: StorageEngine) -> Result<Self, PortError> {
        Self::open_as(data_dir, Some(engine))
    }

    /// The engine a data directory was created with, without opening it.
    pub fn engine_of(data_dir: &Path) -> Result<StorageEngine, PortError> {
        format_version::check_or_stamp_as(data_dir, None)
    }

    fn open_as(data_dir: &Path, wanted: Option<StorageEngine>) -> Result<Self, PortError> {
        fs::create_dir_all(data_dir).map_err(|error| {
            PortError::Unavailable(format!(
                "embedded store could not create data dir `{}`: {error}",
                data_dir.display()
            ))
        })?;
        let engine = format_version::check_or_stamp_as(data_dir, wanted)?;

        let store_file = format_version::store_file_path_for(data_dir, engine);
        fs::create_dir_all(store_file.parent().expect("store file has a parent")).map_err(
            |error| {
                PortError::Unavailable(format!(
                    "embedded store could not create store dir under `{}`: {error}",
                    data_dir.display()
                ))
            },
        )?;

        let engine: Arc<dyn Engine> =
            Arc::new(super::engine::sqlite::SqliteEngine::open_file(&store_file)?);
        Ok(Self { engine })
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
        let engine = format_version::check_or_stamp(data_dir)?;
        let store_file = format_version::store_file_path_for(data_dir, engine);
        super::engine::sqlite::SqliteEngine::compact_file(&store_file)
    }
}

pub(crate) fn aggregate_key(root_node_id: &str, role: &str) -> String {
    format!("{root_node_id}\u{1f}{role}")
}
