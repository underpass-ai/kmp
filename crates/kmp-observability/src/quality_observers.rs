//! Adapters for the [`QualityMetricsObserver`] domain port.
//!
//! - [`OTelQualityObserver`]: OpenTelemetry histograms (Prometheus/Grafana via OTLP)
//! - [`TracingQualityObserver`]: Structured tracing logs (Loki/Grafana via Promtail)
//! - [`CompositeQualityObserver`]: Fan-out to multiple observers

use std::sync::Arc;

use kmp_domain::{BundleQualityMetrics, QualityMetricsObserver, QualityObservationContext};
use opentelemetry::metrics::{Histogram, Meter};

// ── OTel adapter ────────────────────────────────────────────────────────

/// Emits quality metrics as OpenTelemetry histograms.
///
/// Requires `OTEL_EXPORTER_OTLP_ENDPOINT` to export; otherwise instruments
/// discard data silently (noop meter behavior).
pub struct OTelQualityObserver {
    raw_equivalent_tokens: Histogram<u64>,
    compression_ratio: Histogram<f64>,
    causal_density: Histogram<f64>,
    noise_ratio: Histogram<f64>,
    detail_coverage: Histogram<f64>,
}

impl OTelQualityObserver {
    pub fn new(meter: &Meter) -> Self {
        Self {
            raw_equivalent_tokens: meter
                .u64_histogram("rehydration.quality.raw_equivalent_tokens")
                .with_description("Flat text token count baseline")
                .build(),
            compression_ratio: meter
                .f64_histogram("rehydration.quality.compression_ratio")
                .with_description("Raw / rendered token ratio")
                .build(),
            causal_density: meter
                .f64_histogram("rehydration.quality.causal_density")
                .with_description("Fraction of explanatory relationships")
                .build(),
            noise_ratio: meter
                .f64_histogram("rehydration.quality.noise_ratio")
                .with_description("Fraction of noise/distractor nodes")
                .build(),
            detail_coverage: meter
                .f64_histogram("rehydration.quality.detail_coverage")
                .with_description("Fraction of nodes with detail")
                .build(),
        }
    }
}

impl QualityMetricsObserver for OTelQualityObserver {
    fn observe(&self, metrics: &BundleQualityMetrics, context: &QualityObservationContext) {
        let attrs = &[opentelemetry::KeyValue::new("rpc", context.rpc.clone())];
        self.raw_equivalent_tokens
            .record(metrics.raw_equivalent_tokens() as u64, attrs);
        self.compression_ratio
            .record(metrics.compression_ratio(), attrs);
        self.causal_density.record(metrics.causal_density(), attrs);
        self.noise_ratio.record(metrics.noise_ratio(), attrs);
        self.detail_coverage
            .record(metrics.detail_coverage(), attrs);
    }
}

// ── Tracing / Loki adapter ──────────────────────────────────────────────

/// Emits quality metrics as structured tracing log events.
///
/// When the kernel runs with `KMP_LOG_FORMAT=json`, these become
/// structured JSON log lines that Promtail / Grafana Agent collect and
/// push to Loki. Grafana can then query via LogQL:
///
/// ```logql
/// {job="kmp"} | json | quality_compression_ratio > 1.5
/// ```
pub struct TracingQualityObserver;

impl QualityMetricsObserver for TracingQualityObserver {
    fn observe(&self, metrics: &BundleQualityMetrics, context: &QualityObservationContext) {
        tracing::info!(
            target: "rehydration.quality",
            rpc = %context.rpc,
            root_node_id = %context.root_node_id,
            role = %context.role,
            quality_raw_equivalent_tokens = metrics.raw_equivalent_tokens(),
            quality_compression_ratio = metrics.compression_ratio(),
            quality_causal_density = metrics.causal_density(),
            quality_noise_ratio = metrics.noise_ratio(),
            quality_detail_coverage = metrics.detail_coverage(),
            "bundle quality metrics"
        );
    }
}

