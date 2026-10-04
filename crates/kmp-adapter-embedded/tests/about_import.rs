//! `import_abouts`: exact abouts of another memory into a live store — all or
//! nothing, idempotent, and never into a different history (#903).
//!
//! The projection derivation is injected, so these tests drive it directly:
//! a content hash `rel:<target>` derives a relation from the about's anchor
//! to `<target>`, which is how a dangling relation is produced on purpose.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use kmp_adapter_embedded::{AboutImportOutcome, EmbeddedKernelStore, verify_bundle};
use kmp_domain::{
    ContextEventStore, ContextUpdatedEvent, NodeProjection, NodeRelationProjection, PortError,
    ProjectionMutation, RelationExplanation, RelationSemanticClass,
};

fn event(about: &str, revision: u64, hash: &str) -> ContextUpdatedEvent {
    ContextUpdatedEvent {
        root_node_id: about.to_string(),
        role: "agent".to_string(),
        revision,
        content_hash: hash.to_string(),
        changes: Vec::new(),
        idempotency_key: Some(format!("write:{about}:{hash}")),
        logical_digest: None,
        requested_by: Some("about-import-test".to_string()),
        occurred_at: UNIX_EPOCH + Duration::from_secs(revision),
    }
}

fn node(node_id: &str) -> NodeProjection {
    NodeProjection {
        node_id: node_id.to_string(),
        node_kind: "memory_anchor".to_string(),
        title: node_id.to_string(),
        summary: node_id.to_string(),
        status: "ACTIVE".to_string(),
        labels: Vec::new(),
        properties: BTreeMap::new(),
        provenance: None,
    }
}

fn derive(event: &ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> {
    let mut mutations = vec![ProjectionMutation::EnsureNode(node(&event.root_node_id))];
    if let Some(target) = event.content_hash.strip_prefix("rel:") {
        mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
            NodeRelationProjection {
                source_node_id: event.root_node_id.clone(),
                target_node_id: target.to_string(),
                relation_type: "same_entity_as".to_string(),
                explanation: RelationExplanation::new(RelationSemanticClass::Evidential),
            },
        )));
    }
    Ok(mutations)
}

/// A store holding `writes` in order; revisions follow each about.
async fn store_with(dir: &Path, writes: &[(&str, &str)]) -> EmbeddedKernelStore {
    let store = EmbeddedKernelStore::open(dir).expect("store opens");
    for (about, hash) in writes {
        let head = store
            .current_revision(about, "agent")
            .await
            .expect("head reads");
        store
            .append(event(about, head + 1, hash), head)
            .await
            .expect("event appends");
        store
            .rebuild_projections(derive)
            .await
            .expect("projections rebuild");
    }
    store
}

