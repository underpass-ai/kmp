#[allow(dead_code)]
#[path = "support/proof_snapshot.rs"]
mod support;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::{AskMemoryQuery, MemoryDimensionData, WakeMemoryQuery};
use kmp_domain::{DimensionSelection, LabelSelector, LabelSelectorOperator, TemporalSelection};
use support::{Reads, command, service};

#[tokio::test]
async fn recall_admits_whole_entry_labels_before_loading_complete_candidate_bodies() {
    let dir = tempfile::tempdir().expect("fixture operation succeeds");
    let reads =
        Reads::new(EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds"));
    let app = service(reads.clone(), true);
    let mut seed = command(2, "v1");
    for (i, env) in ["env:prod", "env:test"].into_iter().enumerate() {
        seed.memory.dimensions.push(MemoryDimensionData {
            id: env.into(),
            kind: "env".into(),
            title: None,
            metadata: Default::default(),
        });
        let mut coordinate = seed.memory.entries[i].coordinates[0].clone();
        coordinate.dimension = "env".into();
        coordinate.scope_id = env.into();
        seed.memory.entries[i].coordinates.push(coordinate);
        seed.memory.evidence[i].supports = vec![seed.memory.entries[i].id.clone()];
        seed.memory.evidence[i].text =
            format!("{} tail-marker-{i}", "unabridged source 123 ".repeat(4096));
    }
    let included = seed.memory.entries[0].id.clone();
    let excluded = seed.memory.entries[1].id.clone();
    let excluded_source = seed.memory.evidence[1].id.clone();
    let source_text = seed.memory.evidence[0].text.clone();
    app.ingest(seed).await.expect("fixture operation succeeds");
    let dimensions = DimensionSelection::only(["task"]).with_selectors([LabelSelector::new(
        "env",
        LabelSelectorOperator::In,
        ["env:prod"],
    )
    .expect("fixture operation succeeds")]);
    reads
        .detail_ids
        .lock()
        .expect("fixture operation succeeds")
        .clear();
    let wake = app
        .wake(WakeMemoryQuery {
            about: support::ABOUT.into(),
            role: "answerer".into(),
            intent: "resume".into(),
            dimensions: dimensions.clone(),
            token_budget: 1600,
            depth: 8,
            max_tier: None,
            max_entries: None,
            temporal: TemporalSelection::Frontier,
        })
        .await
        .expect("fixture operation succeeds");
    let read_ids = reads
        .detail_ids
        .lock()
        .expect("fixture operation succeeds")
        .clone();
    assert!(read_ids.contains(&included));
    assert!(!read_ids.contains(&excluded));
    assert!(!read_ids.contains(&excluded_source));
    assert!(
        wake.bundle
            .node_details()
            .iter()
            .any(|detail| detail.detail() == source_text)
    );
    assert_eq!(
        wake.timing
            .as_ref()
            .expect("fixture operation succeeds")
            .batch_size,
        read_ids.len()
    );
    reads
        .detail_ids
        .lock()
        .expect("fixture operation succeeds")
        .clear();
    let ask = app
        .ask(AskMemoryQuery {
            about: support::ABOUT.into(),
            question: "source 123".into(),
            asked_as: None,
            answer_policy: kmp_application::MemoryAnswerPolicy::default(),
            dimensions,
            token_budget: 1600,
            depth: 8,
            max_tier: None,
            max_entries: None,
            temporal: TemporalSelection::Frontier,
        })
        .await
        .expect("fixture operation succeeds");
    assert_eq!(ask.bundle, wake.bundle);
    assert_eq!(
        *reads.detail_ids.lock().expect("fixture operation succeeds"),
        read_ids
    );
}

#[tokio::test]
async fn ask_revision_belongs_to_its_pinned_bodies_after_a_peer_write() {
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    std::fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tmp"))
        .expect("scratch directory");
    let dir = tempfile::Builder::new()
        .prefix("ask-revision-")
        .tempdir_in(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tmp"))
        .expect("scratch store");
    let mut reads = Reads::new(EmbeddedKernelStore::open(dir.path()).expect("store"));
    reads.cache_revisions = true;
    let app = service(reads.clone(), true);
    app.ingest(command(2, "v1")).await.expect("v1");
    let query = AskMemoryQuery {
        about: support::ABOUT.into(),
        question: "recorded fact".into(),
        asked_as: None,
        answer_policy: kmp_application::MemoryAnswerPolicy::default(),
        dimensions: DimensionSelection::all(),
        token_budget: 1600,
        depth: 8,
        max_tier: None,
        max_entries: None,
        temporal: TemporalSelection::Frontier,
    };
    let before = app.ask(query.clone()).await.expect("before");
    assert!(before.read_revision.is_some());
    reads.pause.store(true, Ordering::SeqCst);
    let task_app = app.clone();
    let task_query = query.clone();
    let pending = tokio::spawn(async move { task_app.ask(task_query).await.expect("paused Ask") });
    tokio::time::timeout(Duration::from_secs(5), reads.paused.notified())
        .await
        .expect("paused after real catalogue");
    let peer = service(
        Reads::new(EmbeddedKernelStore::open(dir.path()).expect("peer")),
        true,
    );
    peer.ingest(command(2, "v2")).await.expect("peer v2");
    reads.resume.notify_one();
    let during = pending.await.expect("resumed");
    let after = app.ask(query.clone()).await.expect("after");
    assert_eq!(before.bundle, during.bundle);
    assert_eq!(before.read_revision, during.read_revision);
    assert_ne!(after.bundle, before.bundle);
    assert_ne!(after.read_revision, before.read_revision);
    assert_eq!(
        after.read_revision,
        app.ask(query.clone())
            .await
            .expect("warm read")
            .read_revision
    );
    // Ports which cannot attest to a snapshot must not produce a cache key.
    let unsupported = service(
        Reads::new(EmbeddedKernelStore::open(dir.path()).expect("unkeyed")),
        true,
    );
    assert!(
        unsupported
            .ask(query)
            .await
            .expect("unkeyed read")
            .read_revision
            .is_none()
    );
}
