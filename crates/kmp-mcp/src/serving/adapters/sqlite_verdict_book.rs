use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, ErrorCode, OptionalExtension, TransactionBehavior, params};

use crate::serving::ports::verdict_book::VerdictBook;
use crate::serving::verdict::Verdict;
use crate::serving::verdict_key::VerdictKey;

#[cfg(test)]
#[path = "sqlite_verdict_book_tests.rs"]
mod tests;

/// The book's layout; a newer one is refused rather than misread.
const FORMAT: i64 = 1;
/// Writes between two checks of the byte budget.
const CHECK_EVERY: usize = 256;
/// Collection brings the book down to this share of its budget, so it does
/// not run again on the next write.
const COLLECT_TO_PERCENT: u64 = 80;

/// The verdict book on SQLite (DESIGN L4 4a): a sidecar file with its own
/// WAL, so judging never competes with the kernel's writer. One WITHOUT
/// ROWID table `verdicts(key BLOB PK, value BLOB, last_used)`; `last_used`
/// is a day number, refreshed at most once a day per verdict so reads stay
/// reads. Several processes may share it: `INSERT OR IGNORE` then a
/// re-read makes the first verdict win for all of them.
pub(super) struct SqliteVerdictBook {
    connection: Mutex<Connection>,
    max_bytes: u64,
    writes: Mutex<usize>,
}

fn storage(error: impl std::fmt::Display) -> String {
    format!("verdict book: {error}")
}

fn today() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| (elapsed.as_secs() / 86_400) as i64)
}

fn is_full(error: &rusqlite::Error) -> bool {
    error.sqlite_error_code() == Some(ErrorCode::DiskFull)
}

impl SqliteVerdictBook {
    /// Opens or creates the book at `path`. Past `max_bytes` of verdicts
    /// the least recently used go first; the file can never grow past twice
    /// `max_bytes`.
    pub(super) fn open(path: &Path, max_bytes: u64) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(storage)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(storage)?;
        // Only takes effect before the first table exists: a new book can
        // hand pages back after collection.
        connection
            .execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")
            .map_err(storage)?;
        connection
            .query_row("PRAGMA journal_mode = WAL", [], |row| {
                row.get::<_, String>(0)
            })
            .map_err(storage)?;
        connection
            .execute_batch("PRAGMA synchronous = NORMAL;")
            .map_err(storage)?;
        let page_size = connection
            .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
            .map_err(storage)? as u64;
        let pages = (max_bytes.saturating_mul(2) / page_size.max(1)).max(16);
        connection
            .query_row(&format!("PRAGMA max_page_count = {pages}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(storage)?;
        Self::migrate(&connection)?;
        let book = Self {
            connection: Mutex::new(connection),
            max_bytes,
            writes: Mutex::new(0),
        };
        book.collect_garbage()?;
        Ok(book)
    }

    fn migrate(connection: &Connection) -> Result<(), String> {
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(storage)?;
        match version {
            FORMAT => return Ok(()),
            0 => {}
            newer => return Err(storage(format!("format {newer} is newer than {FORMAT}"))),
        }
        connection
            .execute_batch(&format!(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS verdicts(
                     key BLOB PRIMARY KEY NOT NULL,
                     value BLOB NOT NULL,
                     last_used INTEGER NOT NULL
                 ) WITHOUT ROWID;
                 CREATE INDEX IF NOT EXISTS verdicts_by_last_used ON verdicts(last_used, key);
                 PRAGMA user_version = {FORMAT};
                 COMMIT;"
            ))
            .map_err(storage)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Bytes the verdicts occupy: the file's pages minus the free ones.
    #[cfg(test)]
    pub(super) fn bytes_used(&self) -> Result<u64, String> {
        used_bytes(&self.lock())
    }

    /// Verdicts held.
    #[cfg(test)]
    pub(super) fn len(&self) -> Result<u64, String> {
        self.lock()
            .query_row("SELECT COUNT(*) FROM verdicts", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count as u64)
            .map_err(storage)
    }

    /// Past the byte budget, drops the least recently used verdicts until
    /// the book is back under `COLLECT_TO_PERCENT` of it, then returns the
    /// freed pages to the file system. Returns how many were dropped.
    pub(super) fn collect_garbage(&self) -> Result<u64, String> {
        let connection = self.lock();
        collect(&connection, self.max_bytes)
    }

    fn try_record(
        connection: &mut Connection,
        verdicts: &[(VerdictKey, Verdict)],
    ) -> Result<Vec<Verdict>, rusqlite::Error> {
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let day = today();
        let mut held = Vec::with_capacity(verdicts.len());
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR IGNORE INTO verdicts(key, value, last_used) VALUES (?1, ?2, ?3)",
            )?;
            let mut select = tx.prepare_cached("SELECT value FROM verdicts WHERE key = ?1")?;
            let mut repair = tx.prepare_cached("UPDATE verdicts SET value = ?2 WHERE key = ?1")?;
            for (key, verdict) in verdicts {
                insert.execute(params![key.as_bytes(), verdict.encode(), day])?;
                let bytes: Vec<u8> = select.query_row(params![key.as_bytes()], |row| row.get(0))?;
                match Verdict::decode(&bytes) {
                    Some(first) => held.push(first),
                    None => {
                        // Bytes no verdict decodes from (a torn or foreign
                        // row) never win over a real verdict.
                        repair.execute(params![key.as_bytes(), verdict.encode()])?;
                        held.push(verdict.clone());
                    }
                }
            }
        }
        tx.commit()?;
        Ok(held)
    }
}