// ── Composite adapter ───────────────────────────────────────────────────

/// Fan-out observer that delegates to multiple backends.
///
/// ```rust,ignore
/// use kmp_observability::quality_observers::*;
/// let meter = opentelemetry::global::meter("example");
/// let observer = CompositeQualityObserver::new(vec![
///     Box::new(OTelQualityObserver::new(&meter)),
///     Box::new(TracingQualityObserver),
/// ]);
/// ```
/// Fan-out observer that spawns each adapter on a background task.
///
/// Individual adapters run fire-and-forget via `tokio::spawn`, keeping
/// the gRPC handler hot path free from observer I/O latency.
pub struct CompositeQualityObserver {
    observers: Arc<Vec<Box<dyn QualityMetricsObserver>>>,
}

impl CompositeQualityObserver {
    pub fn new(observers: Vec<Box<dyn QualityMetricsObserver>>) -> Self {
        Self {
            observers: Arc::new(observers),
        }
    }
}

impl QualityMetricsObserver for CompositeQualityObserver {
    fn observe(&self, metrics: &BundleQualityMetrics, context: &QualityObservationContext) {
        let observers = Arc::clone(&self.observers);
        let metrics = metrics.clone();
        let context = context.clone();
        tokio::spawn(async move {
            for observer in observers.iter() {
                observer.observe(&metrics, &context);
            }
        });
    }
}

// ── Noop adapter (for tests / when observability is disabled) ───────────

/// No-op observer that discards all metrics.
pub struct NoopQualityObserver;

impl QualityMetricsObserver for NoopQualityObserver {
    fn observe(&self, _metrics: &BundleQualityMetrics, _context: &QualityObservationContext) {}
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use kmp_domain::{BundleQualityMetrics, QualityMetricsObserver, QualityObservationContext};

    use super::{
        CompositeQualityObserver, NoopQualityObserver, OTelQualityObserver, TracingQualityObserver,
    };

    fn sample_metrics() -> BundleQualityMetrics {
        BundleQualityMetrics::new(200, 1.5, 0.6, 0.1, 0.8).expect("valid")
    }

    fn sample_context() -> QualityObservationContext {
        QualityObservationContext {
            rpc: "GetContext".to_string(),
            root_node_id: "node:case:123".to_string(),
            role: "developer".to_string(),
        }
    }

    #[test]
    fn noop_observer_does_not_panic() {
        NoopQualityObserver.observe(&sample_metrics(), &sample_context());
    }

    #[test]
    fn otel_observer_does_not_panic_with_noop_meter() {
        let meter = opentelemetry::global::meter("test");
        let observer = OTelQualityObserver::new(&meter);
        observer.observe(&sample_metrics(), &sample_context());
    }

    #[test]
    fn tracing_observer_does_not_panic() {
        TracingQualityObserver.observe(&sample_metrics(), &sample_context());
    }

    /// Recording spy for verifying composite fan-out.
    struct SpyObserver {
        count: Arc<Mutex<u32>>,
    }

    impl SpyObserver {
        fn new(count: Arc<Mutex<u32>>) -> Self {
            Self { count }
        }
    }

    impl QualityMetricsObserver for SpyObserver {
        fn observe(&self, _metrics: &BundleQualityMetrics, _context: &QualityObservationContext) {
            *self.count.lock().expect("mutex not poisoned") += 1;
        }
    }

    #[tokio::test]
    async fn composite_fans_out_to_all_observers() {
        let count = Arc::new(Mutex::new(0u32));
        let observer = CompositeQualityObserver::new(vec![
            Box::new(SpyObserver::new(Arc::clone(&count))),
            Box::new(SpyObserver::new(Arc::clone(&count))),
            Box::new(SpyObserver::new(Arc::clone(&count))),
        ]);
        observer.observe(&sample_metrics(), &sample_context());
        // Yield to let the spawned task run
        tokio::task::yield_now().await;
        assert_eq!(*count.lock().expect("mutex not poisoned"), 3);
    }
}
