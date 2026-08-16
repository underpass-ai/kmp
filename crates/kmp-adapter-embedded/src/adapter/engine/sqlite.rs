//! The SQLite engine ([ADR-018](../../../../../docs/adr/ADR-018-multi-process-embedded-store.md))
//! behind the seam. Opt-in via the `sqlite` feature.
//!
//! One SQL table per seam table, keyed by the seam's key shape. Text keys are
//! `WITHOUT ROWID` with the key columns as primary key, so an ordered scan is
//! an index walk and "every row whose first component is X" is a range on
//! the primary key — the adjacency query stays cheap. The `u64` table uses
//! `INTEGER PRIMARY KEY`, SQLite's rowid, which is the fastest key it has.
//!
//! Ordering matches the seam contract without any collation work: SQLite's
//! default `BINARY` collation compares text bytewise, which is what Rust's
//! `str` ordering does, and integers are integers.
//!
//! What makes this engine different from redb, and the reason it exists:
//! WAL mode. Readers never block the writer, and a second process wanting to
//! write waits for the commit lock instead of being refused. Two agent hosts
//! open the same store and both work.
//!
//! Durability is `synchronous=FULL`: every commit reaches the disk before it
//! returns, matching the crash contract the redb engine gives — no loss
//! beyond the in-flight event.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use kmp_domain::PortError;
use rusqlite::{Connection, OptionalExtension, params};

use super::{
    Engine, Key, KeyShape, ReadTx, Str3Row, StrRow, Table, U64Row, WriteTx, key_shape_mismatch,
    scan_shape_mismatch,
};

/// How long a transaction waits for another process's commit before giving
/// up. Interactive agents commit in milliseconds; ten seconds means "the
/// other side is stuck", not "the other side is busy".
const BUSY_TIMEOUT: Duration = Duration::from_secs(10);

const ALL_TABLES: [Table; 12] = [
    Table::Nodes,
    Table::Relations,
    Table::RelationsByTarget,
    Table::Details,
    Table::Anchors,
    Table::EventLog,
    Table::Aggregates,
    Table::Idempotency,
    Table::Processed,
    Table::Checkpoints,
    Table::Snapshots,
    Table::Migrations,
];

// ---------------------------------------------------------------- engine --

/// One open SQLite file, with a small pool of connections so concurrent
/// tasks inside this process each get their own snapshot.
#[derive(Debug)]
pub(crate) struct SqliteEngine {
    path: PathBuf,
    pool: Mutex<Vec<Connection>>,
}

impl SqliteEngine {
    /// Opens (or creates) `store_file`, switches it to WAL, and creates every
    /// table so readers never race table existence.
    pub(crate) fn open_file(store_file: &Path) -> Result<Self, PortError> {
        let connection = open_connection(store_file)?;
        create_tables(&connection)?;
        Ok(Self {
            path: store_file.to_path_buf(),
            pool: Mutex::new(vec![connection]),
        })
    }

    /// Rebuilds the file compactly. `VACUUM` needs the whole database to
    /// itself; the caller guarantees no other handle is open.
    pub(crate) fn compact_file(store_file: &Path) -> Result<bool, PortError> {
        let connection = open_connection(store_file)?;
        connection.execute_batch("VACUUM").map_err(|error| {
            PortError::Unavailable(format!(
                "embedded store compaction failed for `{}`: {error}",
                store_file.display()
            ))
        })?;
        Ok(true)
    }

    fn take_connection(&self) -> Result<Pooled<'_>, PortError> {
        let reused = self.pool.lock().map_err(|_| poisoned())?.pop();
        let connection = match reused {
            Some(connection) => connection,
            None => open_connection(&self.path)?,
        };
        Ok(Pooled {
            connection: Some(connection),
            pool: &self.pool,
        })
    }
}

