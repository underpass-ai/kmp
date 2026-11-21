use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::continuation::{
    ContinuationError, ContinuationStore, ReadContinuation, ReadContinuationId,
};

/// A retained call larger than this is returned whole instead.
const MAX_CALL_BYTES: usize = 32 * 1024;
/// Calls retained per store; the oldest is evicted first.
const MAX_CALLS: u32 = 256;
/// Seconds a handle stays resolvable.
const RETENTION_SECONDS: u32 = 86_400;

/// Retained continuations beside the store, in their own file. Transport
/// metadata only: it never enters recall, evidence or a bundle.
pub(crate) struct SqliteContinuationStore {
    connection: Mutex<Connection>,
}

fn storage(error: impl std::fmt::Display) -> ContinuationError {
    ContinuationError::Unavailable(format!("continuation store: {error}"))
}

impl SqliteContinuationStore {
    pub(crate) fn at(path: &Path) -> Result<Self, ContinuationError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(storage)?;
        }
        Self::from_connection(Connection::open(path).map_err(storage)?)
    }

    /// For a server without a store path: handles live as long as the process.
    pub(crate) fn volatile() -> Result<Self, ContinuationError> {
        Self::from_connection(Connection::open_in_memory().map_err(storage)?)
    }

    fn from_connection(connection: Connection) -> Result<Self, ContinuationError> {
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(storage)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
                 CREATE TABLE IF NOT EXISTS open_continuations (
                    id TEXT PRIMARY KEY, tool TEXT NOT NULL, arguments TEXT NOT NULL,
                    fingerprint TEXT NOT NULL UNIQUE, expires_at INTEGER NOT NULL);",
            )
            .map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

/// The stored arguments and the fingerprint that deduplicates a retried page.
fn encoded(call: &ReadContinuation) -> Result<(String, String), ContinuationError> {
    let arguments = serde_json::to_string(&call.arguments).map_err(storage)?;
    if arguments.len() > MAX_CALL_BYTES {
        return Err(storage("continuation exceeds the retained-call allowance"));
    }
    let mut digest = Sha256::new();
    digest.update(call.tool.as_bytes());
    digest.update([0]);
    digest.update(arguments.as_bytes());
    Ok((arguments, format!("{:x}", digest.finalize())))
}

impl ContinuationStore for SqliteContinuationStore {
    fn save(&self, call: &ReadContinuation) -> Result<ReadContinuationId, ContinuationError> {
        let (arguments, fingerprint) = encoded(call)?;
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        tx.execute(
            "DELETE FROM open_continuations WHERE expires_at <= unixepoch()",
            [],
        )
        .map_err(storage)?;
        if let Some(id) = tx
            .query_row(
                "SELECT id FROM open_continuations WHERE fingerprint=?1",
                [fingerprint.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage)?
        {
            tx.commit().map_err(storage)?;
            return ReadContinuationId::parse(&id);
        }
        let id = ReadContinuationId::random()?;
        tx.execute(
            "INSERT INTO open_continuations(id,tool,arguments,fingerprint,expires_at)
             VALUES(?1,?2,?3,?4,unixepoch()+?5)",
            params![
                id.as_str(),
                call.tool,
                arguments,
                fingerprint,
                RETENTION_SECONDS
            ],
        )
        .map_err(storage)?;
        // Bounded transport state. Evidence pages themselves are never cached.
        tx.execute(
            "DELETE FROM open_continuations WHERE rowid IN
             (SELECT rowid FROM open_continuations ORDER BY rowid DESC LIMIT -1 OFFSET ?1)",
            [MAX_CALLS],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(id)
    }

    fn load(&self, id: &ReadContinuationId) -> Result<Option<ReadContinuation>, ContinuationError> {
        let connection = self.connection.lock().map_err(storage)?;
        let row = connection
            .query_row(
                "SELECT tool,arguments FROM open_continuations
                 WHERE id=?1 AND expires_at > unixepoch()",
                [id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(storage)?;
        row.map(|(tool, arguments)| {
            ReadContinuation::new(&tool, serde_json::from_str(&arguments).map_err(storage)?)
        })
        .transpose()
    }
}

#[cfg(test)]
#[path = "sqlite_continuation_store_tests.rs"]
mod tests;
