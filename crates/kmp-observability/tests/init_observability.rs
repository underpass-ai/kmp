//! Initialization installs the process-wide subscriber, so it runs once, in
//! its own test binary.

#[test]
fn initialization_installs_logging_and_metrics_and_shuts_down() {
    let guard = kmp_observability::init_observability("kmp-observability-test");
    guard.metrics.rpc_duration.record(0.25, &[]);
    guard.metrics.truncation_total.add(1, &[]);
    tracing::info!("observability test line");
    kmp_observability::shutdown_observability(guard);
}