impl Engine for SqliteEngine {
    fn begin_read(&self) -> Result<Box<dyn ReadTx + '_>, PortError> {
        let connection = self.take_connection()?;
        // A deferred BEGIN: the snapshot is taken at the first read and
        // held until the transaction ends, which is what makes a multi-step
        // graph walk see one consistent store.
        connection.execute_batch("BEGIN").map_err(|error| {
            PortError::Unavailable(format!("embedded store read transaction failed: {error}"))
        })?;
        Ok(Box::new(SqliteRead { connection }))
    }

    fn begin_write(&self) -> Result<Box<dyn WriteTx + '_>, PortError> {
        let connection = self.take_connection()?;
        // IMMEDIATE takes the write lock now, waiting up to BUSY_TIMEOUT for
        // another process to finish committing. A deferred BEGIN would take
        // it on the first write and could then be refused after reads were
        // already done — the classic upgrade deadlock. This is the line that
        // lets a second process write instead of being turned away.
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|error| {
                PortError::Unavailable(format!("embedded store write transaction failed: {error}"))
            })?;
        Ok(Box::new(SqliteWrite { connection }))
    }
}

fn open_connection(store_file: &Path) -> Result<Connection, PortError> {
    let connection = Connection::open(store_file).map_err(|error| {
        PortError::Unavailable(format!(
            "embedded store could not open `{}`: {error}",
            store_file.display()
        ))
    })?;
    // busy_timeout FIRST. Switching the journal mode takes a brief
    // exclusive lock, so two processes opening at the same instant collide
    // there — before WAL is even in effect. Without the timeout already
    // armed, the loser gets SQLITE_BUSY instead of waiting a few
    // milliseconds. The concurrency spike crashed exactly this way.
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| pragma_error(store_file, "busy_timeout", &error))?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| pragma_error(store_file, "journal_mode", &error))?;
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|error| pragma_error(store_file, "synchronous", &error))?;
    Ok(connection)
}

fn create_tables(connection: &Connection) -> Result<(), PortError> {
    let mut ddl = String::new();
    for table in ALL_TABLES {
        let columns = match table.key_shape() {
            KeyShape::Str => "k TEXT NOT NULL, v BLOB NOT NULL, PRIMARY KEY (k)",
            KeyShape::Str2 => {
                "k1 TEXT NOT NULL, k2 TEXT NOT NULL, v BLOB NOT NULL, PRIMARY KEY (k1, k2)"
            }
            KeyShape::Str3 => {
                "k1 TEXT NOT NULL, k2 TEXT NOT NULL, k3 TEXT NOT NULL, v BLOB NOT NULL, \
                 PRIMARY KEY (k1, k2, k3)"
            }
            KeyShape::U64 => "k INTEGER PRIMARY KEY, v BLOB NOT NULL",
        };
        // WITHOUT ROWID clusters text-keyed tables by their key. The u64
        // table keeps the rowid: INTEGER PRIMARY KEY *is* the rowid.
        let suffix = match table.key_shape() {
            KeyShape::U64 => "",
            _ => " WITHOUT ROWID",
        };
        ddl.push_str(&format!(
            "CREATE TABLE IF NOT EXISTS \"{table}\" ({columns}){suffix};\n"
        ));
    }
    connection.execute_batch(&ddl).map_err(|error| {
        PortError::Unavailable(format!("embedded store could not create tables: {error}"))
    })
}

// ------------------------------------------------------------- pooling --

/// A connection borrowed from the engine's pool, returned on drop with any
/// half-open transaction rolled back.
struct Pooled<'e> {
    connection: Option<Connection>,
    pool: &'e Mutex<Vec<Connection>>,
}

impl Pooled<'_> {
    fn get(&self) -> &Connection {
        self.connection
            .as_ref()
            .expect("pooled connection is present until drop")
    }
}

impl std::ops::Deref for Pooled<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.get()
    }
}

