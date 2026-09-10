use crate::commands::{CommandApplicationService, UpdateContextCommand, UpdateContextUseCase};
use kmp_domain::ContextEventStore;
use kmp_testkit::InMemoryContextEventStore;
use std::collections::BTreeMap;
use std::sync::Arc;

fn command(about: &str, key: &str) -> UpdateContextCommand {
    UpdateContextCommand {
        root_node_id: about.into(),
        role: "memory".into(),
        work_item_id: key.into(),
        changes: Vec::new(),
        expected_revision: None,
        expected_content_hash: None,
        idempotency_key: Some(key.into()),
        logical_digest: Some(key.into()),
        requested_by: None,
    }
}

#[tokio::test]
async fn review_read_excludes_append_and_projection_until_released() {
    let store = Arc::new(InMemoryContextEventStore::new());
    let service = Arc::new(CommandApplicationService::new(Arc::new(
        UpdateContextUseCase::new(store.clone(), "test"),
    )));
    let guard = service.projection_read().await;
    let writer = service.clone();
    let task =
        tokio::spawn(async move { writer.update_context(command("project:a", "write")).await });
    tokio::task::yield_now().await;
    assert_eq!(
        store
            .current_revision("project:a", "memory")
            .await
            .expect("command revision fixture"),
        0
    );
    assert!(!task.is_finished());
    drop(guard);
    task.await
        .expect("command revision fixture")
        .expect("command revision fixture");
    assert_eq!(
        store
            .current_revision("project:a", "memory")
            .await
            .expect("command revision fixture"),
        1
    );
}

#[tokio::test]
async fn foreign_revision_is_checked_at_commit_but_accepted_retries_stay_replays() {
    let store = Arc::new(InMemoryContextEventStore::new());
    let service =
        CommandApplicationService::new(Arc::new(UpdateContextUseCase::new(store.clone(), "test")));
    let old = BTreeMap::from([("project:a".into(), 0), ("project:b".into(), 0)]);
    service
        .update_context(command("project:b", "foreign"))
        .await
        .expect("command revision fixture");
    let error = service
        .update_context_after_read(command("project:a", "ours"), &old)
        .await
        .expect_err("stale context must refuse the commit");
    assert!(matches!(
        error,
        crate::ApplicationError::RetryableConflict(_)
    ));
    assert_eq!(
        store
            .current_revision("project:a", "memory")
            .await
            .expect("command revision fixture"),
        0
    );
    let current = BTreeMap::from([("project:a".into(), 0), ("project:b".into(), 1)]);
    assert!(
        !service
            .update_context_after_read(command("project:a", "ours"), &current)
            .await
            .expect("command revision fixture")
            .replayed
    );
    assert!(
        service
            .update_context_after_read(command("project:a", "ours"), &old)
            .await
            .expect("command revision fixture")
            .replayed
    );
    assert_eq!(
        store
            .current_revision("project:a", "memory")
            .await
            .expect("command revision fixture"),
        1
    );
}
