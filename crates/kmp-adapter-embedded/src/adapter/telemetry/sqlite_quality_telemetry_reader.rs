use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use kmp_application::{
    ObservabilityExemplar, ObservabilityMetricPoint, ObservabilityProjection, ObservabilityQuery,
    ObservabilityQueryPort, ObservabilitySeries,
};
use kmp_domain::PortError;
use kmp_observability::QualityTelemetryObservation;
use rusqlite::{Connection, params};

use super::storage::open_quality_connection;
use crate::adapter::serdes::decode;

/// Read-only query adapter for the shareable local quality journal.
#[derive(Debug, Clone)]
pub struct SqliteQualityTelemetryReader {
    connection: Arc<Mutex<Connection>>,
}

impl ObservabilityQueryPort for SqliteQualityTelemetryReader {
    fn available_series(&self) -> Vec<String> {
        SUPPORTED_SERIES
            .iter()
            .map(|(name, _, _)| (*name).to_string())
            .collect()
    }

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
            let requested = if query.series.is_empty() {
                SUPPORTED_SERIES
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
                let Some((name, unit, scope)) = SUPPORTED_SERIES
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

const SUPPORTED_SERIES: &[(&str, &str, &str)] = &[
    ("raw_equivalent_tokens", "tokens", "rendered_bundle"),
    ("compression_ratio", "ratio", "rendered_bundle"),
    ("causal_density", "ratio", "rendered_bundle"),
    ("noise_ratio", "ratio", "rendered_bundle"),
    ("detail_coverage", "ratio", "rendered_bundle"),
];

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

impl SqliteQualityTelemetryReader {
    pub(super) fn from_connection(connection: Arc<Mutex<Connection>>) -> Self {
        Self { connection }
    }

    pub fn open(data_dir: &Path) -> Result<Self, PortError> {
        let connection = open_quality_connection(data_dir)?;
        Ok(Self::from_connection(Arc::new(Mutex::new(connection))))
    }

    pub fn count(&self) -> Result<u64, PortError> {
        let count: i64 = self
            .connection()?
            .query_row("SELECT COUNT(*) FROM quality_observations", [], |row| {
                row.get(0)
            })
            .map_err(|error| {
                PortError::Unavailable(format!("quality telemetry count failed: {error}"))
            })?;
        u64::try_from(count)
            .map_err(|_| PortError::InvalidState("quality telemetry count is negative".to_string()))
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
        let from = i64::try_from(since_millis).unwrap_or(i64::MAX);
        let to = i64::try_from(until_millis).unwrap_or(i64::MAX);
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT payload FROM quality_observations \
                 WHERE observed_at_millis BETWEEN ?1 AND ?2 \
                 ORDER BY observed_at_millis ASC, id ASC",
            )
            .map_err(query_error)?;
        let rows = statement
            .query_map(params![from, to], |row| row.get::<_, Vec<u8>>(0))
            .map_err(query_error)?;
        let mut observations = Vec::new();
        for row in rows {
            let payload = row.map_err(query_error)?;
            let observation: QualityTelemetryObservation = decode("quality observation", &payload)?;
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
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT payload FROM quality_observations \
                 ORDER BY observed_at_millis DESC, id DESC LIMIT ?1",
            )
            .map_err(query_error)?;
        let rows = statement
            .query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .map_err(query_error)?;
        let mut observations = rows
            .map(|row| {
                let payload = row.map_err(query_error)?;
                decode("quality observation", &payload)
            })
            .collect::<Result<Vec<_>, _>>()?;
        observations.reverse();
        Ok(observations)
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, PortError> {
        self.connection.lock().map_err(|_| {
            PortError::Unavailable("quality telemetry connection lock is poisoned".to_string())
        })
    }
}

fn query_error(error: rusqlite::Error) -> PortError {
    PortError::Unavailable(format!("quality telemetry query failed: {error}"))
}
