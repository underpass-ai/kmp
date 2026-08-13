//! E2 exit criterion: the store survives a 100k-event corpus with documented
//! reopen time and file size. Run manually in release:
//!
//! ```bash
//! cargo test -p kmp-adapter-embedded --release --test store_scale -- --ignored --nocapture
//! ```

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::projection_mutations_for_context_event;
use kmp_domain::{ContextEventStore, GraphNeighborhoodReader, NodeDetailReader};

const CORPUS_EVENTS: u64 = 100_000;

fn dir_size_bytes(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                total += dir_size_bytes(&entry_path);
            } else if let Ok(metadata) = entry.metadata() {
                total += metadata.len();
            }
        }
    }
    total
}

#[tokio::test]
#[ignore = "scale run (~minutes, fsync per event); execute manually in release"]
async fn store_survives_100k_event_corpus() {
    let data_dir = tempfile::tempdir().expect("temp data dir");

    let ingest_started = Instant::now();
    let status = Command::new(env!("CARGO_BIN_EXE_embedded_crash_writer"))
        .arg(data_dir.path())
        .arg(CORPUS_EVENTS.to_string())
        .stdout(std::process::Stdio::null())
        .status()
        .expect("corpus writer runs");
    assert!(status.success(), "corpus writer must complete");
    let ingest_elapsed = ingest_started.elapsed();

    let reopen_started = Instant::now();
    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store reopens");
    let detail = store
        .load_node_detail("claim:000001")
        .await
        .expect("detail reads")
        .expect("first detail exists");
    let reopen_elapsed = reopen_started.elapsed();
    assert_eq!(detail.revision, 1);

    let revision = store
        .current_revision("crash:test", "memory")
        .await
        .expect("revision reads");
    assert_eq!(revision, CORPUS_EVENTS);
    let (log_length, last_sequence) = store.event_log_stats().await.expect("log stats");
    assert_eq!(log_length, CORPUS_EVENTS);
    assert_eq!(last_sequence, CORPUS_EVENTS);

    let rebuild_started = Instant::now();
    let report = store
        .rebuild_projections(projection_mutations_for_context_event)
        .await
        .expect("projection rebuild");
    let rebuild_elapsed = rebuild_started.elapsed();
    assert_eq!(report.events_replayed, CORPUS_EVENTS);

    let neighborhood = store
        .load_neighborhood("crash:test", 1)
        .await
        .expect("neighborhood reads")
        .expect("anchor exists");
    assert_eq!(neighborhood.neighbors.len() as u64, CORPUS_EVENTS);

    let size_bytes = dir_size_bytes(data_dir.path());
    eprintln!(
        "100k-corpus results: ingest {:.1}s ({:.0} ev/s durable), reopen+first-read {:.1}ms, \
         projection rebuild {:.1}s, store size {:.1} MB",
        ingest_elapsed.as_secs_f64(),
        CORPUS_EVENTS as f64 / ingest_elapsed.as_secs_f64(),
        reopen_elapsed.as_secs_f64() * 1000.0,
        rebuild_elapsed.as_secs_f64(),
        size_bytes as f64 / 1e6,
    );
    assert!(
        reopen_elapsed.as_millis() < 1000,
        "reopen must stay session-start cheap (ADR-009)"
    );
}
