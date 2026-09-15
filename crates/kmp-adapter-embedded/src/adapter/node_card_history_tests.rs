use super::*;
use kmp_application::projection_mutations_for_context_event as derive;
use kmp_domain::{ContextEventChange, ContextEventStore, ContextUpdatedEvent, NodeCardEvent};

async fn event_seeded() -> (tempfile::TempDir, EmbeddedKernelStore, AuthorNodeCard) {
    let dir = tempfile::tempdir().expect("directory");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let event = ContextUpdatedEvent {
        root_node_id: ABOUT.into(),
        role: "memory".into(),
        revision: 1,
        content_hash: "source-event".into(),
        changes: vec![ContextEventChange {
            operation: "UPSERT".into(),
            entity_kind: "memory_entry".into(),
            entity_id: NODE.into(),
            payload_json: serde_json::json!({"id":NODE,"kind":"observation","text":BODY})
                .to_string(),
            reason: None,
            scopes: Vec::new(),
        }],
        idempotency_key: Some("source-seed".into()),
        logical_digest: None,
        requested_by: Some("source-author".into()),
        occurred_at: std::time::UNIX_EPOCH,
    };
    let mutations = derive(&event).expect("derive source");
    store
        .append_projected(event, 0, Vec::new(), mutations)
        .await
        .expect("source transaction");
    let digest = stored_digest(&store, NODE).await;
    let mut write = command(1, &digest, NodeCardExpectation::Absent);
    write.source_content_hash = store
        .load_node_detail(NODE)
        .await
        .expect("valid card history fixture")
        .expect("valid card history fixture")
        .content_hash;
    (dir, store, write)
}

fn cards(store: &EmbeddedKernelStore, cut: Option<i128>) -> Vec<Option<NodeCard>> {
    let tx = store.begin_read().expect("snapshot");
    read_batch_at(
        tx.as_ref(),
        &[NODE.into(), NODE.into(), "missing".into()],
        "es",
        cut,
    )
    .expect("cards")
}

#[tokio::test]
async fn card_versions_survive_rebuild_reopen_and_filtered_bundle_roundtrip() {
    let (dir, store, mut write) = event_seeded().await;
    let original = store
        .load_node_detail(NODE)
        .await
        .expect("valid card history fixture");
    let source_event = store.read_event_log().expect("valid card history fixture")[0].clone();
    let first = store
        .author_node_card(write.clone())
        .await
        .expect("valid card history fixture")
        .expect("valid card history fixture");
    write.expect = NodeCardExpectation::CardRevision(1);
    write.text = "Segunda lectura.".into();
    write.authored_at = "2026-09-13T09:00:00Z".into();
    let second = store
        .author_node_card(write.clone())
        .await
        .expect("valid card history fixture")
        .expect("valid card history fixture");
    let events = store.read_event_log().expect("valid card history fixture");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0], source_event);
    assert_eq!(
        NodeCardEvent::card(&events[1]).expect("valid card history fixture"),
        Some(first.clone())
    );
    assert_eq!(
        NodeCardEvent::card(&events[2]).expect("valid card history fixture"),
        Some(second.clone())
    );
    assert_eq!(
        store
            .current_revision(ABOUT, "memory")
            .await
            .expect("valid card history fixture"),
        1
    );
    assert_eq!(
        store
            .current_revision(ABOUT, NodeCardEvent::ROLE)
            .await
            .expect("valid card history fixture"),
        2
    );
    let cut = kmp_domain::temporal_instant_nanos("2026-09-12T12:00:00Z");
    let expected = vec![Some(first.clone()), Some(first), None];
    assert_eq!(cards(&store, cut), expected);
    assert_eq!(cards(&store, None)[0], Some(second.clone()));
    // Failed CAS is not an event and cannot advance either stream.
    assert!(
        store
            .author_node_card(write)
            .await
            .expect("valid card history fixture")
            .is_err()
    );
    assert_eq!(
        store.read_event_log().expect("valid card history fixture"),
        events
    );
    let bundle = store
        .export_bundle_for_abouts(&[ABOUT.into()])
        .await
        .expect("valid card history fixture");
    assert_eq!(
        crate::verify_bundle(&bundle)
            .expect("valid card history fixture")
            .event_format,
        3
    );
    store
        .rebuild_projections(derive)
        .await
        .expect("valid card history fixture");
    assert_eq!(cards(&store, cut), expected);
    assert_eq!(
        store
            .load_node_detail(NODE)
            .await
            .expect("valid card history fixture"),
        original
    );
    drop(store);
    let reopened = EmbeddedKernelStore::open(dir.path()).expect("valid card history fixture");
    assert_eq!(
        reopened
            .read_event_log()
            .expect("valid card history fixture"),
        events,
        "reopening adds nothing"
    );
    assert_eq!(cards(&reopened, cut), expected);
    let target = tempfile::tempdir().expect("valid card history fixture");
    let restored = EmbeddedKernelStore::open(target.path()).expect("valid card history fixture");
    restored
        .import_bundle(&bundle, derive)
        .await
        .expect("valid card history fixture");
    assert_eq!(
        restored
            .read_event_log()
            .expect("valid card history fixture"),
        events
    );
    assert_eq!(cards(&restored, cut), expected);
    assert_eq!(cards(&restored, None)[0], Some(second));
    assert_eq!(
        restored
            .load_node_detail(NODE)
            .await
            .expect("valid card history fixture"),
        original
    );
    assert_eq!(
        restored
            .export_bundle()
            .await
            .expect("valid card history fixture"),
        bundle
    );
}

