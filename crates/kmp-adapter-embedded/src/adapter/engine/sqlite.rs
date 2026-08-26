//! The SQLite engine ([ADR-018](../../../../../archive/docs/adr/ADR-018-multi-process-embedded-store.md))
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
use rusqlite::{Connection, OptionalExtension, config::DbConfig, params};

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
        let connection = open_validated_connection(store_file)?;
        create_tables(&connection)?;
        validate_tables(&connection, store_file)?;
        Ok(Self {
            path: store_file.to_path_buf(),
            pool: Mutex::new(vec![connection]),
        })
    }

    /// Rebuilds the file compactly. `VACUUM` needs the whole database to
    /// itself; the caller guarantees no other handle is open.
    pub(crate) fn compact_file(store_file: &Path) -> Result<bool, PortError> {
        let connection = open_validated_connection(store_file)?;
        create_tables(&connection)?;
        validate_tables(&connection, store_file)?;
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

fn open_validated_connection(store_file: &Path) -> Result<Connection, PortError> {
    let connection = open_connection(store_file)?;
    let integrity: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|error| security_error(store_file, "quick_check", &error))?;
    if integrity != "ok" {
        return Err(PortError::InvalidState(format!(
            "embedded SQLite store `{}` failed quick_check: {integrity}",
            store_file.display()
        )));
    }
    Ok(connection)
}

fn open_connection(store_file: &Path) -> Result<Connection, PortError> {
    let connection = Connection::open(store_file).map_err(|error| {
        PortError::Unavailable(format!(
            "embedded store could not open `{}`: {error}",
            store_file.display()
        ))
    })?;
    harden_connection(&connection, store_file)?;
    // busy_timeout FIRST. Switching the journal mode takes a brief
    // exclusive lock, so two processes opening at the same instant collide
    // there — before WAL is even in effect. Without the timeout already
    // armed, the loser gets SQLITE_BUSY instead of waiting a few
    // milliseconds. The concurrency spike crashed exactly this way.
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| pragma_error(store_file, "busy_timeout", &error))?;
    enter_wal(&connection, store_file)?;
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|error| pragma_error(store_file, "synchronous", &error))?;
    Ok(connection)
}

fn harden_connection(connection: &Connection, store_file: &Path) -> Result<(), PortError> {
    for (config, enabled, name) in [
        (DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true, "defensive mode"),
        (
            DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA,
            false,
            "trusted schema",
        ),
        (DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false, "triggers"),
        (DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW, false, "views"),
    ] {
        connection
            .set_db_config(config, enabled)
            .map_err(|error| security_error(store_file, name, &error))?;
    }
    connection
        .execute_batch("PRAGMA cell_size_check=ON; PRAGMA mmap_size=0;")
        .map_err(|error| security_error(store_file, "safe page access", &error))?;
    Ok(())
}

/// Switches the store into WAL, waiting out a holder rather than giving up.
///
/// `busy_timeout` above is necessary and not sufficient. Switching the
/// journal mode takes a brief exclusive lock, and when another connection
/// already holds a write lock the switch fails *immediately*: the busy
/// handler is not consulted for this one. The holder is another agent host
/// doing exactly what this engine exists to allow, so the only correct answer
/// is to wait for it — bounded by the same timeout every other wait uses, so
/// a genuinely stuck store still reports rather than hanging.
fn enter_wal(connection: &Connection, store_file: &Path) -> Result<(), PortError> {
    let deadline = std::time::Instant::now() + BUSY_TIMEOUT;
    let mut backoff = Duration::from_millis(2);
    loop {
        match connection.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(error) if is_busy(&error) && std::time::Instant::now() < deadline => {
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_millis(64));
            }
            Err(error) => return Err(pragma_error(store_file, "journal_mode", &error)),
        }
    }
}

fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
    )
}

fn create_tables(connection: &Connection) -> Result<(), PortError> {
    let mut ddl = String::new();
    for table in ALL_TABLES {
        ddl.push_str(&table_ddl(table, true));
        ddl.push_str(";\n");
    }
    connection.execute_batch(&ddl).map_err(|error| {
        PortError::Unavailable(format!("embedded store could not create tables: {error}"))
    })
}

fn table_ddl(table: Table, if_not_exists: bool) -> String {
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
    // WITHOUT ROWID clusters text-keyed tables by their key. The u64 table
    // keeps the rowid: INTEGER PRIMARY KEY *is* the rowid.
    let suffix = match table.key_shape() {
        KeyShape::U64 => "",
        _ => " WITHOUT ROWID",
    };
    let guard = if if_not_exists { "IF NOT EXISTS " } else { "" };
    format!("CREATE TABLE {guard}\"{table}\" ({columns}){suffix}")
}

