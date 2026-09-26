use kmp_application::{RenderDemand, RenderedContext};
use kmp_domain::{KmpBundle, QualityMetricsObserver, QualityObservationContext};

/// Forwards existing read quality and bundle identity to the session observer.
#[derive(Clone, Copy)]
pub(crate) struct EmbeddedReadTelemetry<'a> {
    observer: &'a dyn QualityMetricsObserver,
}

impl<'a> EmbeddedReadTelemetry<'a> {
    pub(crate) fn new(observer: &'a dyn QualityMetricsObserver) -> Self {
        Self { observer }
    }

    /// Existing application phases, opt-in through the tracing filter. These
    /// exclude recall ranking, rendering and transport; they are not a total.
    pub(crate) fn trace_timing(
        &self,
        rpc: &str,
        timing: Option<&kmp_application::QueryTimingBreakdown>,
    ) {
        if let Some(timing) = timing {
            tracing::debug!(
                target: "kmp_mcp::query_phases",
                rpc,
                graph_load_us = timing.graph_load.as_micros() as u64,
                detail_load_us = timing.detail_load.as_micros() as u64,
                bundle_assembly_us = timing.bundle_assembly.as_micros() as u64,
                role_count = timing.role_count,
                batch_size = timing.batch_size,
                "embedded query phases"
            );
        }
    }

    /// What a read must render: what its response projects, or the complete
    /// measured render when an active observer wants the quality of it.
    pub(crate) fn render_demand(&self, projected: RenderDemand) -> RenderDemand {
        if self.observer.is_active() {
            RenderDemand::Measured
        } else {
            projected
        }
    }

    /// Hands a measured render to the observer. An unmeasured one carries no
    /// quality to observe, and an absent observation is not a zero.
    pub(crate) fn observe(&self, rpc: &str, bundle: &KmpBundle, rendered: &RenderedContext) {
        if !rendered.demand.is_measured() {
            return;
        }
        self.observer.observe(
            &rendered.quality,
            &QualityObservationContext {
                rpc: rpc.to_string(),
                root_node_id: bundle.root_node_id().as_str().to_string(),
                role: bundle.role().as_str().to_string(),
                revision: Some(bundle.metadata().revision),
            },
        );
    }
}
