//! The redb engine ([ADR-009](../../../../../docs/adr/ADR-009-embedded-storage-engine.md))
//! behind the seam.
//!
//! On-disk layout is unchanged from before the seam existed: the same typed
//! table definitions, the same key and value types, the same file. redb
//! records the key and value type names in the table metadata and refuses a
//! definition that disagrees, so a unit-valued table has to stay unit-valued
//! — which is why the seam carries empty bytes for them and this module maps
//! that to `()`.
//!
//! Tables are opened per operation and dropped before the next one. A redb
//! write transaction refuses to open a table that already has a live handle,
//! and holding handles across seam calls would need the transaction and its
//! borrowers in one struct — a self-referential shape not worth the unsafe.
//! Opening an existing table is a root lookup inside the transaction, cheap
//! enough that the conformance and scale suites do not notice.

use std::path::Path;
use std::sync::Arc;

use kmp_domain::PortError;
use redb::{
    Database, ReadTransaction, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition, TableError, WriteTransaction,
};

use super::{
    Engine, Key, KeyShape, ReadTx, Str3Row, StrRow, Table, U64Row, WriteTx, key_shape_mismatch,
    scan_shape_mismatch,
};

// ---------------------------------------------------------------- tables --
// Names, key types and value types are load-bearing: they are what an
// existing store on disk was written with.

const NODES: TableDefinition<&str, &[u8]> = TableDefinition::new("nodes");
const RELATIONS: TableDefinition<(&str, &str, &str), &[u8]> =
    TableDefinition::new("relations_by_source");
const RELATIONS_BY_TARGET: TableDefinition<(&str, &str, &str), ()> =
    TableDefinition::new("relations_by_target");
const DETAILS: TableDefinition<&str, &[u8]> = TableDefinition::new("details");
const ANCHORS: TableDefinition<&str, ()> = TableDefinition::new("memory_anchors");
const EVENT_LOG: TableDefinition<u64, &[u8]> = TableDefinition::new("event_log");
const AGGREGATES: TableDefinition<&str, &[u8]> = TableDefinition::new("aggregates");
const IDEMPOTENCY: TableDefinition<&str, &[u8]> = TableDefinition::new("idempotency");
const PROCESSED: TableDefinition<(&str, &str), ()> = TableDefinition::new("processed_events");
const CHECKPOINTS: TableDefinition<(&str, &str), &[u8]> =
    TableDefinition::new("projection_checkpoints");
const SNAPSHOTS: TableDefinition<(&str, &str), &[u8]> = TableDefinition::new("snapshots");
const MIGRATIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("store_migrations");

// ---------------------------------------------------------------- engine --

/// One open redb file.
#[derive(Debug, Clone)]
pub(crate) struct RedbEngine {
    database: Arc<Database>,
}

impl RedbEngine {
    /// Opens (or creates) `store_file` and materializes every table, so read
    /// transactions never race table existence.
    pub(crate) fn open_file(store_file: &Path) -> Result<Self, PortError> {
        let database = Database::create(store_file).map_err(|error| {
            PortError::Unavailable(format!(
                "embedded store could not open `{}`: {error}",
                store_file.display()
            ))
        })?;
        let engine = Self {
            database: Arc::new(database),
        };
        engine.initialize_tables()?;
        Ok(engine)
    }

    /// Compacts `store_file` in place. Requires exclusive access.
    pub(crate) fn compact_file(store_file: &Path) -> Result<bool, PortError> {
        let mut database = Database::create(store_file).map_err(|error| {
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

    fn initialize_tables(&self) -> Result<(), PortError> {
        let tx = self.database.begin_write().map_err(write_begin_error)?;
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
}

impl Engine for RedbEngine {
    fn begin_read(&self) -> Result<Box<dyn ReadTx + '_>, PortError> {
        let tx = self.database.begin_read().map_err(|error| {
            PortError::Unavailable(format!("embedded store read transaction failed: {error}"))
        })?;
        Ok(Box::new(RedbRead { tx }))
    }

    fn begin_write(&self) -> Result<Box<dyn WriteTx + '_>, PortError> {
        let tx = self.database.begin_write().map_err(write_begin_error)?;
        Ok(Box::new(RedbWrite { tx }))
    }
}

// ---------------------------------------------------------- read helpers --
//
// Generic over the table handle so one implementation serves both a
// `ReadOnlyTable` (read transaction) and a `Table` (write transaction).

fn get_bytes<K: redb::Key + 'static>(
    table: &impl ReadableTable<K, &'static [u8]>,
    key: K::SelfType<'_>,
) -> Result<Option<Vec<u8>>, PortError> {
    Ok(table
        .get(key)
        .map_err(storage_error)?
        .map(|guard| guard.value().to_vec()))
}

fn get_unit<K: redb::Key + 'static>(
    table: &impl ReadableTable<K, ()>,
    key: K::SelfType<'_>,
) -> Result<Option<Vec<u8>>, PortError> {
    Ok(table.get(key).map_err(storage_error)?.map(|_| Vec::new()))
}

