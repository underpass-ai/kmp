//! Synthetic production-adapter clients for the #772 lifecycle control.
use std::fs;
use std::path::Path;
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