impl Drop for Pooled<'_> {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            // A transaction still open here means the owner never committed:
            // roll it back so the connection is clean for the next borrower.
            // Both are best effort — nothing sensible can be done about a
            // failure inside drop, and a connection that will not roll back
            // is simply not returned to the pool.
            if !connection.is_autocommit() && connection.execute_batch("ROLLBACK").is_err() {
                return;
            }
            if let Ok(mut pool) = self.pool.lock() {
                pool.push(connection);
            }
        }
    }
}

// ----------------------------------------------------------- statements --

fn where_clause(shape: KeyShape) -> &'static str {
    match shape {
        KeyShape::Str | KeyShape::U64 => "k = ?1",
        KeyShape::Str2 => "k1 = ?1 AND k2 = ?2",
        KeyShape::Str3 => "k1 = ?1 AND k2 = ?2 AND k3 = ?3",
    }
}

fn key_columns(shape: KeyShape) -> &'static str {
    match shape {
        KeyShape::Str | KeyShape::U64 => "k",
        KeyShape::Str2 => "k1, k2",
        KeyShape::Str3 => "k1, k2, k3",
    }
}

fn u64_to_sql(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| {
        PortError::InvalidState(format!(
            "embedded store: sequence {value} does not fit a SQLite integer"
        ))
    })
}

fn sql_to_u64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| {
        PortError::InvalidState(format!(
            "embedded store: SQLite returned a negative sequence ({value})"
        ))
    })
}

fn check_key(table: Table, key: Key<'_>) -> Result<(), PortError> {
    if table.key_shape() == key.shape() {
        Ok(())
    } else {
        Err(key_shape_mismatch(table, key.shape()))
    }
}

/// Every seam operation on one connection. Both transaction types delegate
/// here; the difference between them is only which `BEGIN` they issued.
struct Ops<'c> {
    connection: &'c Connection,
}