fn abouts(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

/// What the destination holds, as identity: event count and digest.
async fn fingerprint(store: &EmbeddedKernelStore) -> (u64, String) {
    let header = verify_bundle(&store.export_bundle().await.expect("export")).expect("verifies");
    (header.event_count, header.content_digest)
}

#[tokio::test]
async fn an_absent_about_is_imported_and_a_second_run_changes_nothing() {
    let source_dir = tempfile::tempdir().expect("source");
    let source = store_with(
        source_dir.path(),
        &[
            ("project:a", "a1"),
            ("project:other", "o1"),
            ("project:a", "a2"),
        ],
    )
    .await;
    let bundle = source.export_bundle().await.expect("export");
    let target_dir = tempfile::tempdir().expect("target");
    let target = store_with(target_dir.path(), &[("project:here", "h1")]).await;

    let first = target
        .import_abouts(&bundle, &abouts(&["project:a"]), derive)
        .await
        .expect("absent about imports");
    assert_eq!(first.events_imported, 2);
    assert_eq!(
        first.abouts,
        BTreeMap::from([("project:a".to_string(), AboutImportOutcome::Imported)])
    );
    let after_first = fingerprint(&target).await;
    assert_eq!(after_first.0, 3, "only the requested about came in");
    assert_eq!(
        target
            .current_revision("project:a", "agent")
            .await
            .expect("test step"),
        2
    );
    assert_eq!(
        target
            .current_revision("project:other", "agent")
            .await
            .expect("test step"),
        0
    );

    let second = target
        .import_abouts(&bundle, &abouts(&["project:a"]), derive)
        .await
        .expect("repeat is a no-op");
    assert_eq!(second.events_imported, 0);
    assert_eq!(second.mutations_applied, 0);
    assert_eq!(
        second.abouts,
        BTreeMap::from([("project:a".to_string(), AboutImportOutcome::Unchanged)])
    );
    assert_eq!(fingerprint(&target).await, after_first);
}

#[tokio::test]
async fn an_exact_prefix_is_extended_with_only_the_missing_tail() {
    let source_dir = tempfile::tempdir().expect("source");
    let source = store_with(source_dir.path(), &[("project:a", "a1")]).await;
    let target_dir = tempfile::tempdir().expect("target");
    let target = store_with(target_dir.path(), &[]).await;
    target
        .import_abouts(
            &source.export_bundle().await.expect("test step"),
            &abouts(&["project:a"]),
            derive,
        )
        .await
        .expect("first import");
    // The source moves on after the destination took its copy.
    source
        .append(event("project:a", 2, "a2"), 1)
        .await
        .expect("source grows");
    source
        .append(event("project:a", 3, "a3"), 2)
        .await
        .expect("source grows");

    let report = target
        .import_abouts(
            &source.export_bundle().await.expect("test step"),
            &abouts(&["project:a"]),
            derive,
        )
        .await
        .expect("prefix extends");
    assert_eq!(report.events_imported, 2);
    assert_eq!(
        report.abouts["project:a"],
        AboutImportOutcome::Extended,
        "{report:?}"
    );
    assert_eq!(
        fingerprint(&target).await,
        fingerprint(&source).await,
        "the destination now holds the source's history exactly"
    );
}

#[tokio::test]
async fn a_diverging_history_is_refused_and_nothing_is_written() {
    let source_dir = tempfile::tempdir().expect("source");
    let source = store_with(
        source_dir.path(),
        &[
            ("project:a", "a1"),
            ("project:a", "a2"),
            ("project:b", "b1"),
        ],
    )
    .await;
    let target_dir = tempfile::tempdir().expect("target");
    let target = store_with(
        target_dir.path(),
        &[("project:a", "a1"), ("project:a", "x2")],
    )
    .await;
    let before = fingerprint(&target).await;

    // `project:b` alone would import; the whole request is still refused.
    let error = target
        .import_abouts(
            &source.export_bundle().await.expect("test step"),
            &abouts(&["project:a", "project:b"]),
            derive,
        )
        .await
        .expect_err("divergence is refused");
    let message = error.to_string();
    assert!(message.contains("`project:a`"), "{message}");
    assert!(message.contains("diverge at revision 2"), "{message}");
    assert!(message.contains("nothing was written"), "{message}");
    assert_eq!(fingerprint(&target).await, before);
    assert_eq!(
        target
            .current_revision("project:b", "agent")
            .await
            .expect("test step"),
        0
    );
}

#[tokio::test]
async fn a_destination_ahead_of_the_source_is_refused() {
    let source_dir = tempfile::tempdir().expect("source");
    let source = store_with(source_dir.path(), &[("project:a", "a1")]).await;
    let target_dir = tempfile::tempdir().expect("target");
    let target = store_with(
        target_dir.path(),
        &[("project:a", "a1"), ("project:a", "a2")],
    )
    .await;
    let before = fingerprint(&target).await;

    let error = target
        .import_abouts(
            &source.export_bundle().await.expect("test step"),
            &abouts(&["project:a"]),
            derive,
        )
        .await
        .expect_err("destination ahead is refused");
    assert!(error.to_string().contains("ahead at revision 2"), "{error}");
    assert_eq!(fingerprint(&target).await, before);
}

#[tokio::test]
async fn a_requested_about_the_source_lacks_is_refused_before_anything_is_written() {
    let source_dir = tempfile::tempdir().expect("source");
    let source = store_with(source_dir.path(), &[("project:a", "a1")]).await;
    let target_dir = tempfile::tempdir().expect("target");
    let target = store_with(target_dir.path(), &[]).await;

    let error = target
        .import_abouts(
            &source.export_bundle().await.expect("test step"),
            &abouts(&["project:a", "project:A"]),
            derive,
        )
        .await
        .expect_err("abouts match exactly; `project:A` is missing");
    assert!(error.to_string().contains("`project:A`"), "{error}");
    assert_eq!(fingerprint(&target).await.0, 0);
}

#[tokio::test]
async fn a_relation_to_a_node_nobody_holds_is_refused_until_its_owner_comes_too() {
    let source_dir = tempfile::tempdir().expect("source");
    let source = store_with(
        source_dir.path(),
        &[("project:b", "b1"), ("project:a", "rel:project:b")],
    )
    .await;
    let bundle = source.export_bundle().await.expect("test step");
    let target_dir = tempfile::tempdir().expect("target");
    let target = store_with(target_dir.path(), &[]).await;

    let error = target
        .import_abouts(&bundle, &abouts(&["project:a"]), derive)
        .await
        .expect_err("the relation would dangle");
    let message = error.to_string();
    assert!(message.contains("missing `project:b`"), "{message}");
    assert!(message.contains("--about"), "{message}");
    assert_eq!(fingerprint(&target).await.0, 0);

    let report = target
        .import_abouts(&bundle, &abouts(&["project:a", "project:b"]), derive)
        .await
        .expect("with its owner the relation resolves");
    assert_eq!(report.events_imported, 2);
}

#[tokio::test]
async fn a_relation_to_a_node_the_destination_already_holds_is_accepted() {
    let source_dir = tempfile::tempdir().expect("source");
    let source = store_with(source_dir.path(), &[("project:a", "rel:project:here")]).await;
    let target_dir = tempfile::tempdir().expect("target");
    let target = store_with(target_dir.path(), &[("project:here", "h1")]).await;

    let report = target
        .import_abouts(
            &source.export_bundle().await.expect("test step"),
            &abouts(&["project:a"]),
            derive,
        )
        .await
        .expect("the target already exists here");
    assert_eq!(report.abouts["project:a"], AboutImportOutcome::Imported);
}

#[tokio::test]
async fn an_idempotency_key_already_answering_here_is_refused() {
    let source_dir = tempfile::tempdir().expect("source");
    let source = store_with(source_dir.path(), &[("project:a", "same")]).await;
    let target_dir = tempfile::tempdir().expect("target");
    let target = EmbeddedKernelStore::open(target_dir.path()).expect("target opens");
    let mut clash = event("project:other", 1, "o1");
    clash.idempotency_key = Some("write:project:a:same".to_string());
    target.append(clash, 0).await.expect("clashing write");
    let before = fingerprint(&target).await;

    let error = target
        .import_abouts(
            &source.export_bundle().await.expect("test step"),
            &abouts(&["project:a"]),
            derive,
        )
        .await
        .expect_err("the key answers for another write");
    assert!(error.to_string().contains("idempotency key"), "{error}");
    assert_eq!(fingerprint(&target).await, before);
}

/// Every file under `dir` with its bytes, except SQLite's shared-memory
/// index, which any reader may touch.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("readable dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if !path.to_string_lossy().ends_with("-shm") {
                let name = path
                    .strip_prefix(dir)
                    .expect("test step")
                    .display()
                    .to_string();
                files.insert(name, std::fs::read(&path).expect("readable file"));
            }
        }
    }
    files
}

