use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use redb::{Database, Durability, ReadableDatabase, ReadableTable, ReadableTableMetadata};
use rehydration_domain::PortError;
use rehydration_observability::QualityTelemetryObservation;

use super::quality_telemetry_retention::QualityTelemetryRetention;
use super::storage::{OBSERVATIONS, quality_telemetry_path};
use crate::adapter::serdes::encode;
use crate::adapter::store::{commit_error, range_error, storage_error, table_error};

const DEFAULT_DURABLE_EVERY_BATCHES: u64 = 16;

/// Relaxed-durability writer for `telemetry/quality.redb`.
#[derive(Debug)]
pub struct RedbQualityTelemetryWriter {
    database: Arc<Database>,
    retention: QualityTelemetryRetention,
    next_sequence: AtomicU64,
    batch_number: AtomicU64,
    durable_every_batches: u64,
    write_failures: AtomicU64,
}

impl RedbQualityTelemetryWriter {
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
        let path = quality_telemetry_path(data_dir);
        let parent = path.parent().expect("quality telemetry path has a parent");
        std::fs::create_dir_all(parent).map_err(|error| {
            PortError::Unavailable(format!(
                "quality telemetry could not create `{}`: {error}",
                parent.display()
            ))
        })?;
        let database = Arc::new(Database::create(&path).map_err(|error| {
            PortError::Unavailable(format!(
                "quality telemetry could not open `{}`: {error}",
                path.display()
            ))
        })?);
        initialize_table(&database)?;
        let next_sequence = load_highest_sequence(&database)?;
        Ok(Self {
            database,
            retention,
            next_sequence: AtomicU64::new(next_sequence),
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
        let durability = if current_batch.is_multiple_of(self.durable_every_batches) {
            Durability::Immediate
        } else {
            Durability::None
        };
        let mut tx = self.database.begin_write().map_err(|error| {
            PortError::Unavailable(format!(
                "quality telemetry write transaction failed: {error}"
            ))
        })?;
        tx.set_durability(durability).map_err(|error| {
            PortError::Unavailable(format!(
                "quality telemetry durability configuration failed: {error}"
            ))
        })?;
        {
            let mut table = tx.open_table(OBSERVATIONS).map_err(table_error)?;
            for observation in observations {
                let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed) + 1;
                let bytes = encode("quality observation", observation)?;
                table
                    .insert(
                        (observation.observed_at_millis(), sequence),
                        bytes.as_slice(),
                    )
                    .map_err(storage_error)?;
            }
            let excess = self.retention.excess(table.len().map_err(storage_error)?);
            for _ in 0..excess {
                table.pop_first().map_err(storage_error)?;
            }
        }
        tx.commit().map_err(commit_error)
    }

    fn flush_durable_inner(&self) -> Result<(), PortError> {
        let mut tx = self.database.begin_write().map_err(|error| {
            PortError::Unavailable(format!(
                "quality telemetry durable flush failed to start: {error}"
            ))
        })?;
        tx.set_durability(Durability::Immediate).map_err(|error| {
            PortError::Unavailable(format!(
                "quality telemetry durable flush configuration failed: {error}"
            ))
        })?;
        tx.commit().map_err(commit_error)
    }
}

fn initialize_table(database: &Database) -> Result<(), PortError> {
    let tx = database.begin_write().map_err(|error| {
        PortError::Unavailable(format!("quality telemetry initialization failed: {error}"))
    })?;
    tx.open_table(OBSERVATIONS).map_err(table_error)?;
    tx.commit().map_err(commit_error)
}

fn load_highest_sequence(database: &Database) -> Result<u64, PortError> {
    let tx = database.begin_read().map_err(|error| {
        PortError::Unavailable(format!("quality telemetry sequence read failed: {error}"))
    })?;
    let table = tx.open_table(OBSERVATIONS).map_err(table_error)?;
    let mut highest = 0u64;
    for row in table.iter().map_err(range_error)? {
        let (key, _) = row.map_err(range_error)?;
        highest = highest.max(key.value().1);
    }
    Ok(highest)
}