#[tokio::test]
async fn legacy_current_card_is_adopted_once_without_inventing_older_revisions() {
    let (dir, store, write) = event_seeded().await;
    let mut legacy = node_card_policy::admit(
        &write,
        Some(&match node() {
            ProjectionMutation::UpsertNode(n) => n,
            _ => unreachable!(),
        }),
        super::super::super::node_body_descriptor::read_one(
            store
                .begin_read()
                .expect("valid card history fixture")
                .as_ref(),
            NODE,
        )
        .expect("valid card history fixture")
        .as_ref(),
        None,
        None,
    )
    .expect("valid card history fixture");
    legacy.card_revision = 7;
    let mut tx = store.begin_write().expect("valid card history fixture");
    tx.remove(
        Table::Migrations,
        Key::Str(crate::adapter::node_card_adoption::MARKER),
    )
    .expect("valid card history fixture");
    tx.insert(
        Table::Cards,
        Key::Str2(NODE, "es"),
        &encode("legacy card", &CardRecord::from(legacy.clone()))
            .expect("valid card history fixture"),
    )
    .expect("valid card history fixture");
    tx.commit().expect("valid card history fixture");
    drop(store);
    std::fs::write(dir.path().join("FORMAT_VERSION"), "3\n").expect("valid card history fixture");
    let store = EmbeddedKernelStore::open(dir.path()).expect("valid card history fixture");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("FORMAT_VERSION"))
            .expect("valid card history fixture"),
        "4\n"
    );
    let log = store.read_event_log().expect("valid card history fixture");
    assert_eq!(log.len(), 2);
    assert_eq!(log[1].changes[0].operation, "BASELINE");
    assert_eq!(
        NodeCardEvent::card(&log[1]).expect("valid card history fixture"),
        Some(legacy.clone())
    );
    assert_eq!(
        store
            .begin_read()
            .expect("valid card history fixture")
            .count(Table::CardVersions)
            .expect("valid card history fixture"),
        1
    );
    store
        .rebuild_projections(derive)
        .await
        .expect("valid card history fixture");
    assert_eq!(cards(&store, None)[0], Some(legacy));
    drop(store);
    assert_eq!(
        EmbeddedKernelStore::open(dir.path())
            .expect("valid card history fixture")
            .read_event_log()
            .expect("valid card history fixture"),
        log
    );
}