#[tokio::test]
async fn a_source_store_is_read_without_being_changed() {
    let source_dir = tempfile::tempdir().expect("source");
    {
        store_with(
            source_dir.path(),
            &[("project:a", "a1"), ("project:a", "a2")],
        )
        .await;
    }
    let before = snapshot(source_dir.path());

    let source = EmbeddedKernelStore::open_read_only(source_dir.path()).expect("opens read-only");
    let bundle = source
        .export_bundle_for_abouts(&abouts(&["project:a"]))
        .await
        .expect("filtered export");
    let refused = source.append(event("project:a", 3, "a3"), 2).await;
    assert!(refused.is_err(), "a read-only source refuses writes");
    drop(source);

    let target_dir = tempfile::tempdir().expect("target");
    let target = store_with(target_dir.path(), &[]).await;
    target
        .import_abouts(&bundle, &abouts(&["project:a"]), derive)
        .await
        .expect("imports");

    let after = snapshot(source_dir.path());
    for (name, bytes) in &before {
        assert_eq!(after.get(name), Some(bytes), "`{name}` changed");
    }
    for name in after.keys().filter(|name| !before.contains_key(*name)) {
        assert!(
            name.ends_with("kernel.sqlite3-wal"),
            "reading the source created `{name}`"
        );
        assert!(after[name].is_empty(), "`{name}` holds data");
    }
}

#[test]
fn a_directory_that_is_not_a_store_is_refused_and_not_created() {
    let parent = tempfile::tempdir().expect("parent");
    let missing = parent.path().join("nowhere");
    let error = EmbeddedKernelStore::open_read_only(&missing).expect_err("no store");
    assert!(error.to_string().contains("not a KMP store"), "{error}");
    assert!(!missing.exists());
    let empty = tempfile::tempdir().expect("empty");
    assert!(EmbeddedKernelStore::open_read_only(empty.path()).is_err());
    assert_eq!(
        std::fs::read_dir(empty.path()).expect("test step").count(),
        0
    );
}
