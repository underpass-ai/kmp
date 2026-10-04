use std::path::Path;
use std::time::Duration;

use kmp_embedded::{StorageEngine, store_file_path_for};
use rusqlite::{Connection, OpenFlags};

use crate::lifecycle::domain::about_event_count::AboutEventCount;
use crate::lifecycle::domain::store_contents::StoreContents;
use crate::lifecycle::ports::store_contents_reader::StoreContentsReader;

/// How long a read waits behind a writer's checkpoint before saying the store
/// is busy. Short: an inventory that hangs on one store tells nobody anything.
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);

/// Every about in the log and its event count, decoded from the stored event
/// JSON. `CAST` first: the value column is a BLOB of JSON text, and SQLite
/// reads a BLOB handed to `json_extract` as its binary JSONB encoding.
const ABOUT_COUNTS: &str = "SELECT json_extract(CAST(v AS TEXT), '$.root_node_id'), COUNT(*) \
     FROM event_log GROUP BY 1";

/// When the newest event was written, in the format `last opened` uses.
const LAST_WRITE: &str = "SELECT strftime('%Y-%m-%d %H:%M:%S', \
     json_extract(CAST(v AS TEXT), '$.occurred_at') / 1000, 'unixepoch') \
     FROM event_log ORDER BY k DESC LIMIT 1";

/// Reads a SQLite store's event log through a read-only connection.
///
/// Not through the engine: opening a store for use creates tables, switches
/// the journal mode and runs an integrity check, and the session lease
/// belongs to whoever is serving it. This takes no lease and writes no table.
///
/// A store at rest has no `-wal` file, and a plain read-only connection to a
/// WAL database would create `-wal` and `-shm` beside it and leave them there
/// — files in someone's memory that a look put there. So a store at rest is
/// read as immutable, which opens nothing but the database file. A store with
/// a `-wal` has a live or interrupted writer whose newest events are only in
/// that file; it is read through WAL like any other reader, which creates
/// nothing because the side files already exist.
pub struct SqliteStoreContentsReader;

impl StoreContentsReader for SqliteStoreContentsReader {
    fn contents(&self, store: &Path) -> StoreContents {
        let file = store_file_path_for(store, StorageEngine::Sqlite);
        if !file.is_file() {
            // Stamped but never written: an empty memory, not an unreadable one.
            return StoreContents::read(Vec::new(), None);
        }
        match read(&file) {
            Ok(contents) => contents,
            Err(error) => {
                StoreContents::unreadable(format!("contents not readable by this engine: {error}"))
            }
        }
    }
}