impl Ops<'_> {
    fn get(&self, table: Table, key: Key<'_>) -> Result<Option<Vec<u8>>, PortError> {
        check_key(table, key)?;
        let sql = format!(
            "SELECT v FROM \"{table}\" WHERE {}",
            where_clause(table.key_shape())
        );
        let mut statement = self.prepare(&sql)?;
        let result = match key {
            Key::Str(k) => statement.query_row(params![k], |row| row.get::<_, Vec<u8>>(0)),
            Key::Str2(a, b) => statement.query_row(params![a, b], |row| row.get(0)),
            Key::Str3(a, b, c) => statement.query_row(params![a, b, c], |row| row.get(0)),
            Key::U64(k) => statement.query_row(params![u64_to_sql(k)?], |row| row.get(0)),
        };
        result.optional().map_err(|error| read_error(table, &error))
    }

    fn scan_str(&self, table: Table) -> Result<Vec<StrRow>, PortError> {
        if table.key_shape() != KeyShape::Str {
            return Err(scan_shape_mismatch(table, KeyShape::Str));
        }
        let sql = format!("SELECT k, v FROM \"{table}\" ORDER BY k");
        let mut statement = self.prepare(&sql)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|error| read_error(table, &error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| read_error(table, &error))
    }

    fn scan_str3_by_first(&self, table: Table, first: &str) -> Result<Vec<Str3Row>, PortError> {
        if table.key_shape() != KeyShape::Str3 {
            return Err(scan_shape_mismatch(table, KeyShape::Str3));
        }
        let sql =
            format!("SELECT k1, k2, k3, v FROM \"{table}\" WHERE k1 = ?1 ORDER BY k1, k2, k3");
        let mut statement = self.prepare(&sql)?;
        let rows = statement
            .query_map(params![first], |row| {
                Ok((
                    (
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ),
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })
            .map_err(|error| read_error(table, &error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| read_error(table, &error))
    }

    fn scan_u64(&self, table: Table) -> Result<Vec<U64Row>, PortError> {
        if table.key_shape() != KeyShape::U64 {
            return Err(scan_shape_mismatch(table, KeyShape::U64));
        }
        let sql = format!("SELECT k, v FROM \"{table}\" ORDER BY k");
        let mut statement = self.prepare(&sql)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|error| read_error(table, &error))?;
        rows.map(|row| {
            let (key, value) = row.map_err(|error| read_error(table, &error))?;
            Ok((sql_to_u64(key)?, value))
        })
        .collect()
    }

    fn last_u64(&self, table: Table) -> Result<Option<U64Row>, PortError> {
        if table.key_shape() != KeyShape::U64 {
            return Err(scan_shape_mismatch(table, KeyShape::U64));
        }
        let sql = format!("SELECT k, v FROM \"{table}\" ORDER BY k DESC LIMIT 1");
        let mut statement = self.prepare(&sql)?;
        let row = statement
            .query_row([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .optional()
            .map_err(|error| read_error(table, &error))?;
        row.map(|(key, value)| Ok((sql_to_u64(key)?, value)))
            .transpose()
    }

    fn count(&self, table: Table) -> Result<u64, PortError> {
        let sql = format!("SELECT COUNT(*) FROM \"{table}\"");
        let mut statement = self.prepare(&sql)?;
        let count: i64 = statement
            .query_row([], |row| row.get(0))
            .map_err(|error| read_error(table, &error))?;
        sql_to_u64(count)
    }

    fn insert(&self, table: Table, key: Key<'_>, value: &[u8]) -> Result<(), PortError> {
        check_key(table, key)?;
        let shape = table.key_shape();
        let sql = format!(
            "INSERT INTO \"{table}\" ({cols}, v) VALUES ({marks}, ?{v}) \
             ON CONFLICT ({cols}) DO UPDATE SET v = excluded.v",
            cols = key_columns(shape),
            marks = match shape {
                KeyShape::Str | KeyShape::U64 => "?1",
                KeyShape::Str2 => "?1, ?2",
                KeyShape::Str3 => "?1, ?2, ?3",
            },
            v = match shape {
                KeyShape::Str | KeyShape::U64 => 2,
                KeyShape::Str2 => 3,
                KeyShape::Str3 => 4,
            },
        );
        let mut statement = self.prepare(&sql)?;
        let result = match key {
            Key::Str(k) => statement.execute(params![k, value]),
            Key::Str2(a, b) => statement.execute(params![a, b, value]),
            Key::Str3(a, b, c) => statement.execute(params![a, b, c, value]),
            Key::U64(k) => statement.execute(params![u64_to_sql(k)?, value]),
        };
        result.map(drop).map_err(|error| write_error(table, &error))
    }

    fn remove(&self, table: Table, key: Key<'_>) -> Result<(), PortError> {
        check_key(table, key)?;
        let sql = format!(
            "DELETE FROM \"{table}\" WHERE {}",
            where_clause(table.key_shape())
        );
        let mut statement = self.prepare(&sql)?;
        let result = match key {
            Key::Str(k) => statement.execute(params![k]),
            Key::Str2(a, b) => statement.execute(params![a, b]),
            Key::Str3(a, b, c) => statement.execute(params![a, b, c]),
            Key::U64(k) => statement.execute(params![u64_to_sql(k)?]),
        };
        result.map(drop).map_err(|error| write_error(table, &error))
    }

    fn clear(&self, table: Table) -> Result<(), PortError> {
        let sql = format!("DELETE FROM \"{table}\"");
        self.connection
            .execute_batch(&sql)
            .map_err(|error| write_error(table, &error))
    }

    fn prepare(&self, sql: &str) -> Result<rusqlite::CachedStatement<'_>, PortError> {
        // The statement cache is per connection; with a fixed table set the
        // handful of distinct SQL strings are compiled once per connection.
        self.connection.prepare_cached(sql).map_err(|error| {
            PortError::Unavailable(format!("embedded store could not prepare `{sql}`: {error}"))
        })
    }
}

// ----------------------------------------------------------- read txn --

struct SqliteRead<'e> {
    connection: Pooled<'e>,
}

impl SqliteRead<'_> {
    fn ops(&self) -> Ops<'_> {
        Ops {
            connection: &self.connection,
        }
    }
}

