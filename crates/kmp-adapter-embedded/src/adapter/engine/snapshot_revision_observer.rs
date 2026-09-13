use std::sync::atomic::{AtomicU64, Ordering};

use kmp_domain::{GraphReadRevision, PortError};
use rusqlite::Connection;

static NEXT_OBSERVER: AtomicU64 = AtomicU64::new(1);

/// A connection used only to observe commits. SQLite's data_version is local
/// to a connection, so it must never be compared across pooled connections.
/// This connection never writes or holds a read transaction. It observes
/// commits made by all writers, including older binaries and other processes.
#[derive(Debug)]
pub(super) struct SnapshotRevisionObserver {
    connection: Connection,
    incarnation: u64,
}

impl SnapshotRevisionObserver {
    pub(super) fn new(connection: Connection) -> Result<Self, PortError> {
        connection
            .execute_batch("PRAGMA query_only=ON")
            .map_err(error)?;
        let incarnation = NEXT_OBSERVER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| PortError::Unavailable("snapshot observer identities exhausted".into()))?;
        Ok(Self {
            connection,
            incarnation,
        })
    }

    pub(super) fn observe(&self) -> Result<GraphReadRevision, PortError> {
        let version: i64 = self
            .connection
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .map_err(error)?;
        GraphReadRevision::new(format!("sqlite-observer:{}:{version}", self.incarnation))
            .map_err(|e| PortError::InvalidState(e.to_string()))
    }

    pub(super) fn pin<T>(
        &self,
        pin: impl FnOnce() -> Result<T, PortError>,
    ) -> Result<(T, Option<GraphReadRevision>), PortError> {
        let before = self.observe()?;
        let snapshot = pin()?;
        let revision = (before == self.observe()?).then_some(before);
        Ok((snapshot, revision))
    }
}

fn error(error: rusqlite::Error) -> PortError {
    PortError::Unavailable(format!("snapshot revision observation failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commit_during_acquisition_never_certifies_a_reusable_snapshot() {
        let dir = tempfile::tempdir().expect("fixture operation succeeds");
        let path = dir.path().join("revision.sqlite");
        let writer = Connection::open(&path).expect("fixture operation succeeds");
        writer
            .execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE value(n INTEGER)")
            .expect("fixture operation succeeds");
        let observer = SnapshotRevisionObserver::new(
            Connection::open(&path).expect("fixture operation succeeds"),
        )
        .expect("fixture operation succeeds");
        let (_, clean) = observer.pin(|| Ok(())).expect("fixture operation succeeds");
        assert!(clean.is_some());
        let (_, raced) = observer
            .pin(|| {
                writer
                    .execute("INSERT INTO value VALUES(1)", [])
                    .expect("fixture operation succeeds");
                Ok(())
            })
            .expect("fixture operation succeeds");
        assert!(raced.is_none());
        let (_, current) = observer.pin(|| Ok(())).expect("fixture operation succeeds");
        assert!(current.is_some());
        assert_ne!(current, clean);
    }
}