#[tokio::test]
async fn refused_source_and_failed_projection_commit_no_event() {
    let (_dir, store, mut write) = event_seeded().await;
    let before = store
        .export_bundle()
        .await
        .expect("valid card history fixture");
    write.source_record_digest = "wrong".into();
    assert!(
        store
            .author_node_card(write.clone())
            .await
            .expect("valid card history fixture")
            .is_err()
    );
    assert_eq!(
        store
            .export_bundle()
            .await
            .expect("valid card history fixture"),
        before
    );
    write.source_record_digest = stored_digest(&store, NODE).await;
    // Simulate a conflicting immutable slot after validation. The appended
    // event must roll back with the projection error in the same transaction.
    let mut collision = cards(&store, None);
    assert!(collision.remove(0).is_none());
    let key = history_key(
        kmp_domain::temporal_instant_nanos(&write.authored_at).expect("valid card history fixture"),
        1,
    );
    let mut tx = store.begin_write().expect("valid card history fixture");
    tx.insert(
        Table::CardVersions,
        Key::Str3(NODE, "es", &key),
        b"conflict",
    )
    .expect("valid card history fixture");
    tx.commit().expect("valid card history fixture");
    assert!(store.author_node_card(write).await.is_err());
    assert_eq!(
        store
            .export_bundle()
            .await
            .expect("valid card history fixture"),
        before
    );
    assert_eq!(cards(&store, None)[0], None);
}

#[tokio::test]
async fn concurrent_authors_commit_one_card_and_one_event() {
    let (_dir, store, write) = event_seeded().await;
    let (left, right) = tokio::join!(
        store.author_node_card(write.clone()),
        store.author_node_card(write)
    );
    let successes = [
        left.expect("valid card history fixture"),
        right.expect("valid card history fixture"),
    ]
    .iter()
    .filter(|r| r.is_ok())
    .count();
    assert_eq!(successes, 1);
    assert_eq!(
        store
            .read_event_log()
            .expect("valid card history fixture")
            .len(),
        2
    );
    assert_eq!(
        store
            .begin_read()
            .expect("valid card history fixture")
            .count(Table::CardVersions)
            .expect("valid card history fixture"),
        1
    );
}

#[tokio::test]
async fn malformed_card_payload_is_rejected_before_importing_any_events() {
    let (_dir, store, write) = event_seeded().await;
    store
        .author_node_card(write)
        .await
        .expect("valid card history fixture")
        .expect("valid card history fixture");
    // Use the event port to represent a corrupt producer, but export through
    // the ordinary encoder so the outer bundle checksum remains valid.
    let mut event = store.read_event_log().expect("valid card history fixture")[1].clone();
    event.changes[0].payload_json = "{}".into();
    store
        .append(event, 1)
        .await
        .expect("valid card history fixture");
    let bundle = store
        .export_bundle()
        .await
        .expect("valid card history fixture");
    let dir = tempfile::tempdir().expect("valid card history fixture");
    let restored = EmbeddedKernelStore::open(dir.path()).expect("valid card history fixture");
    assert!(restored.import_bundle(&bundle, derive).await.is_err());
    assert_eq!(
        restored
            .event_log_stats()
            .await
            .expect("valid card history fixture")
            .0,
        0
    );
    assert_eq!(
        restored
            .begin_read()
            .expect("valid card history fixture")
            .count(Table::Cards)
            .expect("valid card history fixture"),
        0
    );
}

#[tokio::test]
async fn a_failed_rebuild_keeps_the_previous_cards_and_source() {
    let (_dir, store, write) = event_seeded().await;
    store
        .author_node_card(write)
        .await
        .expect("store")
        .expect("card");
    let before = cards(&store, None);
    let source = store.load_node_detail(NODE).await.expect("source");
    let result = store
        .rebuild_projections(|event| {
            if event.role == NodeCardEvent::ROLE {
                return Err(PortError::Unavailable(
                    "injected replay interruption".into(),
                ));
            }
            derive(event)
        })
        .await;
    assert!(result.is_err());
    assert_eq!(cards(&store, None), before);
    assert_eq!(store.load_node_detail(NODE).await.expect("source"), source);
}
