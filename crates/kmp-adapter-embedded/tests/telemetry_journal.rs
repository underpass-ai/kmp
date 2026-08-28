//! The local journal is buffered, persistent, bounded, and queryable.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use kmp_adapter_embedded::{
    QualityTelemetryRetention, SqliteQualityTelemetryReader, SqliteQualityTelemetryWriter,
    quality_telemetry_path,
};
use kmp_application::{ObservabilityQuery, ObservabilityQueryPort};
use kmp_domain::{BundleQualityMetrics, QualityMetricsObserver, QualityObservationContext};
use kmp_observability::{
    BufferedQualityMetricsObserver, EmbeddedTelemetryGuard, QualityTelemetryObservation,
};
use redb::{Database, TableDefinition};

const LEGACY_OBSERVATIONS: TableDefinition<(u64, u64), &[u8]> =
    TableDefinition::new("quality_observations");

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
        SqliteQualityTelemetryWriter::open_with_durable_interval(data_dir.path(), retention, 1)
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
        data_dir.path().join("telemetry/quality.sqlite3")
    );
    let reader = SqliteQualityTelemetryReader::open(data_dir.path()).expect("reader opens");
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
    let writer = SqliteQualityTelemetryWriter::open(
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

#[test]
fn two_independent_writers_share_one_quality_journal() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let retention = QualityTelemetryRetention::new(100).expect("retention");
    let first = SqliteQualityTelemetryWriter::open(data_dir.path(), retention).expect("first");
    let second = SqliteQualityTelemetryWriter::open(data_dir.path(), retention).expect("second");
    let metrics = BundleQualityMetrics::new(100, 2.0, 0.5, 0.1, 0.9).expect("metrics");
    let observation = |about: &str| {
        QualityTelemetryObservation::capture(
            &metrics,
            &QualityObservationContext {
                rpc: "kmp_wake".to_string(),
                root_node_id: about.to_string(),
                role: "resumer".to_string(),
                revision: None,
            },
        )
    };

    first
        .write_batch(&[observation("project:first")])
        .expect("first writes");
    second
        .write_batch(&[observation("project:second")])
        .expect("second writes");

    let observations = first.reader().latest(10).expect("shared read");
    assert_eq!(observations.len(), 2);
    assert!(
        observations
            .iter()
            .any(|value| value.root_node_id() == "project:first")
    );
    assert!(
        observations
            .iter()
            .any(|value| value.root_node_id() == "project:second")
    );
}

#[test]
fn telemetry_process_worker() {
    let Ok(data_dir) = std::env::var("KMP_TEST_QUALITY_DATA_DIR") else {
        return;
    };
    let about = std::env::var("KMP_TEST_QUALITY_ABOUT").expect("worker about");
    let writer = SqliteQualityTelemetryWriter::open(
        Path::new(&data_dir),
        QualityTelemetryRetention::new(1_000).expect("retention"),
    )
    .expect("worker writer opens");
    let metrics = BundleQualityMetrics::new(100, 2.0, 0.5, 0.1, 0.9).expect("metrics");
    let observations = (0..50)
        .map(|revision| {
            QualityTelemetryObservation::capture(
                &metrics,
                &QualityObservationContext {
                    rpc: "kmp_wake".to_string(),
                    root_node_id: about.clone(),
                    role: "resumer".to_string(),
                    revision: Some(revision),
                },
            )
        })
        .collect::<Vec<_>>();
    writer.write_batch(&observations).expect("worker writes");
    writer.flush_durable().expect("worker flushes");
}

#[test]
fn two_processes_keep_the_same_observability_pulse() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let legacy_path = data_dir.path().join("telemetry/quality.redb");
    std::fs::create_dir_all(legacy_path.parent().expect("parent")).expect("telemetry dir");
    {
        let database = Database::create(&legacy_path).expect("legacy journal");
        let transaction = database.begin_write().expect("legacy transaction");
        transaction
            .open_table(LEGACY_OBSERVATIONS)
            .expect("legacy observations");
        transaction.commit().expect("legacy commit");
    }
    let executable = std::env::current_exe().expect("current test executable");
    let spawn = |about: &str| {
        std::process::Command::new(&executable)
            .args(["--exact", "telemetry_process_worker", "--nocapture"])
            .env("KMP_TEST_QUALITY_DATA_DIR", data_dir.path())
            .env("KMP_TEST_QUALITY_ABOUT", about)
            .spawn()
            .expect("worker spawns")
    };
    let first = spawn("project:first-process");
    let second = spawn("project:second-process");
    for child in [first, second] {
        assert!(
            child
                .wait_with_output()
                .expect("worker exits")
                .status
                .success()
        );
    }

    let reader = SqliteQualityTelemetryReader::open(data_dir.path()).expect("reader opens");
    let observations = reader.latest(1_000).expect("journal reads");
    assert_eq!(observations.len(), 100);
    for about in ["project:first-process", "project:second-process"] {
        assert_eq!(
            observations
                .iter()
                .filter(|observation| observation.root_node_id() == about)
                .count(),
            50,
            "each host keeps its observations"
        );
    }
    assert!(legacy_path.is_file(), "concurrent import keeps the source");
}

#[test]
fn legacy_redb_history_is_imported_once_and_left_in_place() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let legacy_path = data_dir.path().join("telemetry/quality.redb");
    std::fs::create_dir_all(legacy_path.parent().expect("parent")).expect("telemetry dir");
    let metrics = BundleQualityMetrics::new(100, 2.0, 0.5, 0.1, 0.9).expect("metrics");
    let observation = QualityTelemetryObservation::capture(
        &metrics,
        &QualityObservationContext {
            rpc: "kmp_ask".to_string(),
            root_node_id: "project:legacy".to_string(),
            role: "resumer".to_string(),
            revision: Some(7),
        },
    );
    {
        let database = Database::create(&legacy_path).expect("legacy journal");
        let transaction = database.begin_write().expect("legacy transaction");
        {
            let mut table = transaction
                .open_table(LEGACY_OBSERVATIONS)
                .expect("legacy observations");
            let payload = serde_json::to_vec(&observation).expect("legacy payload");
            table
                .insert((observation.observed_at_millis(), 1), payload.as_slice())
                .expect("legacy insert");
        }
        transaction.commit().expect("legacy commit");
    }

    let first = SqliteQualityTelemetryWriter::open(
        data_dir.path(),
        QualityTelemetryRetention::new(10).expect("retention"),
    )
    .expect("sqlite journal opens");
    assert_eq!(first.migrated_legacy_observations(), 1);
    assert_eq!(first.reader().count().expect("count"), 1);
    drop(first);

    let second = SqliteQualityTelemetryWriter::open(
        data_dir.path(),
        QualityTelemetryRetention::new(10).expect("retention"),
    )
    .expect("sqlite journal reopens");
    assert_eq!(second.migrated_legacy_observations(), 0);
    assert_eq!(second.reader().count().expect("count"), 1);
    assert!(
        legacy_path.is_file(),
        "the migration keeps the source evidence"
    );
}
