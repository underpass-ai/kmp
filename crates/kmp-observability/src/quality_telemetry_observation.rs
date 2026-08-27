use std::time::{SystemTime, UNIX_EPOCH};

use kmp_domain::{BundleQualityMetrics, QualityObservationContext};
use serde::{Deserialize, Serialize};

/// One typed, timestamped quality observation persisted by local telemetry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityTelemetryObservation {
    observed_at_millis: u64,
    rpc: String,
    root_node_id: String,
    role: String,
    #[serde(default)]
    revision: Option<u64>,
    raw_equivalent_tokens: u32,
    compression_ratio: f64,
    causal_density: f64,
    noise_ratio: f64,
    detail_coverage: f64,
}

impl QualityTelemetryObservation {
    pub fn capture(metrics: &BundleQualityMetrics, context: &QualityObservationContext) -> Self {
        Self {
            observed_at_millis: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
                .unwrap_or(0),
            rpc: context.rpc.clone(),
            root_node_id: context.root_node_id.clone(),
            role: context.role.clone(),
            revision: context.revision,
            raw_equivalent_tokens: metrics.raw_equivalent_tokens(),
            compression_ratio: metrics.compression_ratio(),
            causal_density: metrics.causal_density(),
            noise_ratio: metrics.noise_ratio(),
            detail_coverage: metrics.detail_coverage(),
        }
    }

    pub fn observed_at_millis(&self) -> u64 {
        self.observed_at_millis
    }

    pub fn rpc(&self) -> &str {
        &self.rpc
    }

    pub fn root_node_id(&self) -> &str {
        &self.root_node_id
    }

    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn revision(&self) -> Option<u64> {
        self.revision
    }

    pub fn raw_equivalent_tokens(&self) -> u32 {
        self.raw_equivalent_tokens
    }

    pub fn compression_ratio(&self) -> f64 {
        self.compression_ratio
    }

    pub fn causal_density(&self) -> f64 {
        self.causal_density
    }

    pub fn noise_ratio(&self) -> f64 {
        self.noise_ratio
    }

    pub fn detail_coverage(&self) -> f64 {
        self.detail_coverage
    }
}
