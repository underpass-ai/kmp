use std::path::Path;
use std::sync::Arc;

use kmp_domain::PortError;
use kmp_observability::QualityTelemetryObservation;
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata};

use super::storage::{OBSERVATIONS, quality_telemetry_path};
use crate::adapter::serdes::decode;
use crate::adapter::store::{range_error, table_error};

/// Read-only query adapter for the local quality journal.
#[derive(Debug, Clone)]
pub struct RedbQualityTelemetryReader {
    database: Arc<Database>,
}

impl RedbQualityTelemetryReader {
    pub fn open(data_dir: &Path) -> Result<Self, PortError> {
        let path = quality_telemetry_path(data_dir);
        let database = Database::open(&path).map_err(|error| {
            PortError::Unavailable(format!(
                "quality telemetry could not open `{}` for reading: {error}",
                path.display()
            ))
        })?;
        Ok(Self {
            database: Arc::new(database),
        })
    }

    pub fn count(&self) -> Result<u64, PortError> {
        let tx = self.begin_read()?;
        let table = tx.open_table(OBSERVATIONS).map_err(table_error)?;
        table.len().map_err(|error| {
            PortError::Unavailable(format!("quality telemetry count failed: {error}"))
        })
    }

    pub fn query_since(
        &self,
        since_millis: u64,
        rpc: Option<&str>,
        limit: usize,
    ) -> Result<Vec<QualityTelemetryObservation>, PortError> {
        self.query_between(since_millis, u64::MAX, rpc, limit)
    }

    pub fn query_between(
        &self,
        since_millis: u64,
        until_millis: u64,
        rpc: Option<&str>,
        limit: usize,
    ) -> Result<Vec<QualityTelemetryObservation>, PortError> {
        if limit == 0 || until_millis < since_millis {
            return Ok(Vec::new());
        }
        let tx = self.begin_read()?;
        let table = tx.open_table(OBSERVATIONS).map_err(table_error)?;
        let mut observations = Vec::new();
        for row in table
            .range((since_millis, 0u64)..=(until_millis, u64::MAX))
            .map_err(range_error)?
        {
            let (_, value) = row.map_err(range_error)?;
            let observation: QualityTelemetryObservation =
                decode("quality observation", value.value())?;
            if rpc.is_some_and(|wanted| wanted != observation.rpc()) {
                continue;
            }
            observations.push(observation);
            if observations.len() >= limit {
                break;
            }
        }
        Ok(observations)
    }

    pub fn latest(&self, limit: usize) -> Result<Vec<QualityTelemetryObservation>, PortError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let tx = self.begin_read()?;
        let table = tx.open_table(OBSERVATIONS).map_err(table_error)?;
        let mut observations = Vec::new();
        for row in table.iter().map_err(range_error)?.rev().take(limit) {
            let (_, value) = row.map_err(range_error)?;
            observations.push(decode("quality observation", value.value())?);
        }
        observations.reverse();
        Ok(observations)
    }

    fn begin_read(&self) -> Result<redb::ReadTransaction, PortError> {
        self.database.begin_read().map_err(|error| {
            PortError::Unavailable(format!(
                "quality telemetry read transaction failed: {error}"
            ))
        })
    }
}
