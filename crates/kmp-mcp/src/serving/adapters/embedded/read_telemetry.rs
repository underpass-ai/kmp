use kmp_domain::{
    BundleQualityMetrics, KmpBundle, QualityMetricsObserver, QualityObservationContext,
};

/// Forwards existing read quality and bundle identity to the session observer.
#[derive(Clone, Copy)]
pub(crate) struct EmbeddedReadTelemetry<'a> {
    observer: &'a dyn QualityMetricsObserver,
}

impl<'a> EmbeddedReadTelemetry<'a> {
    pub(crate) fn new(observer: &'a dyn QualityMetricsObserver) -> Self {
        Self { observer }
    }

    pub(crate) fn observe(&self, rpc: &str, bundle: &KmpBundle, quality: &BundleQualityMetrics) {
        self.observer.observe(
            quality,
            &QualityObservationContext {
                rpc: rpc.to_string(),
                root_node_id: bundle.root_node_id().as_str().to_string(),
                role: bundle.role().as_str().to_string(),
                revision: Some(bundle.metadata().revision),
            },
        );
    }
}