impl ReadTx for SqliteRead<'_> {
    fn get(&self, table: Table, key: Key<'_>) -> Result<Option<Vec<u8>>, PortError> {
        self.ops().get(table, key)
    }
    fn scan_str(&self, table: Table) -> Result<Vec<StrRow>, PortError> {
        self.ops().scan_str(table)
    }
    fn scan_str3_by_first(&self, table: Table, first: &str) -> Result<Vec<Str3Row>, PortError> {
        self.ops().scan_str3_by_first(table, first)
    }
    fn scan_u64(&self, table: Table) -> Result<Vec<U64Row>, PortError> {
        self.ops().scan_u64(table)
    }
    fn last_u64(&self, table: Table) -> Result<Option<U64Row>, PortError> {
        self.ops().last_u64(table)
    }
    fn count(&self, table: Table) -> Result<u64, PortError> {
        self.ops().count(table)
    }
}

// ---------------------------------------------------------- write txn --

struct SqliteWrite<'e> {
    connection: Pooled<'e>,
}

impl SqliteWrite<'_> {
    fn ops(&self) -> Ops<'_> {
        Ops {
            connection: &self.connection,
        }
    }
}

impl ReadTx for SqliteWrite<'_> {
    fn get(&self, table: Table, key: Key<'_>) -> Result<Option<Vec<u8>>, PortError> {
        self.ops().get(table, key)
    }
    fn scan_str(&self, table: Table) -> Result<Vec<StrRow>, PortError> {
        self.ops().scan_str(table)
    }
    fn scan_str3_by_first(&self, table: Table, first: &str) -> Result<Vec<Str3Row>, PortError> {
        self.ops().scan_str3_by_first(table, first)
    }
    fn scan_u64(&self, table: Table) -> Result<Vec<U64Row>, PortError> {
        self.ops().scan_u64(table)
    }
    fn last_u64(&self, table: Table) -> Result<Option<U64Row>, PortError> {
        self.ops().last_u64(table)
    }
    fn count(&self, table: Table) -> Result<u64, PortError> {
        self.ops().count(table)
    }
}

impl WriteTx for SqliteWrite<'_> {
    fn insert(&mut self, table: Table, key: Key<'_>, value: &[u8]) -> Result<(), PortError> {
        self.ops().insert(table, key, value)
    }
    fn remove(&mut self, table: Table, key: Key<'_>) -> Result<(), PortError> {
        self.ops().remove(table, key)
    }
    fn clear(&mut self, table: Table) -> Result<(), PortError> {
        self.ops().clear(table)
    }
    fn commit(self: Box<Self>) -> Result<(), PortError> {
        // After COMMIT the connection is back in autocommit, so the pooled
        // drop that follows returns it clean instead of rolling anything back.
        self.connection.execute_batch("COMMIT").map_err(|error| {
            PortError::Unavailable(format!("embedded store commit failed: {error}"))
        })
    }
}

// ---------------------------------------------------------------- errors --

fn poisoned() -> PortError {
    PortError::Unavailable("embedded store connection pool is poisoned".to_string())
}

fn pragma_error(store_file: &Path, pragma: &str, error: &rusqlite::Error) -> PortError {
    PortError::Unavailable(format!(
        "embedded store could not set {pragma} on `{}`: {error}",
        store_file.display()
    ))
}

fn read_error(table: Table, error: &rusqlite::Error) -> PortError {
    PortError::Unavailable(format!("embedded store read on `{table}` failed: {error}"))
}

fn write_error(table: Table, error: &rusqlite::Error) -> PortError {
    PortError::Unavailable(format!("embedded store write on `{table}` failed: {error}"))
}
