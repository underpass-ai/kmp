use std::path::{Path, PathBuf};
use std::time::Duration;

use kmp_domain::PortError;
use kmp_observability::QualityTelemetryObservation;
use rusqlite::{Connection, Transaction, config::DbConfig, params};

use super::QualityTelemetryRetention;
use crate::adapter::serdes::encode;

const BUSY_TIMEOUT: Duration = Duration::from_secs(10);

pub fn quality_telemetry_path(data_dir: &Path) -> PathBuf {
    data_dir.join("telemetry").join("quality.sqlite3")
}

pub(super) fn open_quality_connection(data_dir: &Path) -> Result<Connection, PortError> {
    let path = quality_telemetry_path(data_dir);
    let parent = path.parent().expect("quality telemetry path has a parent");
    std::fs::create_dir_all(parent).map_err(|error| {
        PortError::Unavailable(format!(
            "quality telemetry could not create `{}`: {error}",
            parent.display()
        ))
    })?;
    let connection = Connection::open(&path).map_err(|error| {
        PortError::Unavailable(format!(
            "quality telemetry could not open `{}`: {error}",
            path.display()
        ))
    })?;
    harden_connection(&connection, &path)?;
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| sqlite_error(&path, "configure busy timeout", error))?;
    enter_wal(&connection, &path)?;
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|error| sqlite_error(&path, "configure durability", error))?;
    initialize_schema(&connection, &path)?;
    Ok(connection)
}

pub(super) fn insert_observation(
    transaction: &Transaction<'_>,
    observation: &QualityTelemetryObservation,
) -> Result<(), PortError> {
    let observed_at = i64::try_from(observation.observed_at_millis()).map_err(|_| {
        PortError::InvalidState("quality observation timestamp exceeds SQLite range".to_string())
    })?;
    let payload = encode("quality observation", observation)?;
    transaction
        .execute(
            "INSERT INTO quality_observations (observed_at_millis, payload) VALUES (?1, ?2)",
            params![observed_at, payload],
        )
        .map_err(|error| {
            PortError::Unavailable(format!(
                "quality telemetry could not persist an observation: {error}"
            ))
        })?;
    Ok(())
}

pub(super) fn enforce_retention(
    transaction: &Transaction<'_>,
    retention: QualityTelemetryRetention,
) -> Result<(), PortError> {
    let total: i64 = transaction
        .query_row("SELECT COUNT(*) FROM quality_observations", [], |row| {
            row.get(0)
        })
        .map_err(|error| {
            PortError::Unavailable(format!("quality telemetry count failed: {error}"))
        })?;
    let total = u64::try_from(total)
        .map_err(|_| PortError::InvalidState("quality telemetry count is negative".to_string()))?;
    let excess = retention.excess(total);
    if excess > 0 {
        transaction
            .execute(
                "DELETE FROM quality_observations WHERE id IN (\
                 SELECT id FROM quality_observations \
                 ORDER BY observed_at_millis ASC, id ASC LIMIT ?1)",
                params![i64::try_from(excess).unwrap_or(i64::MAX)],
            )
            .map_err(|error| {
                PortError::Unavailable(format!(
                    "quality telemetry retention cleanup failed: {error}"
                ))
            })?;
    }
    Ok(())
}

fn initialize_schema(connection: &Connection, path: &Path) -> Result<(), PortError> {
    let integrity: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|error| sqlite_error(path, "run quick_check", error))?;
    if integrity != "ok" {
        return Err(PortError::InvalidState(format!(
            "quality telemetry SQLite journal `{}` failed quick_check: {integrity}",
            path.display()
        )));
    }
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS quality_observations (\
               id INTEGER PRIMARY KEY AUTOINCREMENT,\
               observed_at_millis INTEGER NOT NULL,\
               payload BLOB NOT NULL\
             );\
             CREATE INDEX IF NOT EXISTS quality_observations_by_time \
               ON quality_observations (observed_at_millis, id);\
             CREATE TABLE IF NOT EXISTS quality_metadata (\
               key TEXT PRIMARY KEY,\
               value TEXT NOT NULL\
             ) WITHOUT ROWID;",
        )
        .map_err(|error| sqlite_error(path, "initialize schema", error))
}

fn harden_connection(connection: &Connection, path: &Path) -> Result<(), PortError> {
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
            .map_err(|error| sqlite_error(path, name, error))?;
    }
    connection
        .execute_batch("PRAGMA cell_size_check=ON; PRAGMA mmap_size=0;")
        .map_err(|error| sqlite_error(path, "configure safe page access", error))
}

fn enter_wal(connection: &Connection, path: &Path) -> Result<(), PortError> {
    let deadline = std::time::Instant::now() + BUSY_TIMEOUT;
    let mut backoff = Duration::from_millis(2);
    loop {
        match connection.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(error) if is_busy(&error) && std::time::Instant::now() < deadline => {
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_millis(64));
            }
            Err(error) => return Err(sqlite_error(path, "enter WAL mode", error)),
        }
    }
}

fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
    )
}

fn sqlite_error(path: &Path, action: &str, error: rusqlite::Error) -> PortError {
    PortError::Unavailable(format!(
        "quality telemetry could not {action} at `{}`: {error}",
        path.display()
    ))
}