fn read(file: &Path) -> Result<StoreContents, rusqlite::Error> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let mut wal = file.as_os_str().to_owned();
    wal.push("-wal");
    let connection = if Path::new(&wal).exists() {
        Connection::open_with_flags(file, flags)?
    } else {
        Connection::open_with_flags(immutable_uri(file), flags | OpenFlags::SQLITE_OPEN_URI)?
    };
    connection.busy_timeout(BUSY_TIMEOUT)?;
    let mut statement = connection.prepare(ABOUT_COUNTS)?;
    let abouts = statement
        .query_map([], |row| {
            let about: Option<String> = row.get(0)?;
            let events: i64 = row.get(1)?;
            Ok(AboutEventCount::new(
                about.unwrap_or_default(),
                u64::try_from(events).unwrap_or_default(),
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let last_write = connection
        .query_row(LAST_WRITE, [], |row| row.get::<_, Option<String>>(0))
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(StoreContents::read(abouts, last_write))
}

/// `file:<path>?immutable=1`, with every byte a URI could misread escaped.
fn immutable_uri(file: &Path) -> String {
    let mut uri = String::from("file:");
    for byte in file.as_os_str().as_encoded_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                uri.push(char::from(*byte));
            }
            other => uri.push_str(&format!("%{other:02X}")),
        }
    }
    uri.push_str("?immutable=1");
    uri
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;

    /// A store laid out the way the engine lays one out: WAL journal,
    /// `event_log(k, v)` with JSON event bytes, closed cleanly.
    fn store_with_events(base: &Path, events: &[(&str, u64)]) -> PathBuf {
        let store = base.join("repo/.kernel");
        std::fs::create_dir_all(store.join("store")).expect("store dir");
        std::fs::write(store.join("FORMAT_VERSION"), "4").expect("stamp");
        let connection =
            Connection::open(store_file_path_for(&store, StorageEngine::Sqlite)).expect("sqlite");
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .expect("wal");
        connection
            .execute_batch("CREATE TABLE event_log (k INTEGER PRIMARY KEY, v BLOB NOT NULL)")
            .expect("table");
        for (sequence, (about, occurred_at)) in events.iter().enumerate() {
            let value = serde_json::to_vec(&serde_json::json!({
                "root_node_id": about,
                "role": "memory",
                "revision": 1,
                "occurred_at": occurred_at,
            }))
            .expect("event");
            connection
                .execute(
                    "INSERT INTO event_log (k, v) VALUES (?1, ?2)",
                    rusqlite::params![sequence as i64 + 1, value],
                )
                .expect("event row");
        }
        drop(connection);
        store
    }

    fn listing(store: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        std::fs::read_dir(store.join("store"))
            .expect("store dir")
            .flatten()
            .map(|entry| {
                (
                    entry.path(),
                    std::fs::read(entry.path()).expect("store file"),
                )
            })
            .collect()
    }

    #[test]
    fn a_supported_store_reads_its_abouts_counts_and_last_write() {
        let base = tempfile::tempdir().expect("temp");
        let store = store_with_events(
            base.path(),
            &[
                ("project:a", 1_787_604_261_000),
                ("project:b", 1_787_604_262_000),
                ("project:a", 1_787_604_263_000),
            ],
        );

        let contents = SqliteStoreContentsReader.contents(&store);
        assert_eq!(
            contents,
            StoreContents::Read {
                abouts: vec![
                    AboutEventCount::new("project:a", 2),
                    AboutEventCount::new("project:b", 1),
                ],
                last_write: Some("2026-08-24 20:44:23".to_string()),
            }
        );
    }

    #[test]
    fn reading_leaves_every_store_file_byte_for_byte_as_it_was() {
        let base = tempfile::tempdir().expect("temp");
        let store = store_with_events(base.path(), &[("project:a", 1_000)]);
        let before = listing(&store);

        let _ = SqliteStoreContentsReader.contents(&store);

        assert_eq!(
            listing(&store),
            before,
            "an inventory that leaves a file behind or changes a byte has written to memory"
        );
    }

    #[test]
    fn a_store_with_a_live_writer_is_read_through_its_wal_and_left_unlocked() {
        let base = tempfile::tempdir().expect("temp");
        let store = store_with_events(base.path(), &[("project:a", 1_000)]);
        // A writer that keeps its connection open: its newest event lives in
        // the WAL until a checkpoint, and the inventory must still see it.
        let writer =
            Connection::open(store_file_path_for(&store, StorageEngine::Sqlite)).expect("writer");
        writer
            .pragma_update(None, "wal_autocheckpoint", 0)
            .expect("no checkpoint");
        writer
            .execute(
                "INSERT INTO event_log (k, v) VALUES (2, ?1)",
                [serde_json::to_vec(&serde_json::json!({
                    "root_node_id": "project:b",
                    "occurred_at": 2_000,
                }))
                .expect("event")],
            )
            .expect("live event");

        let contents = SqliteStoreContentsReader.contents(&store);
        assert_eq!(contents.total_events(), 2, "{contents:?}");

        writer
            .execute_batch("BEGIN IMMEDIATE; COMMIT;")
            .expect("the reader left no lock behind");
    }

    #[test]
    fn a_path_with_uri_characters_is_still_read_as_itself() {
        let base = tempfile::tempdir().expect("temp");
        let odd = base.path().join("a dir?#%");
        let store = store_with_events(&odd, &[("project:a", 1_000)]);
        assert_eq!(SqliteStoreContentsReader.contents(&store).total_events(), 1);
    }

    #[test]
    fn a_store_whose_file_is_not_a_kmp_log_is_unreadable_rather_than_an_error() {
        let base = tempfile::tempdir().expect("temp");
        let store = base.path().join("old/.kernel");
        std::fs::create_dir_all(store.join("store")).expect("store dir");
        std::fs::write(store.join("FORMAT_VERSION"), "4").expect("stamp");
        std::fs::write(
            store_file_path_for(&store, StorageEngine::Sqlite),
            b"not a database at all",
        )
        .expect("bytes");

        let contents = SqliteStoreContentsReader.contents(&store);
        assert!(
            matches!(&contents, StoreContents::Unreadable { reason }
                if reason.starts_with("contents not readable by this engine")),
            "{contents:?}"
        );
    }

    #[test]
    fn a_stamped_store_with_no_file_yet_is_an_empty_memory() {
        let base = tempfile::tempdir().expect("temp");
        let store = base.path().join("fresh");
        std::fs::create_dir_all(&store).expect("store");
        assert_eq!(
            SqliteStoreContentsReader.contents(&store),
            StoreContents::read(Vec::new(), None)
        );
    }
}
