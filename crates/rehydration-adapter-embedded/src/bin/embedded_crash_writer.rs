//! Crash-test workload driver: appends context events and applies
//! projections in a tight loop, printing each committed revision to stdout.
//! The crash-recovery test SIGKILLs this process mid-loop and verifies the
//! store's durability contract; the scale test runs it to corpus size.

use std::io::Write;
use std::time::SystemTime;

use rehydration_adapter_embedded::EmbeddedKernelStore;
use rehydration_domain::{
    ContextEventChange, ContextEventStore, ContextUpdatedEvent, NodeDetailProjection,
    NodeProjection, ProjectionMutation, ProjectionWriter,
};

const ABOUT: &str = "crash:test";
const ROLE: &str = "memory";
const SCOPE: &str = "about:crash:test:dimension:conversation:s1";

fn entry_id(revision: u64) -> String {
    format!("claim:{revision:06}")
}

fn event_for(revision: u64) -> ContextUpdatedEvent {
    let entry = entry_id(revision);
    let payload = serde_json::json!({
        "id": entry,
        "kind": "claim",
        "text": format!("Crash-test decision {revision}."),
        "coordinates": [{
            "dimension": "conversation",
            "scope_id": SCOPE,
            "occurred_at": format!("2026-07-21T{:02}:{:02}:00Z", revision / 60 % 24, revision % 60),
            "sequence": revision.max(1),
        }],
    })
    .to_string();

    ContextUpdatedEvent {
        root_node_id: ABOUT.to_string(),
        role: ROLE.to_string(),
        revision,
        content_hash: format!("hash-{revision}"),
        changes: vec![ContextEventChange {
            operation: "UPSERT".to_string(),
            entity_kind: "memory_entry".to_string(),
            entity_id: entry,
            payload_json: payload,
            reason: Some("crash-test ingest".to_string()),
            scopes: vec![SCOPE.to_string()],
        }],
        idempotency_key: Some(format!("ingest:crash-{revision}")),
        logical_digest: None,
        requested_by: None,
        occurred_at: SystemTime::now(),
    }
}

fn projection_for(revision: u64) -> Vec<ProjectionMutation> {
    let entry = entry_id(revision);
    vec![
        ProjectionMutation::UpsertNode(NodeProjection {
            node_id: entry.clone(),
            node_kind: "claim".to_string(),
            title: format!("Crash-test decision {revision}"),
            summary: format!("Crash-test decision {revision}."),
            status: "ACTIVE".to_string(),
            labels: vec!["memory".to_string(), "entry".to_string()],
            properties: Default::default(),
            provenance: None,
        }),
        ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
            node_id: entry,
            detail: format!("Crash-test decision {revision}."),
            content_hash: format!("detail-hash-{revision}"),
            revision,
        }),
    ]
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args
        .next()
        .expect("usage: embedded_crash_writer <data-dir> <count>");
    let count: u64 = args
        .next()
        .expect("usage: embedded_crash_writer <data-dir> <count>")
        .parse()
        .expect("count must be a positive integer");

    let store = EmbeddedKernelStore::open(std::path::Path::new(&data_dir))
        .expect("embedded store should open");
    let stdout = std::io::stdout();

    for revision in 1..=count {
        let committed = store
            .append(event_for(revision), revision - 1)
            .await
            .expect("append should succeed");
        store
            .apply_mutations(projection_for(committed))
            .await
            .expect("projection should apply");
        let mut lock = stdout.lock();
        writeln!(lock, "{committed}").expect("stdout write");
        lock.flush().expect("stdout flush");
    }
}
