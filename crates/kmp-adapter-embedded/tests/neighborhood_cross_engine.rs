use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::{
    CommandApplicationService, KernelMemoryApplicationService, MemoryCoordinateData, MemoryData,
    MemoryDimensionData, MemoryEntryData, MemoryIngestCommand, MemoryRelationData,
    QueryApplicationService, RoutingProjectionWriter, UpdateContextUseCase,
};
use kmp_domain::{ContextEventStore, ContextUpdatedEvent, IdempotentOutcome, PortError};
use tokio::sync::Notify;

const ABOUT: &str = "project:concurrent-review";

// Return a real revision read, delayed at a controlled scheduling boundary.
// A peer uses a separate engine and application service over the same SQLite file.
struct PausedEvents {
    store: EmbeddedKernelStore,
    pause_on: AtomicUsize,
    paused: Notify,
    resume: Notify,
}

impl ContextEventStore for PausedEvents {
    fn commits_projections_atomically(&self) -> bool {
        self.store.commits_projections_atomically()
    }
    async fn append_projected(
        &self,
        event: ContextUpdatedEvent,
        expected: u64,
        read_revisions: Vec<kmp_domain::ContextRevision>,
        mutations: Vec<kmp_domain::ProjectionMutation>,
    ) -> Result<u64, PortError> {
        self.store
            .append_projected(event, expected, read_revisions, mutations)
            .await
    }
    async fn append(&self, event: ContextUpdatedEvent, expected: u64) -> Result<u64, PortError> {
        self.store.append(event, expected).await
    }
    async fn current_revision(&self, root: &str, role: &str) -> Result<u64, PortError> {
        let revision = self.store.current_revision(root, role).await?;
        if self
            .pause_on
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            == Ok(1)
        {
            self.paused.notify_one();
            self.resume.notified().await;
        }
        Ok(revision)
    }
    async fn current_content_hash(
        &self,
        root: &str,
        role: &str,
    ) -> Result<Option<String>, PortError> {
        self.store.current_content_hash(root, role).await
    }
    async fn find_by_idempotency_key(
        &self,
        key: &str,
    ) -> Result<Option<IdempotentOutcome>, PortError> {
        self.store.find_by_idempotency_key(key).await
    }
}

fn service<E: ContextEventStore + Send + Sync>(
    store: EmbeddedKernelStore,
    events: Arc<E>,
) -> KernelMemoryApplicationService<
    EmbeddedKernelStore,
    EmbeddedKernelStore,
    EmbeddedKernelStore,
    E,
    RoutingProjectionWriter<Arc<EmbeddedKernelStore>, Arc<EmbeddedKernelStore>>,
> {
    let store = Arc::new(store);
    KernelMemoryApplicationService::new(
        Arc::new(QueryApplicationService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            "test",
        )),
        Arc::new(CommandApplicationService::new(Arc::new(
            UpdateContextUseCase::new_with_projection_writer(
                events,
                RoutingProjectionWriter::new(store.clone(), store),
                "test",
            ),
        ))),
    )
}

fn command(key: &str, rich: bool) -> MemoryIngestCommand {
    let entry = |suffix: &str| MemoryEntryData {
        id: format!("{ABOUT}:entry:observation:{key}-{suffix}"),
        kind: "observation".into(),
        text: format!("Observation {key} {suffix}"),
        metadata: Default::default(),
        coordinates: vec![MemoryCoordinateData {
            dimension: "task".into(),
            scope_id: "task:review".into(),
            observed_at: Some("2026-09-10T12:00:00Z".into()),
            occurred_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
            sequence: None,
            rank: None,
            metadata: Default::default(),
        }],
    };
    let mut entries = vec![entry("first")];
    let mut relations = vec![];
    if rich {
        entries.push(entry("second"));
        relations.push(MemoryRelationData {
            source_ref: entries[0].id.clone(),
            target_ref: entries[1].id.clone(),
            rel: "verified_by".into(),
            semantic_class: "evidential".into(),
            why: Some("The second observation verifies the first.".into()),
            evidence: Some("Source records both observations and their check.".into()),
            confidence: Some("high".into()),
            sequence: Some(1),
            motivation: None,
            method: None,
            decision_id: None,
            caused_by_node_id: None,
            coordinate: None,
        });
    }
    MemoryIngestCommand {
        about: ABOUT.into(),
        idempotency_key: key.into(),
        dry_run: false,
        provenance: None,
        label_policy: Default::default(),
        receipt_context: None,
        default_observation_to_ingestion: false,
        neighborhood_review: rich.then(String::new),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: "task:review".into(),
                kind: "task".into(),
                title: None,
                metadata: Default::default(),
            }],
            entries,
            relations,
            evidence: vec![],
        },
    }
}

