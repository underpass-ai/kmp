use crate::value_objects::BundleQualityMetrics;

/// Context metadata for a quality metrics observation.
#[derive(Debug, Clone)]
pub struct QualityObservationContext {
    /// The RPC that produced this observation (e.g. "GetContext", "GetContextPath").
    pub rpc: String,
    /// Root node ID of the queried graph.
    pub root_node_id: String,
    /// Role for which the bundle was rendered.
    pub role: String,
}

/// Port for observing quality metrics produced by the rendering pipeline.
///
/// Adapters implement this to push metrics to different backends:
/// OTel histograms, Loki structured logs, Prometheus push gateway, etc.
///
/// The kernel calls [`observe`] after every successful render. Adapters
/// must be non-blocking — an adapter that blocks on I/O should buffer
/// internally and flush asynchronously.
pub trait QualityMetricsObserver: Send + Sync {
    /// Record a quality metrics observation.
    fn observe(&self, metrics: &BundleQualityMetrics, context: &QualityObservationContext);
}
