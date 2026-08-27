//! The local journal is buffered, persistent, bounded, and queryable.

use std::sync::Arc;
use std::time::Duration;

use kmp_adapter_embedded::{
    QualityTelemetryRetention, RedbQualityTelemetryReader, RedbQualityTelemetryWriter,
    quality_telemetry_path,
};
use kmp_application::{ObservabilityQuery, ObservabilityQueryPort};
use kmp_domain::{BundleQualityMetrics, QualityMetricsObserver, QualityObservationContext};
use kmp_observability::{
    BufferedQualityMetricsObserver, EmbeddedTelemetryGuard, QualityTelemetryObservation,
};

fn observe_n(observer: &BufferedQualityMetricsObserver, rpc: &str, count: usize) {
    let metrics = BundleQualityMetrics::new(100, 2.0, 0.5, 0.1, 0.9).expect("metrics");
    for index in 0..count {
        observer.observe(
            &metrics,
            &QualityObservationContext {
                rpc: rpc.to_string(),
                root_node_id: format!("question:{index}"),
                role: "resumer".to_string(),
                revision: Some(index as u64),
            },
        );
    }
}

#[test]
fn observations_persist_survive_reopen_and_respect_retention() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let retention = QualityTelemetryRetention::new(10).expect("retention");
    let writer = Arc::new(
        RedbQualityTelemetryWriter::open_with_durable_interval(data_dir.path(), retention, 1)
            .expect("journal writer opens"),
    );
    let (observer, receiver) = BufferedQualityMetricsObserver::with_capacity(64);
    let batch_writer = Arc::clone(&writer);
    let final_writer = Arc::clone(&writer);
    let guard = EmbeddedTelemetryGuard::try_spawn(
        receiver,
        8,
        Duration::from_millis(20),
        move |batch| {
            batch_writer.write_batch(&batch).expect("batch persists");
        },
        move || {
            final_writer.flush_durable().expect("tail flushes");
        },
    )
    .expect("worker starts");

    observe_n(&observer, "kmp_wake", 12);
    observe_n(&observer, "kmp_ask", 3);
    drop(observer);
    guard.close();
    assert_eq!(writer.write_failures(), 0);
    drop(writer);

    assert_eq!(
        quality_telemetry_path(data_dir.path()),
        data_dir.path().join("telemetry/quality.redb")
    );
    let reader = RedbQualityTelemetryReader::open(data_dir.path()).expect("reader opens");
    let wakes = reader
        .query_since(0, Some("kmp_wake"), 100)
        .expect("query wakes");
    let asks = reader
        .query_since(0, Some("kmp_ask"), 100)
        .expect("query asks");
    let all = reader.latest(100).expect("query latest");
    assert_eq!(reader.count().expect("count"), 10);
    assert_eq!(all.len(), 10, "retention must cap the journal");
    assert_eq!(asks.len(), 3, "newest observations must survive retention");
    assert_eq!(
        asks.last().and_then(|observation| observation.revision()),
        Some(2)
    );
    assert!(wakes.len() >= 7, "older kind keeps the remainder");
    assert!(
        all.iter()
            .all(|observation| observation.observed_at_millis() > 0)
    );
}

#[tokio::test]
async fn observability_projection_filters_the_about_and_keeps_revision_exemplars() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let writer = RedbQualityTelemetryWriter::open(
        data_dir.path(),
        QualityTelemetryRetention::new(10).expect("retention"),
    )
    .expect("writer");
    let metrics = BundleQualityMetrics::new(100, 2.0, 0.5, 0.1, 0.9).expect("metrics");
    let observation = |about: &str, revision| {
        QualityTelemetryObservation::capture(
            &metrics,
            &QualityObservationContext {
                rpc: "kmp_wake".to_string(),
                root_node_id: about.to_string(),
                role: "resumer".to_string(),
                revision: Some(revision),
            },
        )
    };
    writer
        .write_batch(&[observation("project:a", 7), observation("project:b", 8)])
        .expect("observations persist");

    let projection = writer
        .reader()
        .query(ObservabilityQuery {
            about: Some("project:a".to_string()),
            from_millis: 0,
            to_millis: u64::MAX,
            series: vec!["causal_density".to_string()],
            max_points: 10,
        })
        .await
        .expect("projection");

    assert_eq!(projection.exemplars.len(), 1);
    assert_eq!(projection.exemplars[0].about.as_deref(), Some("project:a"));
    assert_eq!(projection.exemplars[0].revision, Some(7));
    assert_eq!(projection.series[0].points.len(), 1);
}