#[tokio::test]
async fn a_peer_commit_after_the_revision_read_invalidates_the_review() {
    stale_review(false, false, false).await;
}

#[tokio::test]
async fn a_foreign_peer_commit_invalidates_the_review() {
    stale_review(true, false, false).await;
}

#[tokio::test]
async fn a_commit_during_neighborhood_loading_refuses_a_mixed_view() {
    stale_review(false, true, false).await;
}

#[tokio::test]
async fn a_separate_process_commit_invalidates_the_review() {
    stale_review(false, false, true).await;
}

async fn stale_review(foreign: bool, during_read: bool, process: bool) {
    let directory = tempfile::tempdir().expect("directory");
    let store = EmbeddedKernelStore::open(directory.path()).expect("first engine");
    let peer_store = EmbeddedKernelStore::open(directory.path()).expect("second engine");
    let events = Arc::new(PausedEvents {
        store: store.clone(),
        pause_on: AtomicUsize::new(0),
        paused: Notify::new(),
        resume: Notify::new(),
    });
    let writer = Arc::new(service(store.clone(), events.clone()));
    let peer = service(peer_store.clone(), Arc::new(peer_store));
    let mut peer_command = command("peer", false);
    let mut proposal = command("reviewed", true);
    if foreign {
        peer_command.about = "project:remote-review".into();
        peer_command.memory.entries[0].id = "project:remote-review:entry:observation:peer".into();
        peer.ingest(peer_command.clone())
            .await
            .expect("foreign seed");
        proposal.memory.relations[0].target_ref = peer_command.memory.entries[0].id.clone();
        proposal.memory.relations[0].rel = "same_entity_as".into();
        proposal.memory.relations[0].method =
            Some(format!("{}test", kmp_domain::DECLARED_FROM_RELATE_METHOD));
        peer_command.idempotency_key = "peer-change".into();
        peer_command.memory.entries[0].text = "Changed foreign observation".into();
    }
    let preview = writer.ingest(proposal.clone()).await.expect("review");
    assert!(!preview.read_after_write_ready);
    proposal.neighborhood_review = Some(preview.neighborhood.expect("pending context").token);

    // Read before/after the neighborhood, then the final pre-append revision check.
    events.pause_on.store(
        if during_read {
            1
        } else if foreign {
            6
        } else {
            3
        },
        Ordering::SeqCst,
    );
    let task = tokio::spawn(async move { writer.ingest(proposal).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), events.paused.notified())
        .await
        .expect("writer reaches final revision check");
    if process {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_embedded_crash_writer"))
            .arg(directory.path())
            .args(["1", ABOUT])
            .env("KMP_TEST_ATOMIC_PROJECTION", "1")
            .output()
            .expect("separate writer process");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else {
        peer.ingest(peer_command)
            .await
            .expect("independent peer commits");
    }
    events.resume.notify_one();
    let result = task.await.expect("writer task");
    assert!(
        result.is_err()
            || !result
                .as_ref()
                .expect("checked result")
                .read_after_write_ready,
        "a stale neighborhood must not commit: {result:?}"
    );
    assert!(
        store
            .find_by_idempotency_key("reviewed")
            .await
            .expect("receipt lookup")
            .is_none()
    );
}
