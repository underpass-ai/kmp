//! Synthetic production-adapter clients for the #772 lifecycle control.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use kmp_adapter_embedded::{
    EmbeddedKernelStore, QualityTelemetryRetention, SqliteQualityTelemetryReader,
    SqliteQualityTelemetryWriter,
};
use kmp_domain::{
    BundleQualityMetrics, ContextEventChange, ContextEventStore, ContextUpdatedEvent,
    QualityObservationContext,
};
use kmp_observability::QualityTelemetryObservation;
use rusqlite::Connection;
use serde_json::{Value, json};

const SEED: usize = 128;

fn event(root: &str, revision: u64, bytes: usize) -> ContextUpdatedEvent {
    ContextUpdatedEvent {
        root_node_id: root.into(),
        role: "memory".into(),
        revision,
        content_hash: format!("{root}-{revision}"),
        changes: vec![ContextEventChange {
            operation: "UPSERT".into(),
            entity_kind: "synthetic".into(),
            entity_id: format!("{root}-{revision}"),
            payload_json: json!({"text": "x".repeat(bytes)}).to_string(),
            reason: None,
            scopes: vec![],
        }],
        idempotency_key: None,
        logical_digest: None,
        requested_by: None,
        occurred_at: SystemTime::UNIX_EPOCH,
    }
}

fn observation(root: &str, revision: usize) -> QualityTelemetryObservation {
    QualityTelemetryObservation::capture(
        &BundleQualityMetrics::new(100, 2.0, 0.5, 0.1, 0.9).expect("metrics"),
        &QualityObservationContext {
            rpc: "synthetic-lifecycle".into(),
            root_node_id: root.into(),
            role: "memory".into(),
            revision: Some(revision as u64),
        },
    )
}

fn writer(path: &Path) -> SqliteQualityTelemetryWriter {
    SqliteQualityTelemetryWriter::open(
        path,
        QualityTelemetryRetention::new(10_000).expect("retention"),
    )
    .expect("quality writer")
}

fn wal_bytes(kind: &str, path: &Path) -> u64 {
    let file = if kind == "kernel" {
        "store/kernel.sqlite3-wal"
    } else {
        "telemetry/quality.sqlite3-wal"
    };
    fs::metadata(path.join(file)).map(|m| m.len()).unwrap_or(0)
}

fn kernel_file(path: &Path) -> PathBuf {
    path.join("store/kernel.sqlite3")
}

fn kernel_wal_file(path: &Path) -> PathBuf {
    path.join("store/kernel.sqlite3-wal")
}

fn wal_frames(bytes: u64, page_size: u64) -> u64 {
    if bytes < 32 {
        return 0;
    }
    let frame_bytes = page_size + 24;
    assert_eq!((bytes - 32) % frame_bytes, 0, "complete WAL frames");
    (bytes - 32) / frame_bytes
}