fn scan_str_bytes(
    table: &impl ReadableTable<&'static str, &'static [u8]>,
) -> Result<Vec<StrRow>, PortError> {
    let mut rows = Vec::new();
    for row in table.iter().map_err(range_error)? {
        let (key, value) = row.map_err(range_error)?;
        rows.push((key.value().to_string(), value.value().to_vec()));
    }
    Ok(rows)
}

fn scan_str_unit(table: &impl ReadableTable<&'static str, ()>) -> Result<Vec<StrRow>, PortError> {
    let mut rows = Vec::new();
    for row in table.iter().map_err(range_error)? {
        let (key, _) = row.map_err(range_error)?;
        rows.push((key.value().to_string(), Vec::new()));
    }
    Ok(rows)
}

type Str3Key = (&'static str, &'static str, &'static str);

fn scan_str3_bytes(
    table: &impl ReadableTable<Str3Key, &'static [u8]>,
    first: &str,
) -> Result<Vec<Str3Row>, PortError> {
    let upper = format!("{first}\u{0}");
    let mut rows = Vec::new();
    for row in table
        .range((first, "", "")..(upper.as_str(), "", ""))
        .map_err(range_error)?
    {
        let (key, value) = row.map_err(range_error)?;
        let (a, b, c) = key.value();
        rows.push((
            (a.to_string(), b.to_string(), c.to_string()),
            value.value().to_vec(),
        ));
    }
    Ok(rows)
}

fn scan_str3_unit(
    table: &impl ReadableTable<Str3Key, ()>,
    first: &str,
) -> Result<Vec<Str3Row>, PortError> {
    let upper = format!("{first}\u{0}");
    let mut rows = Vec::new();
    for row in table
        .range((first, "", "")..(upper.as_str(), "", ""))
        .map_err(range_error)?
    {
        let (key, _) = row.map_err(range_error)?;
        let (a, b, c) = key.value();
        rows.push(((a.to_string(), b.to_string(), c.to_string()), Vec::new()));
    }
    Ok(rows)
}

fn scan_u64_bytes(
    table: &impl ReadableTable<u64, &'static [u8]>,
) -> Result<Vec<U64Row>, PortError> {
    let mut rows = Vec::new();
    for row in table.iter().map_err(range_error)? {
        let (key, value) = row.map_err(range_error)?;
        rows.push((key.value(), value.value().to_vec()));
    }
    Ok(rows)
}

fn last_u64_bytes(
    table: &impl ReadableTable<u64, &'static [u8]>,
) -> Result<Option<U64Row>, PortError> {
    Ok(table
        .last()
        .map_err(storage_error)?
        .map(|(key, value)| (key.value(), value.value().to_vec())))
}

fn count_of(table: &impl ReadableTableMetadata) -> Result<u64, PortError> {
    table.len().map_err(storage_error)
}

// ----------------------------------------------------------- read txn --

struct RedbRead {
    tx: ReadTransaction,
}

/// Opens `def` read-only, treating a missing table as an empty one, and
/// applies `op` to it.
macro_rules! read_table {
    ($tx:expr, $def:expr, $empty:expr, |$t:ident| $op:expr) => {{
        match $tx.open_table($def) {
            Ok($t) => $op,
            Err(TableError::TableDoesNotExist(_)) => Ok($empty),
            Err(error) => Err(table_error(error)),
        }
    }};
}

impl ReadTx for RedbRead {
    fn get(&self, table: Table, key: Key<'_>) -> Result<Option<Vec<u8>>, PortError> {
        let tx = &self.tx;
        match (table, key) {
            (Table::Nodes, Key::Str(k)) => read_table!(tx, NODES, None, |t| get_bytes(&t, k)),
            (Table::Details, Key::Str(k)) => read_table!(tx, DETAILS, None, |t| get_bytes(&t, k)),
            (Table::Aggregates, Key::Str(k)) => {
                read_table!(tx, AGGREGATES, None, |t| get_bytes(&t, k))
            }
            (Table::Idempotency, Key::Str(k)) => {
                read_table!(tx, IDEMPOTENCY, None, |t| get_bytes(&t, k))
            }
            (Table::Migrations, Key::Str(k)) => {
                read_table!(tx, MIGRATIONS, None, |t| get_bytes(&t, k))
            }
            (Table::Anchors, Key::Str(k)) => read_table!(tx, ANCHORS, None, |t| get_unit(&t, k)),
            (Table::Checkpoints, Key::Str2(a, b)) => {
                read_table!(tx, CHECKPOINTS, None, |t| get_bytes(&t, (a, b)))
            }
            (Table::Snapshots, Key::Str2(a, b)) => {
                read_table!(tx, SNAPSHOTS, None, |t| get_bytes(&t, (a, b)))
            }
            (Table::Processed, Key::Str2(a, b)) => {
                read_table!(tx, PROCESSED, None, |t| get_unit(&t, (a, b)))
            }
            (Table::Relations, Key::Str3(a, b, c)) => {
                read_table!(tx, RELATIONS, None, |t| get_bytes(&t, (a, b, c)))
            }
            (Table::RelationsByTarget, Key::Str3(a, b, c)) => {
                read_table!(tx, RELATIONS_BY_TARGET, None, |t| get_unit(&t, (a, b, c)))
            }
            (Table::EventLog, Key::U64(k)) => {
                read_table!(tx, EVENT_LOG, None, |t| get_bytes(&t, k))
            }
            (table, key) => Err(key_shape_mismatch(table, key.shape())),
        }
    }

    fn scan_str(&self, table: Table) -> Result<Vec<StrRow>, PortError> {
        let tx = &self.tx;
        match table {
            Table::Nodes => read_table!(tx, NODES, Vec::new(), |t| scan_str_bytes(&t)),
            Table::Details => read_table!(tx, DETAILS, Vec::new(), |t| scan_str_bytes(&t)),
            Table::Aggregates => read_table!(tx, AGGREGATES, Vec::new(), |t| scan_str_bytes(&t)),
            Table::Idempotency => read_table!(tx, IDEMPOTENCY, Vec::new(), |t| scan_str_bytes(&t)),
            Table::Migrations => read_table!(tx, MIGRATIONS, Vec::new(), |t| scan_str_bytes(&t)),
            Table::Anchors => read_table!(tx, ANCHORS, Vec::new(), |t| scan_str_unit(&t)),
            other => Err(scan_shape_mismatch(other, KeyShape::Str)),
        }
    }

    fn scan_str3_by_first(&self, table: Table, first: &str) -> Result<Vec<Str3Row>, PortError> {
        let tx = &self.tx;
        match table {
            Table::Relations => {
                read_table!(tx, RELATIONS, Vec::new(), |t| scan_str3_bytes(&t, first))
            }
            Table::RelationsByTarget => {
                read_table!(tx, RELATIONS_BY_TARGET, Vec::new(), |t| scan_str3_unit(
                    &t, first
                ))
            }
            other => Err(scan_shape_mismatch(other, KeyShape::Str3)),
        }
    }

    fn scan_u64(&self, table: Table) -> Result<Vec<U64Row>, PortError> {
        match table {
            Table::EventLog => read_table!(&self.tx, EVENT_LOG, Vec::new(), |t| scan_u64_bytes(&t)),
            other => Err(scan_shape_mismatch(other, KeyShape::U64)),
        }
    }

    fn last_u64(&self, table: Table) -> Result<Option<U64Row>, PortError> {
        match table {
            Table::EventLog => read_table!(&self.tx, EVENT_LOG, None, |t| last_u64_bytes(&t)),
            other => Err(scan_shape_mismatch(other, KeyShape::U64)),
        }
    }

    fn count(&self, table: Table) -> Result<u64, PortError> {
        let tx = &self.tx;
        match table {
            Table::Nodes => read_table!(tx, NODES, 0, |t| count_of(&t)),
            Table::Relations => read_table!(tx, RELATIONS, 0, |t| count_of(&t)),
            Table::RelationsByTarget => read_table!(tx, RELATIONS_BY_TARGET, 0, |t| count_of(&t)),
            Table::Details => read_table!(tx, DETAILS, 0, |t| count_of(&t)),
            Table::Anchors => read_table!(tx, ANCHORS, 0, |t| count_of(&t)),
            Table::EventLog => read_table!(tx, EVENT_LOG, 0, |t| count_of(&t)),
            Table::Aggregates => read_table!(tx, AGGREGATES, 0, |t| count_of(&t)),
            Table::Idempotency => read_table!(tx, IDEMPOTENCY, 0, |t| count_of(&t)),
            Table::Processed => read_table!(tx, PROCESSED, 0, |t| count_of(&t)),
            Table::Checkpoints => read_table!(tx, CHECKPOINTS, 0, |t| count_of(&t)),
            Table::Snapshots => read_table!(tx, SNAPSHOTS, 0, |t| count_of(&t)),
            Table::Migrations => read_table!(tx, MIGRATIONS, 0, |t| count_of(&t)),
        }
    }
}

// ---------------------------------------------------------- write txn --

struct RedbWrite {
    tx: WriteTransaction,
}

/// Opens `def` read-write — creating it if absent, which is what makes a
/// lazily-created table like `Migrations` appear on first write — and
/// applies a mutating `op` to it.
macro_rules! write_table {
    ($tx:expr, $def:expr, |$t:ident| $op:expr) => {{
        let mut $t = $tx.open_table($def).map_err(table_error)?;
        $op
    }};
}

/// Same open, for a read inside a write transaction: this transaction's own
/// writes are visible, and the handle is not mutated.
macro_rules! peek_table {
    ($tx:expr, $def:expr, |$t:ident| $op:expr) => {{
        let $t = $tx.open_table($def).map_err(table_error)?;
        $op
    }};
}

impl RedbWrite {
    fn clear_typed<K, V>(&self, def: TableDefinition<'static, K, V>) -> Result<(), PortError>
    where
        K: redb::Key + 'static,
        V: redb::Value + 'static,
    {
        // delete + reopen: the table exists again, empty, inside this same
        // transaction — a reader after the commit finds it empty, not gone.
        self.tx.delete_table(def).map_err(table_error)?;
        self.tx.open_table(def).map_err(table_error)?;
        Ok(())
    }
}

impl ReadTx for RedbWrite {
    fn get(&self, table: Table, key: Key<'_>) -> Result<Option<Vec<u8>>, PortError> {
        let tx = &self.tx;
        match (table, key) {
            (Table::Nodes, Key::Str(k)) => peek_table!(tx, NODES, |t| get_bytes(&t, k)),
            (Table::Details, Key::Str(k)) => peek_table!(tx, DETAILS, |t| get_bytes(&t, k)),
            (Table::Aggregates, Key::Str(k)) => peek_table!(tx, AGGREGATES, |t| get_bytes(&t, k)),
            (Table::Idempotency, Key::Str(k)) => {
                peek_table!(tx, IDEMPOTENCY, |t| get_bytes(&t, k))
            }
            (Table::Migrations, Key::Str(k)) => peek_table!(tx, MIGRATIONS, |t| get_bytes(&t, k)),
            (Table::Anchors, Key::Str(k)) => peek_table!(tx, ANCHORS, |t| get_unit(&t, k)),
            (Table::Checkpoints, Key::Str2(a, b)) => {
                peek_table!(tx, CHECKPOINTS, |t| get_bytes(&t, (a, b)))
            }
            (Table::Snapshots, Key::Str2(a, b)) => {
                peek_table!(tx, SNAPSHOTS, |t| get_bytes(&t, (a, b)))
            }
            (Table::Processed, Key::Str2(a, b)) => {
                peek_table!(tx, PROCESSED, |t| get_unit(&t, (a, b)))
            }
            (Table::Relations, Key::Str3(a, b, c)) => {
                peek_table!(tx, RELATIONS, |t| get_bytes(&t, (a, b, c)))
            }
            (Table::RelationsByTarget, Key::Str3(a, b, c)) => {
                peek_table!(tx, RELATIONS_BY_TARGET, |t| get_unit(&t, (a, b, c)))
            }
            (Table::EventLog, Key::U64(k)) => peek_table!(tx, EVENT_LOG, |t| get_bytes(&t, k)),
            (table, key) => Err(key_shape_mismatch(table, key.shape())),
        }
    }

    fn scan_str(&self, table: Table) -> Result<Vec<StrRow>, PortError> {
        let tx = &self.tx;
        match table {
            Table::Nodes => peek_table!(tx, NODES, |t| scan_str_bytes(&t)),
            Table::Details => peek_table!(tx, DETAILS, |t| scan_str_bytes(&t)),
            Table::Aggregates => peek_table!(tx, AGGREGATES, |t| scan_str_bytes(&t)),
            Table::Idempotency => peek_table!(tx, IDEMPOTENCY, |t| scan_str_bytes(&t)),
            Table::Migrations => peek_table!(tx, MIGRATIONS, |t| scan_str_bytes(&t)),
            Table::Anchors => peek_table!(tx, ANCHORS, |t| scan_str_unit(&t)),
            other => Err(scan_shape_mismatch(other, KeyShape::Str)),
        }
    }

    fn scan_str3_by_first(&self, table: Table, first: &str) -> Result<Vec<Str3Row>, PortError> {
        let tx = &self.tx;
        match table {
            Table::Relations => peek_table!(tx, RELATIONS, |t| scan_str3_bytes(&t, first)),
            Table::RelationsByTarget => {
                peek_table!(tx, RELATIONS_BY_TARGET, |t| scan_str3_unit(&t, first))
            }
            other => Err(scan_shape_mismatch(other, KeyShape::Str3)),
        }
    }

    fn scan_u64(&self, table: Table) -> Result<Vec<U64Row>, PortError> {
        match table {
            Table::EventLog => peek_table!(&self.tx, EVENT_LOG, |t| scan_u64_bytes(&t)),
            other => Err(scan_shape_mismatch(other, KeyShape::U64)),
        }
    }

    fn last_u64(&self, table: Table) -> Result<Option<U64Row>, PortError> {
        match table {
            Table::EventLog => peek_table!(&self.tx, EVENT_LOG, |t| last_u64_bytes(&t)),
            other => Err(scan_shape_mismatch(other, KeyShape::U64)),
        }
    }

    fn count(&self, table: Table) -> Result<u64, PortError> {
        let tx = &self.tx;
        match table {
            Table::Nodes => peek_table!(tx, NODES, |t| count_of(&t)),
            Table::Relations => peek_table!(tx, RELATIONS, |t| count_of(&t)),
            Table::RelationsByTarget => peek_table!(tx, RELATIONS_BY_TARGET, |t| count_of(&t)),
            Table::Details => peek_table!(tx, DETAILS, |t| count_of(&t)),
            Table::Anchors => peek_table!(tx, ANCHORS, |t| count_of(&t)),
            Table::EventLog => peek_table!(tx, EVENT_LOG, |t| count_of(&t)),
            Table::Aggregates => peek_table!(tx, AGGREGATES, |t| count_of(&t)),
            Table::Idempotency => peek_table!(tx, IDEMPOTENCY, |t| count_of(&t)),
            Table::Processed => peek_table!(tx, PROCESSED, |t| count_of(&t)),
            Table::Checkpoints => peek_table!(tx, CHECKPOINTS, |t| count_of(&t)),
            Table::Snapshots => peek_table!(tx, SNAPSHOTS, |t| count_of(&t)),
            Table::Migrations => peek_table!(tx, MIGRATIONS, |t| count_of(&t)),
        }
    }
}

impl WriteTx for RedbWrite {
    fn insert(&mut self, table: Table, key: Key<'_>, value: &[u8]) -> Result<(), PortError> {
        let tx = &self.tx;
        match (table, key) {
            (Table::Nodes, Key::Str(k)) => {
                write_table!(tx, NODES, |t| t
                    .insert(k, value)
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Details, Key::Str(k)) => {
                write_table!(tx, DETAILS, |t| t
                    .insert(k, value)
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Aggregates, Key::Str(k)) => write_table!(tx, AGGREGATES, |t| t
                .insert(k, value)
                .map(drop)
                .map_err(storage_error)),
            (Table::Idempotency, Key::Str(k)) => write_table!(tx, IDEMPOTENCY, |t| t
                .insert(k, value)
                .map(drop)
                .map_err(storage_error)),
            (Table::Migrations, Key::Str(k)) => write_table!(tx, MIGRATIONS, |t| t
                .insert(k, value)
                .map(drop)
                .map_err(storage_error)),
            (Table::Anchors, Key::Str(k)) => {
                write_table!(tx, ANCHORS, |t| t
                    .insert(k, ())
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Checkpoints, Key::Str2(a, b)) => write_table!(tx, CHECKPOINTS, |t| t
                .insert((a, b), value)
                .map(drop)
                .map_err(storage_error)),
            (Table::Snapshots, Key::Str2(a, b)) => write_table!(tx, SNAPSHOTS, |t| t
                .insert((a, b), value)
                .map(drop)
                .map_err(storage_error)),
            (Table::Processed, Key::Str2(a, b)) => write_table!(tx, PROCESSED, |t| t
                .insert((a, b), ())
                .map(drop)
                .map_err(storage_error)),
            (Table::Relations, Key::Str3(a, b, c)) => write_table!(tx, RELATIONS, |t| t
                .insert((a, b, c), value)
                .map(drop)
                .map_err(storage_error)),
            (Table::RelationsByTarget, Key::Str3(a, b, c)) => {
                write_table!(tx, RELATIONS_BY_TARGET, |t| t
                    .insert((a, b, c), ())
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::EventLog, Key::U64(k)) => {
                write_table!(tx, EVENT_LOG, |t| t
                    .insert(k, value)
                    .map(drop)
                    .map_err(storage_error))
            }
            (table, key) => Err(key_shape_mismatch(table, key.shape())),
        }
    }

    fn remove(&mut self, table: Table, key: Key<'_>) -> Result<(), PortError> {
        let tx = &self.tx;
        match (table, key) {
            (Table::Nodes, Key::Str(k)) => {
                write_table!(tx, NODES, |t| t.remove(k).map(drop).map_err(storage_error))
            }
            (Table::Details, Key::Str(k)) => {
                write_table!(tx, DETAILS, |t| t
                    .remove(k)
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Aggregates, Key::Str(k)) => {
                write_table!(tx, AGGREGATES, |t| t
                    .remove(k)
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Idempotency, Key::Str(k)) => {
                write_table!(tx, IDEMPOTENCY, |t| t
                    .remove(k)
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Migrations, Key::Str(k)) => {
                write_table!(tx, MIGRATIONS, |t| t
                    .remove(k)
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Anchors, Key::Str(k)) => {
                write_table!(tx, ANCHORS, |t| t
                    .remove(k)
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Checkpoints, Key::Str2(a, b)) => {
                write_table!(tx, CHECKPOINTS, |t| t
                    .remove((a, b))
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Snapshots, Key::Str2(a, b)) => {
                write_table!(tx, SNAPSHOTS, |t| t
                    .remove((a, b))
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Processed, Key::Str2(a, b)) => {
                write_table!(tx, PROCESSED, |t| t
                    .remove((a, b))
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::Relations, Key::Str3(a, b, c)) => write_table!(tx, RELATIONS, |t| t
                .remove((a, b, c))
                .map(drop)
                .map_err(storage_error)),
            (Table::RelationsByTarget, Key::Str3(a, b, c)) => {
                write_table!(tx, RELATIONS_BY_TARGET, |t| t
                    .remove((a, b, c))
                    .map(drop)
                    .map_err(storage_error))
            }
            (Table::EventLog, Key::U64(k)) => {
                write_table!(tx, EVENT_LOG, |t| t
                    .remove(k)
                    .map(drop)
                    .map_err(storage_error))
            }
            (table, key) => Err(key_shape_mismatch(table, key.shape())),
        }
    }

    fn clear(&mut self, table: Table) -> Result<(), PortError> {
        match table {
            Table::Nodes => self.clear_typed(NODES),
            Table::Relations => self.clear_typed(RELATIONS),
            Table::RelationsByTarget => self.clear_typed(RELATIONS_BY_TARGET),
            Table::Details => self.clear_typed(DETAILS),
            Table::Anchors => self.clear_typed(ANCHORS),
            Table::EventLog => self.clear_typed(EVENT_LOG),
            Table::Aggregates => self.clear_typed(AGGREGATES),
            Table::Idempotency => self.clear_typed(IDEMPOTENCY),
            Table::Processed => self.clear_typed(PROCESSED),
            Table::Checkpoints => self.clear_typed(CHECKPOINTS),
            Table::Snapshots => self.clear_typed(SNAPSHOTS),
            Table::Migrations => self.clear_typed(MIGRATIONS),
        }
    }

    fn commit(self: Box<Self>) -> Result<(), PortError> {
        self.tx.commit().map_err(commit_error)
    }
}

// ---------------------------------------------------------------- errors --

pub(crate) fn write_begin_error(error: redb::TransactionError) -> PortError {
    PortError::Unavailable(format!("embedded store write transaction failed: {error}"))
}

pub(crate) fn table_error(error: TableError) -> PortError {
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
