#[allow(dead_code)]
#[path = "support/proof_snapshot.rs"]
mod support;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::TemporalIncludeOptions;
use kmp_domain::{TemporalCursor, TemporalDirection};
use support::{Reads, command, service, temporal_query};

#[tokio::test]
async fn temporal_reads_only_selected_bodies_and_their_sources() {
    let dir = tempfile::tempdir().expect("isolated store");
    let reads = Reads::new(EmbeddedKernelStore::open(dir.path()).expect("store"));
    let app = service(reads.clone(), true);
    let seed = large_fixture();
    app.ingest(seed).await.expect("seed independent proofs");
    let mut query = temporal_query();
    query.direction = TemporalDirection::Forward;
    query.cursor = Some(TemporalCursor::Time("2026-09-01T00:00:00Z".into()));
    query.entry_selection = None;
    query.limit_entries = Some(1);
    query.include = TemporalIncludeOptions {
        evidence: true,
        relations: true,
        raw_refs: true,
        dependencies: false,
    };
    reads.detail_ids.lock().expect("reads").clear();
    let result = app.temporal(query.clone()).await.expect("selected page");
    let ids = reads.detail_ids.lock().expect("reads").clone();
    assert_eq!(result.traversal.entries().len(), 1);
    assert_eq!(result.traversal.page().total(), 64);
    assert_eq!(
        ids.len(),
        2,
        "one entry and its one canonical source: {ids:?}"
    );
    assert_eq!(result.source_bundle.node_details().len(), 2);
    assert!(
        result
            .source_bundle
            .node_details()
            .iter()
            .any(|body| body.detail().len() > 50_000)
    );

    query.include = TemporalIncludeOptions {
        evidence: false,
        relations: true,
        raw_refs: false,
        dependencies: false,
    };
    reads.detail_ids.lock().expect("reads").clear();
    let without = app
        .temporal(query.clone())
        .await
        .expect("entries and relations only");
    assert_eq!(without.traversal, result.traversal);
    assert!(reads.detail_ids.lock().expect("reads").is_empty());

    query.include.raw_refs = true;
    reads.detail_ids.lock().expect("reads").clear();
    let raw = app.temporal(query).await.expect("raw entry body");
    assert_eq!(raw.traversal, result.traversal);
    assert_eq!(reads.detail_ids.lock().expect("reads").len(), 1);
}

fn large_fixture() -> kmp_application::MemoryIngestCommand {
    let mut seed = command(1, "v1");
    let entry = seed.memory.entries[0].clone();
    let evidence = seed.memory.evidence[0].clone();
    seed.memory.entries.clear();
    seed.memory.evidence.clear();
    for n in 0..64 {
        let mut entry = entry.clone();
        entry.id = format!("{}-{n:03}", support::CLAIM);
        entry.coordinates[0].sequence = Some(n + 1);
        let mut evidence = evidence.clone();
        evidence.id = format!("{}-{n:03}", evidence.id);
        evidence.supports = vec![entry.id.clone()];
        evidence.text = "Canonical large source with exclusion and quantity 17. ".repeat(1024);
        seed.memory.entries.push(entry);
        seed.memory.evidence.push(evidence);
    }
    seed
}

#[tokio::test]
async fn body_reads_follow_bounded_dependencies_after_temporal_admission() {
    use kmp_application::MemoryRelationData;
    use kmp_application::memory::MemoryRelationClocks;
    let dir = tempfile::tempdir().expect("isolated store");
    let reads = Reads::new(EmbeddedKernelStore::open(dir.path()).expect("store"));
    let app = service(reads.clone(), true);
    let mut seed = large_fixture();
    let first = seed.memory.entries[0].id.clone();
    seed.memory.relations = seed
        .memory
        .entries
        .iter()
        .skip(1)
        .enumerate()
        .map(|(n, entry)| MemoryRelationData {
            source_ref: first.clone(),
            target_ref: entry.id.clone(),
            rel: "uses_background".into(),
            semantic_class: "evidential".into(),
            why: Some("The source records this observation as the reference count.".into()),
            evidence: Some("The two observations explicitly share reference count 17.".into()),
            confidence: Some("high".into()),
            clocks: Some(MemoryRelationClocks {
                observed_at: Some(
                    if n == 0 {
                        "2026-09-11T10:00:00Z"
                    } else {
                        "2026-09-10T10:00:00Z"
                    }
                    .into(),
                ),
                ..Default::default()
            }),
            sequence: None,
            motivation: None,
            method: None,
            decision_id: None,
            caused_by_node_id: None,
            coordinate: None,
        })
        .collect();
    app.ingest(seed)
        .await
        .expect("seed a dense dependency group");
    let mut query = temporal_query();
    query.cursor = Some(TemporalCursor::Time("2026-09-10T11:00:00Z".into()));
    query.entry_selection =
        Some(kmp_domain::TemporalEntrySelection::new(vec![first.clone()]).expect("seed selector"));
    query.include.dependencies = true;
    reads.detail_ids.lock().expect("reads").clear();
    let result = app.temporal(query).await.expect("dependency page");
    let plan =
        kmp_domain::TemporalProofPlan::select(&result.source_bundle, &result.traversal, true)
            .expect("proof plan");
    let group = &plan.groups()[0];
    assert_eq!(group.member_refs().len(), 8);
    assert!(
        !group
            .member_refs()
            .contains(&format!("{}-001", support::CLAIM)),
        "future relation must not fill the member allowance"
    );
    assert!(group.omitted_by_limit() > 0);
    assert_eq!(
        reads.detail_ids.lock().expect("reads").len(),
        16,
        "eight entries and their eight sources, not the entire graph"
    );
}
