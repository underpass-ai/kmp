//! `fast_forward_to_bundle_blocking`: a store behind its committed bundle is
//! caught up in the bundle's own order; anything else is left untouched.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use kmp_adapter_embedded::{EmbeddedKernelStore, HistoryRelation, compare_bundle_histories};
use kmp_domain::{
    ContextEventStore, ContextUpdatedEvent, NodeProjection, PortError, ProjectionMutation,
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
        requested_by: Some("fast-forward-test".to_string()),
        occurred_at: UNIX_EPOCH + Duration::from_secs(revision),
    }
}

fn derive(event: &ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> {
    Ok(vec![ProjectionMutation::EnsureNode(NodeProjection {
        node_id: event.root_node_id.clone(),
        node_kind: "memory_anchor".to_string(),
        title: event.root_node_id.clone(),
        summary: event.root_node_id.clone(),
        status: "ACTIVE".to_string(),
        labels: Vec::new(),
        properties: BTreeMap::new(),
        provenance: None,
    })])
}

async fn write(store: &EmbeddedKernelStore, about: &str, hash: &str) {
    let head = store
        .current_revision(about, "agent")
        .await
        .expect("head reads");
    store
        .append(event(about, head + 1, hash), head)
        .await
        .expect("event appends");
}

async fn store_with(dir: &Path, writes: &[(&str, &str)]) -> EmbeddedKernelStore {
    let store = EmbeddedKernelStore::open(dir).expect("store opens");
    for (about, hash) in writes {
        write(&store, about, hash).await;
    }
    store
        .rebuild_projections(derive)
        .await
        .expect("projections rebuild");
    store
}

async fn relation(store: &EmbeddedKernelStore, bundle: &str) -> HistoryRelation {
    compare_bundle_histories(&store.export_bundle().await.expect("export"), bundle)
        .expect("compare")
}

#[tokio::test]
async fn an_empty_store_takes_the_whole_bundle() {
    let teammate = tempfile::tempdir().expect("teammate");
    let clone = tempfile::tempdir().expect("clone");
    let theirs = store_with(teammate.path(), &[("project:a", "a1"), ("project:b", "b1")]).await;
    let bundle = theirs.export_bundle().await.expect("export");
    let mine = store_with(clone.path(), &[]).await;

    let found = mine
        .fast_forward_to_bundle_blocking(&bundle, &[], derive)
        .expect("fast-forward");

    assert_eq!(
        found,
        HistoryRelation::BundleAhead {
            store_events: 0,
            bundle_events: 2
        }
    );
    assert_eq!(
        relation(&mine, &bundle).await,
        HistoryRelation::Equal { events: 2 }
    );
}

/// The risk the report named: an about import run once per about appends a
/// multi-about tail grouped by about, which is a different history from the
/// bundle's. The fast-forward appends in the bundle's order and stays equal.
#[tokio::test]
async fn a_multi_about_tail_keeps_the_bundle_order_where_per_about_import_does_not() {
    let teammate = tempfile::tempdir().expect("teammate");
    let theirs = store_with(teammate.path(), &[("project:a", "a1"), ("project:b", "b1")]).await;
    let base = theirs.export_bundle().await.expect("export");
    write(&theirs, "project:a", "a2").await;
    write(&theirs, "project:b", "b2").await;
    write(&theirs, "project:a", "a3").await;
    let pulled = theirs.export_bundle().await.expect("export");

    let per_about_dir = tempfile::tempdir().expect("per about");
    let per_about = store_with(per_about_dir.path(), &[]).await;
    per_about
        .fast_forward_to_bundle_blocking(&base, &[], derive)
        .expect("base");
    for about in ["project:a", "project:b"] {
        per_about
            .import_abouts(&pulled, &[about.to_string()], derive)
            .await
            .expect("about import");
    }
    assert!(
        matches!(
            relation(&per_about, &pulled).await,
            HistoryRelation::Diverged { position: 4, .. }
        ),
        "{:?}",
        relation(&per_about, &pulled).await
    );

    let ff_dir = tempfile::tempdir().expect("fast forward");
    let mine = store_with(ff_dir.path(), &[]).await;
    mine.fast_forward_to_bundle_blocking(&base, &[], derive)
        .expect("base");
    let found = mine
        .fast_forward_to_bundle_blocking(&pulled, &[], derive)
        .expect("fast-forward");
    assert_eq!(
        found,
        HistoryRelation::BundleAhead {
            store_events: 2,
            bundle_events: 5
        }
    );
    assert_eq!(
        relation(&mine, &pulled).await,
        HistoryRelation::Equal { events: 5 }
    );
}

#[tokio::test]
async fn excluded_abouts_in_the_store_do_not_block_the_catch_up() {
    let teammate = tempfile::tempdir().expect("teammate");
    let theirs = store_with(teammate.path(), &[("project:a", "a1"), ("project:a", "a2")]).await;
    let bundle = theirs.export_bundle().await.expect("export");
    let clone = tempfile::tempdir().expect("clone");
    let mine = store_with(clone.path(), &[("guide:kmp", "g1"), ("project:a", "a1")]).await;
    let excluded = vec!["guide:kmp".to_string()];

    let found = mine
        .fast_forward_to_bundle_blocking(&bundle, &excluded, derive)
        .expect("fast-forward");

    assert_eq!(
        found,
        HistoryRelation::BundleAhead {
            store_events: 1,
            bundle_events: 2
        }
    );
    let authored = mine
        .export_bundle_excluding_abouts(&excluded)
        .await
        .expect("export");
    assert_eq!(
        compare_bundle_histories(&authored, &bundle).expect("compare"),
        HistoryRelation::Equal { events: 2 }
    );
}

#[tokio::test]
async fn a_divergent_or_ahead_store_is_left_exactly_as_it_was() {
    let teammate = tempfile::tempdir().expect("teammate");
    let theirs = store_with(teammate.path(), &[("project:a", "theirs")]).await;
    let bundle = theirs.export_bundle().await.expect("export");
    let mine_dir = tempfile::tempdir().expect("mine");
    let mine = store_with(mine_dir.path(), &[("project:a", "mine")]).await;
    let before = mine.export_bundle().await.expect("export");

    let found = mine
        .fast_forward_to_bundle_blocking(&bundle, &[], derive)
        .expect("compare");
    assert!(matches!(
        found,
        HistoryRelation::Diverged { position: 1, .. }
    ));
    assert_eq!(
        relation(&mine, &before).await,
        HistoryRelation::Equal { events: 1 }
    );

    let ahead_dir = tempfile::tempdir().expect("ahead");
    let ahead = store_with(
        ahead_dir.path(),
        &[("project:a", "theirs"), ("project:a", "next")],
    )
    .await;
    let found = ahead
        .fast_forward_to_bundle_blocking(&bundle, &[], derive)
        .expect("compare");
    assert_eq!(
        found,
        HistoryRelation::StoreAhead {
            store_events: 2,
            bundle_events: 1
        }
    );
}
