use std::future::Future;
use std::pin::Pin;

use kmp_domain::PortError;
use serde::{Deserialize, Serialize};

/// One renderer request for telemetry aligned to the loom's time window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityQuery {
    pub about: Option<String>,
    pub from_millis: u64,
    pub to_millis: u64,
    pub series: Vec<String>,
    pub max_points: usize,
}

/// Application seam for persisted local observations and remote telemetry
/// backends. Renderers compose these values; they do not reinterpret them.
pub trait ObservabilityQueryPort: Send + Sync {
    /// Exact series names this reader can resolve without inventing aliases.
    /// Readers backed by a dynamic remote catalog may leave this empty until
    /// they can expose that capability explicitly.
    fn available_series(&self) -> Vec<String> {
        Vec::new()
    }

    fn query<'a>(
        &'a self,
        query: ObservabilityQuery,
    ) -> Pin<Box<dyn Future<Output = Result<ObservabilityProjection, PortError>> + Send + 'a>>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservabilityProjection {
    pub contract: String,
    pub from_millis: u64,
    pub to_millis: u64,
    pub series: Vec<ObservabilitySeries>,
    pub exemplars: Vec<ObservabilityExemplar>,
    pub missing: Vec<String>,
    pub truncated: bool,
}

impl ObservabilityProjection {
    pub fn empty(query: &ObservabilityQuery) -> Self {
        Self {
            contract: "kmp.observability.projection.v1".to_string(),
            from_millis: query.from_millis,
            to_millis: query.to_millis,
            series: Vec::new(),
            exemplars: Vec::new(),
            missing: query.series.clone(),
            truncated: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservabilitySeries {
    pub name: String,
    pub unit: String,
    pub scope: String,
    pub points: Vec<ObservabilityMetricPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservabilityMetricPoint {
    pub at_millis: u64,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exemplar_id: Option<String>,
}

/// A selectable observation resolving back to an operation and memory root.
/// It deliberately does not assert that the operation caused nearby memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservabilityExemplar {
    pub id: String,
    pub at_millis: u64,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    pub attributes: std::collections::BTreeMap<String, String>,
}