fn used_bytes(connection: &Connection) -> Result<u64, String> {
    let pragma = |name: &str| -> Result<u64, String> {
        connection
            .query_row(&format!("PRAGMA {name}"), [], |row| row.get::<_, i64>(0))
            .map(|value| value as u64)
            .map_err(storage)
    };
    let page_size = pragma("page_size")?;
    Ok(pragma("page_count")?.saturating_sub(pragma("freelist_count")?) * page_size)
}

fn collect(connection: &Connection, max_bytes: u64) -> Result<u64, String> {
    let used = used_bytes(connection)?;
    if used <= max_bytes {
        return Ok(0);
    }
    let count = connection
        .query_row("SELECT COUNT(*) FROM verdicts", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(storage)? as u64;
    if count == 0 {
        return Ok(0);
    }
    let per_verdict = (used / count).max(1);
    let target = max_bytes / 100 * COLLECT_TO_PERCENT;
    let excess = (used - target.min(used))
        .div_ceil(per_verdict)
        .clamp(1, count);
    let dropped = connection
        .execute(
            "DELETE FROM verdicts WHERE key IN
               (SELECT key FROM verdicts ORDER BY last_used, key LIMIT ?1)",
            params![excess as i64],
        )
        .map_err(storage)?;
    connection
        .execute_batch("PRAGMA incremental_vacuum;")
        .map_err(storage)?;
    Ok(dropped as u64)
}

/// The smallest byte string greater than every string starting with
/// `prefix`; `None` when there is none (all 0xFF).
fn successor(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut next = prefix.to_vec();
    while let Some(last) = next.pop() {
        if last < u8::MAX {
            next.push(last + 1);
            return Some(next);
        }
    }
    None
}

impl VerdictBook for SqliteVerdictBook {
    fn read(&self, keys: &[VerdictKey]) -> Result<Vec<Option<Verdict>>, String> {
        let connection = self.lock();
        let day = today();
        let mut stale = Vec::new();
        let mut found = Vec::with_capacity(keys.len());
        {
            let mut select = connection
                .prepare_cached("SELECT value, last_used FROM verdicts WHERE key = ?1")
                .map_err(storage)?;
            for key in keys {
                let row = select
                    .query_row(params![key.as_bytes()], |row| {
                        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
                    })
                    .optional()
                    .map_err(storage)?;
                found.push(row.and_then(|(bytes, last_used)| {
                    if last_used < day {
                        stale.push(key);
                    }
                    Verdict::decode(&bytes)
                }));
            }
        }
        if !stale.is_empty() {
            // Best effort: a busy book keeps yesterday's date, never fails a read.
            let _ = (|| -> rusqlite::Result<()> {
                let tx = connection.unchecked_transaction()?;
                {
                    let mut touch = tx.prepare_cached(
                        "UPDATE verdicts SET last_used = ?2 WHERE key = ?1 AND last_used < ?2",
                    )?;
                    for key in stale {
                        touch.execute(params![key.as_bytes(), day])?;
                    }
                }
                tx.commit()
            })();
        }
        Ok(found)
    }

    fn record(&self, verdicts: &[(VerdictKey, Verdict)]) -> Result<Vec<Verdict>, String> {
        if verdicts.is_empty() {
            return Ok(Vec::new());
        }
        let mut connection = self.lock();
        let held = match Self::try_record(&mut connection, verdicts) {
            Err(error) if is_full(&error) => {
                // The file hit its hard cap: make room once, then try again.
                collect(&connection, self.max_bytes / 2)?;
                Self::try_record(&mut connection, verdicts).map_err(storage)?
            }
            other => other.map_err(storage)?,
        };
        let mut writes = self.writes.lock().unwrap_or_else(|p| p.into_inner());
        *writes += verdicts.len();
        if *writes >= CHECK_EVERY {
            *writes = 0;
            collect(&connection, self.max_bytes)?;
        }
        Ok(held)
    }

    fn invalidate(&self, prefix: &[u8]) -> Result<usize, String> {
        let connection = self.lock();
        let dropped = match successor(prefix) {
            Some(end) => connection.execute(
                "DELETE FROM verdicts WHERE key >= ?1 AND key < ?2",
                params![prefix, end],
            ),
            None => connection.execute("DELETE FROM verdicts WHERE key >= ?1", params![prefix]),
        }
        .map_err(storage)?;
        connection
            .execute_batch("PRAGMA incremental_vacuum;")
            .map_err(storage)?;
        Ok(dropped)
    }
}
