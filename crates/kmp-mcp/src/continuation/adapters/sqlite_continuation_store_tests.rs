use super::*;
use serde_json::json;

fn read(page: usize) -> ReadContinuation {
    ReadContinuation::new(
        "kmp_wake",
        json!({"about":"project:a","page":{"cursor":format!("page-{page}")}}),
    )
    .expect("read")
}

#[test]
fn retained_calls_survive_reconnect_deduplicate_and_expire() {
    let dir = tempfile::tempdir().expect("store");
    let path = dir.path().join("continuations.sqlite3");
    let store = SqliteContinuationStore::at(&path).expect("store");
    let call = read(1);
    let id = store.save(&call).expect("save");
    assert_eq!(store.save(&call).expect("replay"), id);
    assert_ne!(store.save(&read(2)).expect("other"), id);
    // Another process over the same store resolves the same handle.
    drop(store);
    let store = SqliteContinuationStore::at(&path).expect("reconnect");
    assert_eq!(store.load(&id).expect("read"), Some(call.clone()));
    store
        .connection
        .lock()
        .expect("connection")
        .execute("UPDATE open_continuations SET expires_at=unixepoch()-1", [])
        .expect("expire");
    assert_eq!(store.load(&id).expect("expired"), None);
    assert_ne!(store.save(&call).expect("new handle"), id);
}

#[test]
fn retention_is_bounded_and_oversize_calls_are_refused() {
    let store = SqliteContinuationStore::volatile().expect("store");
    let oldest = store.save(&read(0)).expect("save");
    for page in 1..=256 {
        store.save(&read(page)).expect("save");
    }
    assert_eq!(store.load(&oldest).expect("evicted"), None);
    let count: i64 = store
        .connection
        .lock()
        .expect("connection")
        .query_row("SELECT COUNT(*) FROM open_continuations", [], |r| r.get(0))
        .expect("count");
    assert_eq!(count, 256);
    let large = ReadContinuation::new("kmp_wake", json!({"about":"x".repeat(MAX_CALL_BYTES + 1)}))
        .expect("call");
    assert!(store.save(&large).is_err());
}

#[test]
fn only_complete_native_calls_are_retained() {
    assert!(ReadContinuation::new("kmp_write_memory", json!({})).is_err());
    assert!(ReadContinuation::new("kmp_wake", json!({"continuation":"read_other"})).is_err());
    assert!(ReadContinuation::new("kmp_ingest", json!({"about":"a"})).is_err());
    assert!(ReadContinuationId::parse("read_xyz").is_err());
    let id = ReadContinuationId::random().expect("entropy");
    assert!(ReadContinuationId::parse(id.as_str()).is_ok());
}
