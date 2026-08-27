use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::QualityTelemetryObservation;

/// Owns a runtime-neutral telemetry worker and closes it without requiring
/// senders to disconnect first.
pub struct EmbeddedTelemetryGuard {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl EmbeddedTelemetryGuard {
    pub fn try_spawn<W, F>(
        receiver: Receiver<QualityTelemetryObservation>,
        batch_size: usize,
        flush_interval: Duration,
        mut write_batch: W,
        finalize: F,
    ) -> Result<Self, io::Error>
    where
        W: FnMut(Vec<QualityTelemetryObservation>) + Send + 'static,
        F: FnOnce() + Send + 'static,
    {
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let batch_size = batch_size.max(1);
        let flush_interval = if flush_interval.is_zero() {
            Duration::from_millis(1)
        } else {
            flush_interval
        };
        let handle = std::thread::Builder::new()
            .name("quality-telemetry".to_string())
            .spawn(move || {
                let mut pending = Vec::with_capacity(batch_size);
                loop {
                    if worker_shutdown.load(Ordering::Acquire) {
                        pending.extend(receiver.try_iter());
                        flush_pending(&mut pending, batch_size, &mut write_batch);
                        break;
                    }
                    match receiver.recv_timeout(flush_interval) {
                        Ok(observation) => {
                            pending.push(observation);
                            if pending.len() >= batch_size {
                                flush_pending(&mut pending, batch_size, &mut write_batch);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            flush_pending(&mut pending, batch_size, &mut write_batch);
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            flush_pending(&mut pending, batch_size, &mut write_batch);
                            break;
                        }
                    }
                }
                finalize();
            })?;
        Ok(Self {
            shutdown,
            handle: Some(handle),
        })
    }

    pub fn close(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for EmbeddedTelemetryGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

fn flush_pending<W>(
    pending: &mut Vec<QualityTelemetryObservation>,
    batch_size: usize,
    write_batch: &mut W,
) where
    W: FnMut(Vec<QualityTelemetryObservation>),
{
    if pending.is_empty() {
        return;
    }
    let batch = std::mem::replace(pending, Vec::with_capacity(batch_size));
    write_batch(batch);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::Duration;

    use kmp_domain::{BundleQualityMetrics, QualityMetricsObserver, QualityObservationContext};

    use crate::BufferedQualityMetricsObserver;

    use super::EmbeddedTelemetryGuard;

    #[test]
    fn drop_drains_and_joins_even_while_the_observer_is_alive() {
        let (observer, receiver) = BufferedQualityMetricsObserver::with_capacity(4);
        let written = Arc::new(AtomicU64::new(0));
        let finalized = Arc::new(AtomicBool::new(false));
        let worker_written = Arc::clone(&written);
        let worker_finalized = Arc::clone(&finalized);
        let guard = EmbeddedTelemetryGuard::try_spawn(
            receiver,
            4,
            Duration::from_millis(5),
            move |batch| {
                worker_written.fetch_add(batch.len() as u64, Ordering::Relaxed);
            },
            move || worker_finalized.store(true, Ordering::Release),
        )
        .expect("worker starts");
        let metrics = BundleQualityMetrics::new(1, 1.0, 0.0, 0.0, 0.0).expect("valid metrics");
        observer.observe(
            &metrics,
            &QualityObservationContext {
                rpc: "kmp_wake".to_string(),
                root_node_id: "question:guard".to_string(),
                role: "resumer".to_string(),
                revision: Some(1),
            },
        );

        drop(guard);

        assert_eq!(written.load(Ordering::Relaxed), 1);
        assert!(finalized.load(Ordering::Acquire));
        assert_eq!(observer.dropped_observations(), 0);
    }
}
