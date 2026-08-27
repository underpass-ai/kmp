use std::path::Path;
use std::sync::Arc;

use kmp_application::{
    ObservabilityExemplar, ObservabilityMetricPoint, ObservabilityProjection, ObservabilityQuery,
    ObservabilityQueryPort, ObservabilitySeries,
};
use kmp_domain::PortError;
use kmp_observability::QualityTelemetryObservation;
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata};

use super::storage::{OBSERVATIONS, quality_telemetry_path};
use crate::adapter::engine::redb::{range_error, table_error};
use crate::adapter::serdes::decode;

/// Read-only query adapter for the local quality journal.
#[derive(Debug, Clone)]
pub struct RedbQualityTelemetryReader {
    database: Arc<Database>,
}

impl ObservabilityQueryPort for RedbQualityTelemetryReader {
    fn query<'a>(
        &'a self,
        query: ObservabilityQuery,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<ObservabilityProjection, PortError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            const SUPPORTED: &[(&str, &str, &str)] = &[
                ("raw_equivalent_tokens", "tokens", "rendered_bundle"),
                ("compression_ratio", "ratio", "rendered_bundle"),
                ("causal_density", "ratio", "rendered_bundle"),
                ("noise_ratio", "ratio", "rendered_bundle"),
                ("detail_coverage", "ratio", "rendered_bundle"),
            ];
            let requested = if query.series.is_empty() {
                SUPPORTED
                    .iter()
                    .map(|(name, _, _)| (*name).to_string())
                    .collect::<Vec<_>>()
            } else {
                query.series.clone()
            };
            if query.to_millis < query.from_millis || query.max_points == 0 {
                return Ok(ObservabilityProjection::empty(&query));
            }
            let mut observations = self.query_between_filtered(
                query.from_millis,
                query.to_millis,
                None,
                query.about.as_deref(),
                query.max_points.saturating_add(1),
            )?;
            let truncated = observations.len() > query.max_points;
            observations.truncate(query.max_points);
            let exemplars = observations
                .iter()
                .enumerate()
                .map(|(index, observation)| {
                    let id = format!("local-quality:{}:{index}", observation.observed_at_millis());
                    let mut attributes = std::collections::BTreeMap::new();
                    attributes.insert("role".to_string(), observation.role().to_string());
                    ObservabilityExemplar {
                        id,
                        at_millis: observation.observed_at_millis(),
                        operation: observation.rpc().to_string(),
                        about: (!observation.root_node_id().is_empty())
                            .then(|| observation.root_node_id().to_string()),
                        bundle_ref: (!observation.root_node_id().is_empty())
                            .then(|| observation.root_node_id().to_string()),
                        revision: observation.revision(),
                        attributes,
                    }
                })
                .collect::<Vec<_>>();
            let mut series = Vec::new();
            let mut missing = Vec::new();
            for requested_name in requested {
                let Some((name, unit, scope)) = SUPPORTED
                    .iter()
                    .find(|(name, _, _)| *name == requested_name)
                else {
                    missing.push(requested_name);
                    continue;
                };
                let points = observations
                    .iter()
                    .zip(&exemplars)
                    .map(|(observation, exemplar)| ObservabilityMetricPoint {
                        at_millis: observation.observed_at_millis(),
                        value: quality_metric(observation, name),
                        exemplar_id: Some(exemplar.id.clone()),
                    })
                    .collect();
                series.push(ObservabilitySeries {
                    name: (*name).to_string(),
                    unit: (*unit).to_string(),
                    scope: (*scope).to_string(),
                    points,
                });
            }
            Ok(ObservabilityProjection {
                contract: "kmp.observability.projection.v1".to_string(),
                from_millis: query.from_millis,
                to_millis: query.to_millis,
                series,
                exemplars,
                missing,
                truncated,
            })
        })
    }
}

fn quality_metric(observation: &QualityTelemetryObservation, name: &str) -> f64 {
    match name {
        "raw_equivalent_tokens" => f64::from(observation.raw_equivalent_tokens()),
        "compression_ratio" => observation.compression_ratio(),
        "causal_density" => observation.causal_density(),
        "noise_ratio" => observation.noise_ratio(),
        "detail_coverage" => observation.detail_coverage(),
        _ => 0.0,
    }
}

impl RedbQualityTelemetryReader {
    pub(super) fn from_database(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn open(data_dir: &Path) -> Result<Self, PortError> {
        let path = quality_telemetry_path(data_dir);
        let database = Database::open(&path).map_err(|error| {
            PortError::Unavailable(format!(
                "quality telemetry could not open `{}` for reading: {error}",
                path.display()
            ))
        })?;
        Ok(Self::from_database(Arc::new(database)))
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
        self.query_between_filtered(since_millis, until_millis, rpc, None, limit)
    }

    fn query_between_filtered(
        &self,
        since_millis: u64,
        until_millis: u64,
        rpc: Option<&str>,
        root_node_id: Option<&str>,
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
            if root_node_id.is_some_and(|wanted| wanted != observation.root_node_id()) {
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
