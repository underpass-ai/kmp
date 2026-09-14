#!/usr/bin/env python3
"""Measure the existing SQLite lifecycle with the production adapters.

The temporary Rust runner prepares bounded synthetic stores through
``EmbeddedKernelStore`` and measures fresh opens, repeated opens, native
transactions, pinned snapshots, and diagnostic checkpoint state. It never
touches a user store; all data lives below the requested scratch directory.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


RUNNER = r'''use kmp_adapter_embedded::{
    EmbeddedKernelStore, QualityTelemetryRetention, SqliteQualityTelemetryWriter, StorageEngine,
};
use kmp_domain::{ContextEventChange, ContextEventStore, ContextUpdatedEvent};
use rusqlite::Connection;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

fn kernel_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store/kernel.sqlite3")
}

fn quality_path(data_dir: &Path) -> PathBuf {
    data_dir.join("telemetry/quality.sqlite3")
}

fn event(revision: u64, payload_bytes: usize) -> ContextUpdatedEvent {
    let payload = "x".repeat(payload_bytes);
    ContextUpdatedEvent {
        root_node_id: "perf:sqlite".to_string(),
        role: "memory".to_string(),
        revision,
        content_hash: format!("perf-hash-{revision}"),
        changes: vec![ContextEventChange {
            operation: "UPSERT".to_string(),
            entity_kind: "diagnostic".to_string(),
            entity_id: format!("perf-entry-{revision}"),
            payload_json: json!({"payload": payload}).to_string(),
            reason: Some("performance fixture".to_string()),
            scopes: Vec::new(),
        }],
        idempotency_key: None,
        logical_digest: None,
        requested_by: None,
        occurred_at: SystemTime::now(),
    }
}

fn wal_bytes(path: &Path) -> u64 {
    fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0)
}

fn checkpoint(path: &Path, mode: &str) -> (i64, i64, i64) {
    let connection = Connection::open(path).expect("checkpoint connection");
    connection
        .query_row(&format!("PRAGMA wal_checkpoint({mode})"), [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .expect("checkpoint result")
}

fn pragma_i64(connection: &Connection, name: &str) -> i64 {
    connection
        .pragma_query_value(None, name, |row| row.get(0))
        .unwrap_or(-1)
}

fn inventory(path: &Path) -> serde_json::Value {
    let connection = Connection::open(path).expect("inventory connection");
    let journal_mode: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("journal mode");
    let synchronous = pragma_i64(&connection, "synchronous");
    let cache_size = pragma_i64(&connection, "cache_size");
    let temp_store = pragma_i64(&connection, "temp_store");
    let page_count = pragma_i64(&connection, "page_count");
    let page_size = pragma_i64(&connection, "page_size");
    let wal_autocheckpoint = pragma_i64(&connection, "wal_autocheckpoint");
    let checkpoint_passive = checkpoint(path, "PASSIVE");
    json!({
        "path": path,
        "exists": path.exists(),
        "journal_mode": journal_mode,
        "synchronous": synchronous,
        "cache_size_raw": cache_size,
        "cache_size_unit": if cache_size < 0 { "negative KiB" } else { "pages" },
        "connection_scope": "fresh diagnostic connection; connection-local settings are not writer observations",
        "temp_store": temp_store,
        "page_count": page_count,
        "page_size": page_size,
        "file_bytes": fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0),
        "wal_bytes": wal_bytes(&path.with_extension("sqlite3-wal")),
        "wal_autocheckpoint_pages": wal_autocheckpoint,
        "checkpoint_passive": {
            "busy": checkpoint_passive.0,
            "log_frames": checkpoint_passive.1,
            "checkpointed_frames": checkpoint_passive.2,
        },
    })
}

async fn prepare(data_dir: &Path, events: u64, payload_bytes: usize) {
    fs::create_dir_all(data_dir).expect("data directory");
    let store = EmbeddedKernelStore::open_with_engine(data_dir, StorageEngine::Sqlite)
        .expect("production store creates");
    for revision in 0..events {
        store
            .append(event(revision + 1, payload_bytes), revision)
            .await
            .expect("production seed append");
    }
    drop(store);
    let _quality = SqliteQualityTelemetryWriter::open(
        data_dir,
        QualityTelemetryRetention::new(256).expect("quality retention"),
    )
    .expect("quality telemetry opens");
}

async fn measure(data_dir: &Path, warm_samples: usize) -> serde_json::Value {
    let kernel = kernel_path(data_dir);
    let cold_start = Instant::now();
    let cold_store = EmbeddedKernelStore::open(data_dir).expect("cold production open");
    let cold_open_ms = cold_start.elapsed().as_secs_f64() * 1000.0;
    drop(cold_store);

    let mut warm_open_ms = Vec::with_capacity(warm_samples);
    for _ in 0..warm_samples {
        let started = Instant::now();
        let store = EmbeddedKernelStore::open(data_dir).expect("warm production open");
        warm_open_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        drop(store);
    }

    let store = EmbeddedKernelStore::open(data_dir).expect("workload store");
    let mut write_ms = Vec::with_capacity(warm_samples);
    let base_revision = store
        .current_revision("perf:sqlite", "memory")
        .await
        .expect("seed revision");
    for offset in 0..warm_samples {
        let started = Instant::now();
        store
            .append(
                event(base_revision + offset as u64 + 1, 0),
                base_revision + offset as u64,
            )
            .await
            .expect("native write");
        write_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let mut read_ms = Vec::with_capacity(warm_samples);
    for _ in 0..warm_samples {
        let started = Instant::now();
        let _ = store.event_log_stats().await.expect("native read");
        read_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let mut snapshot_ms = Vec::with_capacity(warm_samples);
    for _ in 0..warm_samples {
        let started = Instant::now();
        let snapshot = store.read_snapshot().await.expect("snapshot");
        let _ = snapshot.event_log_stats().await.expect("snapshot read");
        snapshot_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let reader = Connection::open(&kernel).expect("pinned reader");
    reader
        .execute_batch("PRAGMA query_only=ON; BEGIN;")
        .expect("begin pinned raw reader");
    let before: i64 = reader
        .query_row("SELECT COUNT(*) FROM event_log", [], |row| row.get(0))
        .expect("pinned count");
    for offset in 0..warm_samples {
        store
            .append(
                event(base_revision + warm_samples as u64 + offset as u64 + 1, 0),
                base_revision + warm_samples as u64 + offset as u64,
            )
            .await
            .expect("writer while reader pinned");
    }
    let blocked_checkpoint = checkpoint(&kernel, "PASSIVE");
    let wal_while_pinned = wal_bytes(&kernel.with_file_name("kernel.sqlite3-wal"));
    drop(reader);
    let released_checkpoint = checkpoint(&kernel, "FULL");
    let wal_after_release = wal_bytes(&kernel.with_file_name("kernel.sqlite3-wal"));
    let after: i64 = Connection::open(&kernel)
        .expect("post-reader connection")
        .query_row("SELECT COUNT(*) FROM event_log", [], |row| row.get(0))
        .expect("post-reader count");

    let quality = quality_path(data_dir);
    let quality_inventory = inventory(&quality);
    let kernel_inventory = inventory(&kernel);
    json!({
        "cold_open_ms": cold_open_ms,
        "warm_open_ms": warm_open_ms,
        "write_ms": write_ms,
        "read_ms": read_ms,
        "snapshot_ms": snapshot_ms,
        "pinned_reader": {
            "rows_before": before,
            "rows_after_release": after,
            "writes_while_pinned": warm_samples,
            "wal_bytes_while_pinned": wal_while_pinned,
            "checkpoint_passive": {
                "busy": blocked_checkpoint.0,
                "log_frames": blocked_checkpoint.1,
                "checkpointed_frames": blocked_checkpoint.2,
            },
            "checkpoint_full_after_release": {
                "busy": released_checkpoint.0,
                "log_frames": released_checkpoint.1,
                "checkpointed_frames": released_checkpoint.2,
            },
            "wal_bytes_after_release": wal_after_release,
        },
        "kernel_inventory": kernel_inventory,
        "quality_inventory": quality_inventory,
    })
}

#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("prepare") => {
            prepare(
                Path::new(&args[2]),
                args[3].parse().expect("event count"),
                args[4].parse().expect("payload bytes"),
            )
            .await;
        }
        Some("measure") => {
            let result = measure(Path::new(&args[2]), args[3].parse().expect("warm samples")).await;
            println!("{}", serde_json::to_string(&result).expect("result json"));
        }
        other => panic!("mode must be prepare or measure, got {other:?}"),
    }
}
'''


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def output(*command: str) -> str:
    return subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True).stdout.strip()


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, math.ceil(len(ordered) * fraction) - 1))
    return ordered[index]


def hardware_fingerprint() -> tuple[str, str]:
    source = Path("/proc/cpuinfo")
    raw = source.read_bytes() if source.exists() else platform.platform().encode()
    model = next(
        (line.split(":", 1)[1].strip() for line in raw.decode(errors="replace").splitlines()
         if line.lower().startswith(("model name", "hardware", "cpu part")) and ":" in line),
        platform.processor() or "unknown",
    )
    return model, hashlib.sha256(raw).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scratch", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--warm-samples", type=int, default=21)
    parser.add_argument("--build-only", action="store_true")
    parser.add_argument("--runner-binary", type=Path)
    args = parser.parse_args()
    if args.warm_samples < 1:
        parser.error("--warm-samples must be positive")
    args.scratch.mkdir(parents=True, exist_ok=False)
    runner_source = ROOT / "crates/kmp-adapter-embedded/src/bin/sqlite-lifecycle-runner.rs"
    if args.runner_binary:
        binary = args.runner_binary.resolve()
    else:
        if runner_source.exists():
            raise SystemExit(f"refusing to overwrite {runner_source}")
        runner_source.parent.mkdir(parents=True, exist_ok=True)
        runner_source.write_text(RUNNER, encoding="utf-8")
        try:
            subprocess.run(
                [
                    "cargo", "build", "--locked", "-p", "kmp-adapter-embedded",
                    "--bin", "sqlite-lifecycle-runner",
                ],
                cwd=ROOT,
                check=True,
            )
        finally:
            runner_source.unlink(missing_ok=True)
        binary = ROOT / "target/debug/sqlite-lifecycle-runner"
    if args.build_only:
        print(binary)
        return 0

    model, hardware_sha256 = hardware_fingerprint()
    rows = []
    for label, events, payload_bytes in (("empty", 0, 0), ("medium", 128, 32768), ("large", 512, 32768)):
        data_dir = args.scratch / label
        subprocess.run(
            [str(binary), "prepare", str(data_dir), str(events), str(payload_bytes)],
            cwd=ROOT,
            check=True,
        )
        measured = subprocess.run(
            [str(binary), "measure", str(data_dir), str(args.warm_samples)],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        row = json.loads(measured.stdout)
        row.update({
            "shape": label,
            "seed_events": events,
            "payload_bytes_per_event": payload_bytes,
        })
        rows.append(row)
    result = {
        "issue": 772,
        "runner": str(Path(__file__).relative_to(ROOT)),
        "runner_sha256": digest(Path(__file__)),
        "runner_binary_sha256": digest(binary),
        "git_commit": output("git", "rev-parse", "HEAD"),
        "rustc": output("rustc", "-Vv"),
        "cargo_profile": "dev, workspace .cargo/config.toml",
        "host": {
            "system": platform.platform(),
            "machine": platform.machine(),
            "cpu_model": model,
            "hardware_sha256": hardware_sha256,
        },
        "method": {
            "native": "EmbeddedKernelStore and SqliteQualityTelemetryWriter",
            "diagnostics": "separate raw rusqlite PRAGMA/checkpoint queries; no production policy changes",
            "warm_samples_requested": args.warm_samples,
        },
        "shapes": rows,
        "source_hashes": {
            path: digest(ROOT / path)
            for path in (
                "crates/kmp-adapter-embedded/src/adapter/engine/sqlite.rs",
                "crates/kmp-adapter-embedded/src/adapter/telemetry/storage.rs",
                "crates/kmp-adapter-embedded/src/adapter/telemetry/sqlite_quality_telemetry_writer.rs",
            )
        },
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
