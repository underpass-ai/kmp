use std::fs;
use std::path::Path;
use std::sync::Arc;

use kmp_domain::PortError;
use redb::{Database, TableDefinition};

use super::format_version;

/// Graph nodes: `node_id -> NodeRecord` (JSON).
pub(crate) const NODES: TableDefinition<&str, &[u8]> = TableDefinition::new("nodes");
/// Outgoing adjacency: `(source, target, relation_type) -> explanation properties` (JSON).
pub(crate) const RELATIONS: TableDefinition<(&str, &str, &str), &[u8]> =
    TableDefinition::new("relations_by_source");
/// Incoming adjacency index: `(target, source, relation_type) -> ()`.
pub(crate) const RELATIONS_BY_TARGET: TableDefinition<(&str, &str, &str), ()> =
    TableDefinition::new("relations_by_target");
/// Node details: `node_id -> DetailRecord` (JSON).
pub(crate) const DETAILS: TableDefinition<&str, &[u8]> = TableDefinition::new("details");
/// Memory anchor index: `node_id -> ()` for nodes with kind `memory_anchor`.
pub(crate) const ANCHORS: TableDefinition<&str, ()> = TableDefinition::new("memory_anchors");
/// Append-only context event log: `sequence -> ContextUpdatedEvent` (JSON).
pub(crate) const EVENT_LOG: TableDefinition<u64, &[u8]> = TableDefinition::new("event_log");
/// Aggregate heads: `"root\u{1f}role" -> AggregateRecord` (JSON).
pub(crate) const AGGREGATES: TableDefinition<&str, &[u8]> = TableDefinition::new("aggregates");
/// Idempotency outcomes: `key -> IdempotentOutcome` (JSON).
pub(crate) const IDEMPOTENCY: TableDefinition<&str, &[u8]> = TableDefinition::new("idempotency");
/// Projection-consumer dedup: `(consumer, event_id) -> ()`.
pub(crate) const PROCESSED: TableDefinition<(&str, &str), ()> =
    TableDefinition::new("processed_events");
/// Projection checkpoints: `(consumer, stream) -> CheckpointRecord` (JSON).
pub(crate) const CHECKPOINTS: TableDefinition<(&str, &str), &[u8]> =
    TableDefinition::new("projection_checkpoints");
/// Snapshot audit records: `(root, role) -> snapshot summary` (JSON).
pub(crate) const SNAPSHOTS: TableDefinition<(&str, &str), &[u8]> =
    TableDefinition::new("snapshots");

/// Every kernel persistence port on one local redb file.
///
/// Cloning is cheap (shared database handle). Commits are fsync-durable, so
/// each successful port write survives `kill -9`; a crash mid-transaction
/// loses only the in-flight transaction.
#[derive(Debug, Clone)]
pub struct EmbeddedKernelStore {
    database: Arc<Database>,
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

        let database = Database::create(&store_file).map_err(|error| {
            PortError::Unavailable(format!(
                "embedded store could not open `{}`: {error}",
                store_file.display()
            ))
        })?;

        let store = Self {
            database: Arc::new(database),
        };
        store.initialize_tables()?;
        Ok(store)
    }

    /// Creates every table up front so read transactions never race table
    /// existence.
    fn initialize_tables(&self) -> Result<(), PortError> {
        let tx = self.begin_write()?;
        {
            tx.open_table(NODES).map_err(table_error)?;
            tx.open_table(RELATIONS).map_err(table_error)?;
            tx.open_table(RELATIONS_BY_TARGET).map_err(table_error)?;
            tx.open_table(DETAILS).map_err(table_error)?;
            tx.open_table(ANCHORS).map_err(table_error)?;
            tx.open_table(EVENT_LOG).map_err(table_error)?;
            tx.open_table(AGGREGATES).map_err(table_error)?;
            tx.open_table(IDEMPOTENCY).map_err(table_error)?;
            tx.open_table(PROCESSED).map_err(table_error)?;
            tx.open_table(CHECKPOINTS).map_err(table_error)?;
            tx.open_table(SNAPSHOTS).map_err(table_error)?;
        }
        tx.commit().map_err(commit_error)
    }

    pub(crate) fn begin_write(&self) -> Result<redb::WriteTransaction, PortError> {
        self.database.begin_write().map_err(|error| {
            PortError::Unavailable(format!("embedded store write transaction failed: {error}"))
        })
    }

    pub(crate) fn begin_read(&self) -> Result<redb::ReadTransaction, PortError> {
        use redb::ReadableDatabase;
        self.database.begin_read().map_err(|error| {
            PortError::Unavailable(format!("embedded store read transaction failed: {error}"))
        })
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
            let log = tx.open_table(EVENT_LOG).map_err(table_error)?;
            let mut count = 0u64;
            let mut last_sequence = 0u64;
            for row in redb::ReadableTable::iter(&log).map_err(range_error)? {
                let (key, _) = row.map_err(range_error)?;
                count += 1;
                last_sequence = key.value();
            }
            Ok((count, last_sequence))
        })
        .await
    }
}

pub(crate) fn aggregate_key(root_node_id: &str, role: &str) -> String {
    format!("{root_node_id}\u{1f}{role}")
}

pub(crate) fn table_error(error: redb::TableError) -> PortError {
    PortError::Unavailable(format!("embedded store table access failed: {error}"))
}

pub(crate) fn storage_error(error: redb::StorageError) -> PortError {
    PortError::Unavailable(format!("embedded store storage access failed: {error}"))
}

pub(crate) fn range_error(error: impl std::fmt::Display) -> PortError {
    PortError::Unavailable(format!("embedded store range read failed: {error}"))
}

pub(crate) fn commit_error(error: redb::CommitError) -> PortError {
    PortError::Unavailable(format!("embedded store commit failed: {error}"))
}

impl EmbeddedKernelStore {
    /// Compacts the store file in place, reclaiming free pages left by
    /// past transactions (e.g. after a projection rebuild). Requires
    /// exclusive access: call it with no other store handle open on the
    /// same data directory.
    pub fn compact_data_dir(data_dir: &Path) -> Result<bool, PortError> {
        format_version::check_or_stamp(data_dir)?;
        let store_file = format_version::store_file_path(data_dir);
        let mut database = Database::create(&store_file).map_err(|error| {
            PortError::Unavailable(format!(
                "embedded store could not open `{}` for compaction: {error}",
                store_file.display()
            ))
        })?;
        database.compact().map_err(|error| {
            PortError::Unavailable(format!(
                "embedded store compaction failed for `{}`: {error}",
                store_file.display()
            ))
        })
    }
}
