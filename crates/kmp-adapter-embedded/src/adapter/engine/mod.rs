//! The storage seam ([ADR-018](../../../../../archive/docs/adr/ADR-018-multi-process-embedded-store.md)).
//!
//! Everything the kernel ports need from a storage engine, and nothing an
//! engine would need to know about the kernel: eleven key-to-bytes maps,
//! transactions over them, and four key shapes. Port logic — graph
//! traversal, revision checks, idempotency — is written once against this
//! and never sees an engine type.
//!
//! The seam is deliberately narrow. Every method here corresponds to an
//! operation the port code already performed against redb, and no more:
//! point get, insert, remove, a full ordered scan, a scan of one first key
//! component, the last row of a `u64`-keyed table, a row count, and a table
//! clear. Rows come back in ascending key order, compared component by
//! component and byte-wise within a component; both engines can promise that
//! and the neighborhood output depends on it.
//!
//! Scans return `Vec` rather than an iterator. Every caller collected before
//! this seam existed, so nothing is lost, and it keeps the trait object-safe
//! without a lifetime tying a row to its transaction.

use std::fmt;

use kmp_domain::PortError;

pub(crate) mod redb;
pub(crate) mod sqlite;

/// The tables a kernel store consists of. Every engine materializes all of
/// them; the key shape of each is fixed and recorded on the variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Table {
    /// Graph nodes: `node_id -> NodeRecord`.
    Nodes,
    /// Outgoing adjacency: `(source, target, relation_type) -> explanation`.
    Relations,
    /// Incoming adjacency index: `(target, source, relation_type) -> ()`.
    RelationsByTarget,
    /// Node details: `node_id -> DetailRecord`.
    Details,
    /// Memory anchor index: `node_id -> ()`.
    Anchors,
    /// Append-only context event log: `sequence -> ContextUpdatedEvent`.
    EventLog,
    /// Aggregate heads: `"root\u{1f}role" -> AggregateRecord`.
    Aggregates,
    /// Idempotency outcomes: `key -> IdempotentOutcome`.
    Idempotency,
    /// Projection-consumer dedup: `(consumer, event_id) -> ()`.
    Processed,
    /// Projection checkpoints: `(consumer, stream) -> CheckpointRecord`.
    Checkpoints,
    /// Snapshot audit records: `(root, role) -> snapshot summary`.
    Snapshots,
    /// Migration receipts: `migration_id -> StoreMigrationReceipt`. Created
    /// lazily by the first migration; a store nobody migrated has none, and
    /// reads it as empty.
    Migrations,
}

impl Table {
    /// The key shape this table is defined with. A call carrying a key of a
    /// different shape is a programming error inside this crate, and the
    /// engine reports it as one instead of guessing.
    pub(crate) const fn key_shape(self) -> KeyShape {
        match self {
            Table::Nodes
            | Table::Details
            | Table::Anchors
            | Table::Aggregates
            | Table::Idempotency
            | Table::Migrations => KeyShape::Str,
            Table::Processed | Table::Checkpoints | Table::Snapshots => KeyShape::Str2,
            Table::Relations | Table::RelationsByTarget => KeyShape::Str3,
            Table::EventLog => KeyShape::U64,
        }
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Table::Nodes => "nodes",
            Table::Relations => "relations_by_source",
            Table::RelationsByTarget => "relations_by_target",
            Table::Details => "details",
            Table::Anchors => "memory_anchors",
            Table::EventLog => "event_log",
            Table::Aggregates => "aggregates",
            Table::Idempotency => "idempotency",
            Table::Processed => "processed_events",
            Table::Checkpoints => "projection_checkpoints",
            Table::Snapshots => "snapshots",
            Table::Migrations => "store_migrations",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyShape {
    Str,
    Str2,
    Str3,
    U64,
}

/// A borrowed key in one of the four shapes the tables use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key<'a> {
    Str(&'a str),
    Str2(&'a str, &'a str),
    Str3(&'a str, &'a str, &'a str),
    U64(u64),
}

impl Key<'_> {
    pub(crate) const fn shape(&self) -> KeyShape {
        match self {
            Key::Str(_) => KeyShape::Str,
            Key::Str2(..) => KeyShape::Str2,
            Key::Str3(..) => KeyShape::Str3,
            Key::U64(_) => KeyShape::U64,
        }
    }
}

/// A row read back from a `Str`-keyed table.
pub(crate) type StrRow = (String, Vec<u8>);
/// A row read back from a `Str3`-keyed table.
pub(crate) type Str3Row = ((String, String, String), Vec<u8>);
/// A row read back from a `U64`-keyed table.
pub(crate) type U64Row = (u64, Vec<u8>);

/// A read transaction: a consistent snapshot of every table.
pub(crate) trait ReadTx {
    /// The value at `key`, if any. Unit-valued tables answer `Some(vec![])`
    /// for a present key.
    fn get(&self, table: Table, key: Key<'_>) -> Result<Option<Vec<u8>>, PortError>;

    /// Every row of a `Str`-keyed table, ascending.
    fn scan_str(&self, table: Table) -> Result<Vec<StrRow>, PortError>;

    /// Every row of a `Str3`-keyed table whose first component equals
    /// `first`, ascending by the remaining components. This is the adjacency
    /// query: all edges out of (or into) one node.
    fn scan_str3_by_first(&self, table: Table, first: &str) -> Result<Vec<Str3Row>, PortError>;

    /// Every row of a `U64`-keyed table, ascending.
    fn scan_u64(&self, table: Table) -> Result<Vec<U64Row>, PortError>;

    /// The highest-keyed row of a `U64`-keyed table.
    fn last_u64(&self, table: Table) -> Result<Option<U64Row>, PortError>;

    /// Number of rows in `table`.
    fn count(&self, table: Table) -> Result<u64, PortError>;
}

/// A write transaction. Reads see this transaction's own writes; nothing is
/// durable until [`commit`](WriteTx::commit) returns, and dropping the
/// transaction discards everything it did.
pub(crate) trait WriteTx: ReadTx {
    /// Upsert. Unit-valued tables ignore `value`.
    fn insert(&mut self, table: Table, key: Key<'_>, value: &[u8]) -> Result<(), PortError>;

    /// Remove `key` if present. Removing an absent key is not an error.
    fn remove(&mut self, table: Table, key: Key<'_>) -> Result<(), PortError>;

    /// Empty `table`. The table still exists afterwards: a reader following
    /// this commit finds it empty, never missing.
    fn clear(&mut self, table: Table) -> Result<(), PortError>;

    /// Make every write in this transaction durable. On return the data has
    /// reached the disk with the durability the engine was opened with.
    fn commit(self: Box<Self>) -> Result<(), PortError>;
}

/// A storage engine: one opened kernel store, shareable across tasks.
pub(crate) trait Engine: fmt::Debug + Send + Sync {
    fn begin_read(&self) -> Result<Box<dyn ReadTx + '_>, PortError>;
    fn begin_write(&self) -> Result<Box<dyn WriteTx + '_>, PortError>;
}

pub(crate) fn key_shape_mismatch(table: Table, key: KeyShape) -> PortError {
    PortError::InvalidState(format!(
        "embedded engine: table `{table}` is keyed by {:?}, called with a {key:?} key",
        table.key_shape()
    ))
}

pub(crate) fn scan_shape_mismatch(table: Table, wanted: KeyShape) -> PortError {
    PortError::InvalidState(format!(
        "embedded engine: table `{table}` is keyed by {:?}, scanned as {wanted:?}",
        table.key_shape()
    ))
}