fn peak_rss_kib() -> Option<u64> {
    fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn wait_start(barrier: &Path, id: &str) {
    fs::write(barrier.join(format!("ready-{id}")), []).expect("ready marker");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !barrier.join("go").exists() {
        assert!(Instant::now() < deadline, "barrier timeout");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn wait_for(path: &Path, message: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(Instant::now() < deadline, "{message}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

async fn prepare(kind: &str, path: &Path) {
    if kind == "kernel" {
        let store = EmbeddedKernelStore::open(path).expect("seed kernel");
        for i in 0..SEED {
            store
                .append(event("synthetic:seed", i as u64 + 1, 4096), i as u64)
                .await
                .expect("seed event");
        }
    } else {
        let journal = writer(path);
        for i in 0..SEED {
            journal
                .write_batch(&[observation("synthetic:seed", i)])
                .expect("seed observation");
        }
        journal.flush_durable().expect("seed flush");
    }
}

async fn client(kind: &str, path: &Path, id: &str, n: usize, barrier: &Path) -> Value {
    let root = format!("synthetic:client-{id}");
    let mut writes = Vec::new();
    let mut reads = Vec::new();
    let mut wal = Vec::new();
    let started = Instant::now();
    let open_ms;
    let flush_ms;
    if kind == "kernel" {
        let store = EmbeddedKernelStore::open(path).expect("client kernel");
        open_ms = started.elapsed().as_secs_f64() * 1000.0;
        wait_start(barrier, id);
        for i in 0..n {
            let value = event(&root, i as u64 + 1, 256);
            let start = Instant::now();
            store
                .append(value, i as u64)
                .await
                .expect("acknowledged event");
            writes.push(start.elapsed().as_secs_f64() * 1000.0);
            let start = Instant::now();
            let stats = store.event_log_stats().await.expect("native reader");
            reads.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(stats.0 > SEED as u64 + i as u64);
            wal.push(wal_bytes(kind, path));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            store
                .current_revision(&root, "memory")
                .await
                .expect("revision"),
            n as u64
        );
        flush_ms = None;
    } else {
        let journal = writer(path);
        let reader = SqliteQualityTelemetryReader::open(path).expect("independent reader");
        open_ms = started.elapsed().as_secs_f64() * 1000.0;
        wait_start(barrier, id);
        for i in 0..n {
            let value = observation(&root, i);
            let start = Instant::now();
            journal
                .write_batch(&[value])
                .expect("acknowledged observation");
            writes.push(start.elapsed().as_secs_f64() * 1000.0);
            let start = Instant::now();
            let rows = reader.latest(32).expect("native quality read");
            reads.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(rows.len(), 32);
            wal.push(wal_bytes(kind, path));
            std::thread::sleep(Duration::from_millis(1));
        }
        let start = Instant::now();
        journal.flush_durable().expect("quality durable tail");
        flush_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(journal.write_failures(), 0);
    }
    json!({"client": id, "open_ms": open_ms, "write_ms": writes, "read_ms": reads,
        "wal_bytes_after_each_write": wal, "durable_tail_ms": flush_ms, "peak_rss_kib": peak_rss_kib()})
}

async fn checkpoint_reader(path: &Path, barrier: &Path) -> Value {
    let store = EmbeddedKernelStore::open(path).expect("checkpoint reader opens");
    let snapshot = store
        .read_snapshot()
        .await
        .expect("checkpoint snapshot pins");
    let before = snapshot
        .event_log_stats()
        .await
        .expect("pinned snapshot count");
    fs::write(barrier.join("reader-ready"), []).expect("reader ready marker");
    wait_for(
        &barrier.join("reader-release"),
        "reader release barrier timeout",
    );
    let after = snapshot
        .event_log_stats()
        .await
        .expect("pinned snapshot remains readable");
    assert_eq!(after, before, "pinned reader must retain the old snapshot");
    drop(snapshot);
    fs::write(barrier.join("reader-released"), []).expect("reader released marker");
    wait_for(
        &barrier.join("reader-done"),
        "reader completion barrier timeout",
    );
    let current = store
        .event_log_stats()
        .await
        .expect("reader observes checkpointed current state");
    json!({
        "rows_before_writers": before.0,
        "last_sequence_before_writers": before.1,
        "rows_before_release": after.0,
        "last_sequence_before_release": after.1,
        "old_snapshot_preserved": true,
        "snapshot_released_before_full": true,
        "rows_after_full": current.0,
        "last_sequence_after_full": current.1,
    })
}

async fn checkpoint_client(
    path: &Path,
    id: &str,
    n: usize,
    payload_bytes: usize,
    barrier: &Path,
) -> Value {
    let root = format!("synthetic:client-{id}");
    let started = Instant::now();
    let store = EmbeddedKernelStore::open(path).expect("checkpoint writer opens");
    let open_ms = started.elapsed().as_secs_f64() * 1000.0;
    wait_start(barrier, id);
    let mut write_ms = Vec::with_capacity(n);
    let mut wal_bytes_after_each_write = Vec::with_capacity(n);
    for i in 0..n {
        let value = event(&root, i as u64 + 1, payload_bytes);
        let started = Instant::now();
        store
            .append(value, i as u64)
            .await
            .expect("checkpoint workload acknowledgement");
        write_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        wal_bytes_after_each_write.push(fs::metadata(kernel_wal_file(path)).map_or(0, |m| m.len()));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        store
            .current_revision(&root, "memory")
            .await
            .expect("checkpoint workload revision"),
        n as u64,
    );
    json!({
        "client": id,
        "open_ms": open_ms,
        "write_ms": write_ms,
        "wal_bytes_after_each_write": wal_bytes_after_each_write,
        "peak_rss_kib": peak_rss_kib(),
    })
}

fn checkpoint_diagnostic(path: &Path, mode: &str) -> Value {
    assert!(mode == "PASSIVE" || mode == "FULL");
    let database = kernel_file(path);
    let connection = Connection::open(&database).expect("checkpoint diagnostic connection");
    let page_size_value: i64 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .expect("page size");
    let page_size = u64::try_from(page_size_value).expect("positive page size");
    let autocheckpoint_value: i64 = connection
        .pragma_query_value(None, "wal_autocheckpoint", |row| row.get(0))
        .expect("autocheckpoint threshold");
    let autocheckpoint_pages =
        u64::try_from(autocheckpoint_value).expect("positive autocheckpoint threshold");
    let wal_before = fs::metadata(kernel_wal_file(path)).map_or(0, |m| m.len());
    let started = Instant::now();
    let result: (i64, i64, i64) = connection
        .query_row(&format!("PRAGMA wal_checkpoint({mode})"), [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .expect("checkpoint result");
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let wal_after = fs::metadata(kernel_wal_file(path)).map_or(0, |m| m.len());
    json!({
        "mode": mode,
        "duration_ms": elapsed_ms,
        "busy": result.0,
        "log_frames": result.1,
        "checkpointed_frames": result.2,
        "page_size_bytes": page_size,
        "wal_autocheckpoint_pages": autocheckpoint_pages,
        "wal_bytes_before": wal_before,
        "wal_frames_before": wal_frames(wal_before, page_size),
        "wal_bytes_after": wal_after,
        "wal_frames_after": wal_frames(wal_after, page_size),
        "connection_scope": "explicit fresh diagnostic connection; not a production writer observation",
    })
}

async fn verify(kind: &str, path: &Path, clients: usize, n: usize) -> Value {
    let count = if kind == "kernel" {
        let store = EmbeddedKernelStore::open(path).expect("reopen kernel");
        for id in 0..clients {
            assert_eq!(
                store
                    .current_revision(&format!("synthetic:client-{id}"), "memory")
                    .await
                    .expect("revision"),
                n as u64
            );
        }
        store.event_log_stats().await.expect("count").0
    } else {
        let reader = SqliteQualityTelemetryReader::open(path).expect("reopen quality");
        let rows = reader.latest(10_000).expect("all bounded observations");
        for id in 0..clients {
            let root = format!("synthetic:client-{id}");
            assert_eq!(rows.iter().filter(|r| r.root_node_id() == root).count(), n);
        }
        reader.count().expect("count")
    };
    assert_eq!(count, (SEED + clients * n) as u64);
    json!({"reopened_count": count, "expected_count": SEED + clients * n, "every_client_acknowledgement_preserved": true})
}

#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    let kind = &args[2];
    assert!(kind == "kernel" || kind == "quality");
    let path = Path::new(&args[3]);
    let value = match args[1].as_str() {
        "prepare" => {
            prepare(kind, path).await;
            json!({"prepared": true})
        }
        "client" => {
            client(
                kind,
                path,
                &args[4],
                args[5].parse().expect("samples"),
                Path::new(&args[6]),
            )
            .await
        }
        "checkpoint-reader" => checkpoint_reader(path, Path::new(&args[4])).await,
        "checkpoint-client" => {
            checkpoint_client(
                path,
                &args[4],
                args[5].parse().expect("samples"),
                args[6].parse().expect("payload bytes"),
                Path::new(&args[7]),
            )
            .await
        }
        "checkpoint" => checkpoint_diagnostic(path, &args[4]),
        "verify" => {
            verify(
                kind,
                path,
                args[4].parse().expect("clients"),
                args[5].parse().expect("samples"),
            )
            .await
        }
        _ => panic!("unsupported mode"),
    };
    println!("{value}");
}