fn validate_tables(connection: &Connection, store_file: &Path) -> Result<(), PortError> {
    for table in ALL_TABLES {
        let mut statement = connection
            .prepare(
                "SELECT type, name, sql FROM sqlite_schema \
                 WHERE tbl_name = ?1 ORDER BY type, name",
            )
            .map_err(|error| security_error(store_file, "schema validation", &error))?;
        let objects = statement
            .query_map(params![table.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|error| security_error(store_file, "schema validation", &error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| security_error(store_file, "schema validation", &error))?;
        let expected_name = table.to_string();
        let expected_ddl = table_ddl(table, false);
        let valid =
            objects.as_slice() == [("table".to_string(), expected_name, Some(expected_ddl))];
        if !valid {
            return Err(PortError::InvalidState(format!(
                "embedded SQLite store `{}` has an unexpected schema for `{table}`; refusing \
                 to execute against a database that may contain injected schema objects",
                store_file.display()
            )));
        }
    }
    Ok(())
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

fn security_error(store_file: &Path, control: &str, error: &rusqlite::Error) -> PortError {
    PortError::Unavailable(format!(
        "embedded store could not enforce SQLite {control} on `{}`: {error}",
        store_file.display()
    ))
}

fn read_error(table: Table, error: &rusqlite::Error) -> PortError {
    PortError::Unavailable(format!("embedded store read on `{table}` failed: {error}"))
}

fn write_error(table: Table, error: &rusqlite::Error) -> PortError {
    PortError::Unavailable(format!("embedded store write on `{table}` failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_connection_treats_the_database_schema_as_untrusted() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store_file = dir.path().join("kernel.sqlite3");

        let connection = open_connection(&store_file).expect("hardened connection opens");

        assert!(
            connection
                .db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE)
                .expect("defensive mode reads back")
        );
        assert!(
            !connection
                .db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA)
                .expect("trusted schema reads back")
        );
        assert!(
            !connection
                .db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER)
                .expect("trigger policy reads back")
        );
        assert!(
            !connection
                .db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW)
                .expect("view policy reads back")
        );
        let cell_size_check: i64 = connection
            .pragma_query_value(None, "cell_size_check", |row| row.get(0))
            .expect("cell size policy reads back");
        let mmap_size: i64 = connection
            .pragma_query_value(None, "mmap_size", |row| row.get(0))
            .expect("mmap policy reads back");
        assert_eq!(cell_size_check, 1);
        assert_eq!(mmap_size, 0);
    }

    #[test]
    fn a_trigger_from_an_existing_database_is_rejected_before_use() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store_file = dir.path().join("kernel.sqlite3");
        let fixture = Connection::open(&store_file).expect("fixture opens");
        create_tables(&fixture).expect("fixture schema");
        fixture
            .execute_batch(
                "CREATE TABLE trigger_probe (value TEXT NOT NULL);\n\
                 CREATE TRIGGER injected_after_node_insert AFTER INSERT ON nodes\n\
                 BEGIN\n\
                   INSERT INTO trigger_probe (value) VALUES ('executed');\n\
                 END;",
            )
            .expect("hostile trigger fixture");
        drop(fixture);

        let error = SqliteEngine::open_file(&store_file)
            .expect_err("a store carrying an injected trigger must be rejected");
        assert!(error.to_string().contains("unexpected schema for `nodes`"));

        let inspection = Connection::open(&store_file).expect("inspection opens");
        let probe_count: i64 = inspection
            .query_row("SELECT COUNT(*) FROM trigger_probe", [], |row| row.get(0))
            .expect("probe count");
        let node_count: i64 = inspection
            .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
            .expect("node count");
        assert_eq!(
            node_count, 0,
            "no application write may run before rejection"
        );
        assert_eq!(probe_count, 0, "the injected trigger must stay inert");
    }

    /// Losing the race to switch a store into WAL must not end the open.
    ///
    /// The switch takes a brief exclusive lock. A connection that holds a
    /// write lock while the database is still in its default journal mode
    /// makes that switch fail — and `busy_timeout`, armed as it already is,
    /// is not consulted for it: the error comes back immediately. Waiting is
    /// the only correct answer, because the holder is another agent host
    /// doing exactly what this engine exists to allow.
    #[test]
    fn an_open_waits_out_a_conversion_it_lost_rather_than_failing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store_file = dir.path().join("kernel.sqlite3");

        let holder = Connection::open(&store_file).expect("holder opens");
        holder
            .pragma_update(None, "journal_mode", "delete")
            .expect("holder keeps the default journal mode");
        holder
            .execute_batch("CREATE TABLE t (k INTEGER PRIMARY KEY)")
            .expect("holder creates something to lock");
        holder
            .execute_batch("BEGIN IMMEDIATE")
            .expect("holder takes the write lock");

        let releasing = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            holder.execute_batch("ROLLBACK").expect("holder lets go");
            drop(holder);
        });

        let started = std::time::Instant::now();
        let connection = open_connection(&store_file).expect("the open waits and then succeeds");
        let waited = started.elapsed();
        releasing.join().expect("holder thread finishes");

        let mode: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .expect("journal mode reads back");
        assert_eq!(mode, "wal", "the store must end up in WAL, not merely open");
        assert!(
            waited >= Duration::from_millis(200),
            "it must have waited for the holder rather than racing past it, waited {waited:?}"
        );
    }
}
