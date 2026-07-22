//! Runtime-neutral, non-blocking quality-telemetry buffering.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use rehydration_domain::{BundleQualityMetrics, QualityMetricsObserver, QualityObservationContext};

use crate::QualityTelemetryObservation;

/// Implements `QualityMetricsObserver` over a bounded fail-open channel.
pub struct BufferedQualityMetricsObserver {
    sender: SyncSender<QualityTelemetryObservation>,
    dropped: AtomicU64,
}

impl BufferedQualityMetricsObserver {
    pub fn with_capacity(capacity: usize) -> (Self, Receiver<QualityTelemetryObservation>) {
        let (sender, receiver) = sync_channel(capacity);
        (
            Self {
                sender,
                dropped: AtomicU64::new(0),
            },
            receiver,
        )
    }

    /// Observations discarded because the buffer was full or disconnected.
    pub fn dropped_observations(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl QualityMetricsObserver for BufferedQualityMetricsObserver {
    fn observe(&self, metrics: &BundleQualityMetrics, context: &QualityObservationContext) {
        let observation = QualityTelemetryObservation::capture(metrics, context);
        if self.sender.try_send(observation).is_err() {
            let dropped = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            if dropped == 1 || dropped.is_multiple_of(1000) {
                tracing::warn!(
                    dropped,
                    "quality telemetry buffer unavailable; dropping observations"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rehydration_domain::{
        BundleQualityMetrics, QualityMetricsObserver, QualityObservationContext,
    };

    use super::BufferedQualityMetricsObserver;

    fn sample_metrics() -> BundleQualityMetrics {
        BundleQualityMetrics::new(120, 2.5, 0.4, 0.1, 0.8).expect("valid metrics")
    }

    fn sample_context() -> QualityObservationContext {
        QualityObservationContext {
            rpc: "kernel_wake".to_string(),
            root_node_id: "question:t".to_string(),
            role: "resumer".to_string(),
        }
    }

    #[test]
    fn observations_flow_through_the_bounded_channel() {
        let (observer, receiver) = BufferedQualityMetricsObserver::with_capacity(4);

        observer.observe(&sample_metrics(), &sample_context());

        let observation = receiver.try_recv().expect("observation buffered");
        assert_eq!(observation.rpc(), "kernel_wake");
        assert_eq!(observation.root_node_id(), "question:t");
        assert_eq!(observation.raw_equivalent_tokens(), 120);
        assert!(observation.observed_at_millis() > 0);
        assert_eq!(observer.dropped_observations(), 0);
    }

    #[test]
    fn overflow_drops_and_counts_without_blocking() {
        let (observer, receiver) = BufferedQualityMetricsObserver::with_capacity(1);

        observer.observe(&sample_metrics(), &sample_context());
        observer.observe(&sample_metrics(), &sample_context());
        observer.observe(&sample_metrics(), &sample_context());

        assert_eq!(observer.dropped_observations(), 2);
        assert!(receiver.try_recv().is_ok());
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn disconnected_worker_never_fails_the_kernel() {
        let (observer, receiver) = BufferedQualityMetricsObserver::with_capacity(4);
        drop(receiver);

        observer.observe(&sample_metrics(), &sample_context());

        assert_eq!(observer.dropped_observations(), 1);
    }
}
