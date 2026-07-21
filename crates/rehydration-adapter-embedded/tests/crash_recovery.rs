//! E2 exit criterion: `kill -9` during writes, reopen, replay — no data loss
//! beyond the in-flight event, no duplicate application.

use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

use rehydration_adapter_embedded::EmbeddedKernelStore;
use rehydration_application::projection_mutations_for_context_event;
use rehydration_domain::{ContextEventStore, GraphNeighborhoodReader, NodeDetailReader};

const ABOUT: &str = "crash:test";
const ROLE: &str = "memory";

#[tokio::test]
async fn kill_nine_mid_write_loses_at_most_the_inflight_event_and_replays_cleanly() {
    let data_dir = tempfile::tempdir().expect("temp data dir");

    let mut writer = Command::new(env!("CARGO_BIN_EXE_embedded_crash_writer"))
        .arg(data_dir.path())
        .arg("100000")
        .stdout(Stdio::piped())
        .spawn()
        .expect("crash writer should spawn");
    let mut reader = BufReader::new(writer.stdout.take().expect("writer stdout"));

    let mut line = String::new();
    let mut observed = 0u64;
    while observed < 25 {
        line.clear();
        let read = reader.read_line(&mut line).expect("read writer progress");
        assert!(read > 0, "writer exited before reaching 25 events");
        observed += 1;
    }

    // SIGKILL: no destructors, no flush — the process dies mid-loop.
    writer.kill().expect("kill -9 the writer");
    writer.wait().expect("reap the writer");
    // Commits acknowledged after our 25th read still count as durable.
    let mut rest = String::new();
    reader
        .read_to_string(&mut rest)
        .expect("drain writer stdout");
    let acknowledged = observed + rest.lines().count() as u64;

    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store reopens after crash");

    let revision = store
        .current_revision(ABOUT, ROLE)
        .await
        .expect("revision reads");
    assert!(
        revision >= acknowledged && revision <= acknowledged + 1,
        "durability contract: every acknowledged event survives ({acknowledged} acknowledged), \
         at most one in-flight event beyond that ({revision} recovered)"
    );

    let (log_length, last_sequence) = store.event_log_stats().await.expect("log stats");
    assert_eq!(
        log_length, revision,
        "event log length must equal the aggregate revision: no loss, no duplicates"
    );
    assert_eq!(
        last_sequence, revision,
        "event sequences must be contiguous from 1"
    );

    // The crash window between event append and projection apply is healed
    // by replaying the log; replay must be deterministic and complete.
    let report = store
        .rebuild_projections(projection_mutations_for_context_event)
        .await
        .expect("projection rebuild");
    assert_eq!(report.events_replayed, revision);
    assert!(report.mutations_applied >= revision);

    let neighborhood = store
        .load_neighborhood(ABOUT, 1)
        .await
        .expect("neighborhood reads")
        .expect("anchor exists after rebuild");
    assert_eq!(
        neighborhood.neighbors.len() as u64,
        revision,
        "rebuild must materialize exactly one entry per surviving event"
    );

    let detail = store
        .load_node_detail("claim:000001")
        .await
        .expect("detail reads")
        .expect("first entry detail exists");
    assert_eq!(detail.revision, 1, "no duplicate application on replay");
}
