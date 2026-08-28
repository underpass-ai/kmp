use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use kmp_domain::PortError;
use kmp_observability::QualityTelemetryObservation;
use rusqlite::{Connection, TransactionBehavior};

use super::quality_telemetry_retention::QualityTelemetryRetention;
use super::sqlite_quality_telemetry_reader::SqliteQualityTelemetryReader;
use super::storage::{enforce_retention, insert_observation, open_quality_connection};

const DEFAULT_DURABLE_EVERY_BATCHES: u64 = 16;

/// Multi-process SQLite writer for the bounded local quality journal.
#[derive(Debug)]
pub struct SqliteQualityTelemetryWriter {
    connection: Arc<Mutex<Connection>>,
    retention: QualityTelemetryRetention,
    batch_number: AtomicU64,
    durable_every_batches: u64,
    write_failures: AtomicU64,
}

impl SqliteQualityTelemetryWriter {
    pub fn reader(&self) -> SqliteQualityTelemetryReader {
        SqliteQualityTelemetryReader::from_connection(Arc::clone(&self.connection))
    }

    pub fn open(data_dir: &Path, retention: QualityTelemetryRetention) -> Result<Self, PortError> {
        Self::open_with_durable_interval(data_dir, retention, DEFAULT_DURABLE_EVERY_BATCHES)
    }

    pub fn open_with_durable_interval(
        data_dir: &Path,
        retention: QualityTelemetryRetention,
        durable_every_batches: u64,
    ) -> Result<Self, PortError> {
        if durable_every_batches == 0 {
            return Err(PortError::Unavailable(
                "quality telemetry durable interval must be greater than zero".to_string(),
            ));
        }
        let connection = open_quality_connection(data_dir)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            retention,
            batch_number: AtomicU64::new(0),
            durable_every_batches,
            write_failures: AtomicU64::new(0),
        })
    }

    pub fn write_batch(
        &self,
        observations: &[QualityTelemetryObservation],
    ) -> Result<(), PortError> {
        if observations.is_empty() {
            return Ok(());
        }
        let result = self.write_batch_inner(observations);
        if result.is_err() {
            self.write_failures.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    pub fn flush_durable(&self) -> Result<(), PortError> {
        let result = self.flush_durable_inner();
        if result.is_err() {
            self.write_failures.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    pub fn write_failures(&self) -> u64 {
        self.write_failures.load(Ordering::Relaxed)
    }

    fn write_batch_inner(
        &self,
        observations: &[QualityTelemetryObservation],
    ) -> Result<(), PortError> {
        let current_batch = self.batch_number.fetch_add(1, Ordering::Relaxed) + 1;
        let synchronous = if current_batch.is_multiple_of(self.durable_every_batches) {
            "FULL"
        } else {
            "NORMAL"
        };
        let mut connection = self.connection.lock().map_err(|_| poisoned())?;
        connection
            .pragma_update(None, "synchronous", synchronous)
            .map_err(write_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(write_error)?;
        for observation in observations {
            insert_observation(&transaction, observation)?;
        }
        enforce_retention(&transaction, self.retention)?;
        transaction.commit().map_err(write_error)
    }

    fn flush_durable_inner(&self) -> Result<(), PortError> {
        let connection = self.connection.lock().map_err(|_| poisoned())?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(write_error)?;
        connection
            .execute_batch("BEGIN IMMEDIATE; COMMIT; PRAGMA wal_checkpoint(FULL);")
            .map_err(write_error)?;
        connection
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(write_error)
    }
}

fn poisoned() -> PortError {
    PortError::Unavailable("quality telemetry connection lock is poisoned".to_string())
}

fn write_error(error: rusqlite::Error) -> PortError {
    PortError::Unavailable(format!("quality telemetry write failed: {error}"))
}
