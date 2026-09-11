use std::sync::{Arc, Mutex};

use kmp_domain::PortError;
use rusqlite::Connection;

use super::sqlite_snapshot_read::SqliteSnapshotRead;
use super::{Engine, ReadTx, WriteTx};

/// One pinned SQLite read transaction shared by all ports of an operation.
/// Its connection returns to the ordinary pool only after its last owner drops.
#[derive(Debug)]
pub(super) struct SqliteSnapshot {
    connection: Mutex<Option<Connection>>,
    pool: Arc<Mutex<Vec<Connection>>>,
}

impl SqliteSnapshot {
    pub(super) fn new(
        connection: Connection,
        pool: Arc<Mutex<Vec<Connection>>>,
    ) -> Result<Self, PortError> {
        let snapshot = Self {
            connection: Mutex::new(Some(connection)),
            pool,
        };
        {
            let guard = snapshot.connection.lock().map_err(|_| poisoned())?;
            let connection = guard.as_ref().expect("snapshot connection");
            connection
                .execute_batch("PRAGMA query_only=ON; BEGIN;")
                .map_err(|e| PortError::Unavailable(format!("snapshot begin failed: {e}")))?;
            // BEGIN is deferred. Read the SQLite catalogue to pin the database
            // before returning the ports, without reading a memory node or
            // spending a graph traversal budget just to open its snapshot.
            connection
                .query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema)", [], |row| {
                    row.get::<_, bool>(0)
                })
                .map_err(|e| PortError::Unavailable(format!("snapshot pin failed: {e}")))?;
        }
        Ok(snapshot)
    }
}

impl Engine for SqliteSnapshot {
    fn begin_read(&self) -> Result<Box<dyn ReadTx + '_>, PortError> {
        Ok(Box::new(SqliteSnapshotRead(
            self.connection.lock().map_err(|_| poisoned())?,
        )))
    }

    fn begin_write(&self) -> Result<Box<dyn WriteTx + '_>, PortError> {
        Err(PortError::InvalidState(
            "a read snapshot cannot write".into(),
        ))
    }
}

impl Drop for SqliteSnapshot {
    fn drop(&mut self) {
        let Ok(slot) = self.connection.get_mut() else {
            return;
        };
        let Some(connection) = slot.take() else {
            return;
        };
        // Never pool a connection if cleanup failed. Closing it releases the
        // transaction even after cancellation or an error during the read.
        if !connection.is_autocommit() && connection.execute_batch("ROLLBACK").is_err() {
            return;
        }
        if connection.execute_batch("PRAGMA query_only=OFF").is_err() {
            return;
        }
        if let Ok(mut pool) = self.pool.lock() {
            pool.push(connection);
        }
    }
}

fn poisoned() -> PortError {
    PortError::Unavailable("read snapshot connection lock poisoned".into())
}
