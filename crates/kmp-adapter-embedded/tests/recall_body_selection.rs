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
