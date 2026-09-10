use super::*;
use crate::guidance::{AgentDirectory, AgentOpen};
use serde_json::json;

fn register(directory: &SqliteAgentDirectory, key: &str) -> AgentSession {
    directory
        .open(&AgentOpen::Register { key: key.into() }, "r1")
        .expect("register")
        .session
}

fn read(session: &AgentSession, page: usize) -> ReadContinuation {
    ReadContinuation::new("kmp_wake", json!({"context_id":session.context_id.as_str(),"about":"project:a","page":{"cursor":format!("page-{page}")}})).expect("read")
}

#[test]
fn retained_calls_survive_reconnect_deduplicate_and_expire_without_refresh() {
    let dir = tempfile::tempdir().expect("store");
    let path = dir.path().join("agents.sqlite3");
    let directory = SqliteAgentDirectory::at(&path).expect("directory");
    let session = register(&directory, "a");
    let call = read(&session, 1);
    let id = save(&directory, &session, &call).expect("save");
    assert_eq!(save(&directory, &session, &call).expect("replay"), id);
    drop(directory);
    let directory = SqliteAgentDirectory::at(&path).expect("reconnect");
    assert_eq!(load(&directory, &id).expect("read"), Some(call.clone()));
    directory
        .connection
        .lock()
        .expect("connection")
        .execute("UPDATE read_continuations SET expires_at=unixepoch()-1", [])
        .expect("expire");
    assert_eq!(load(&directory, &id).expect("expired"), None);
    assert_ne!(save(&directory, &session, &call).expect("new handle"), id);
}

#[test]
fn retention_is_bounded_per_context_and_directory() {
    let directory = SqliteAgentDirectory::volatile().expect("directory");
    let session = register(&directory, "a");
    let oldest = save(&directory, &session, &read(&session, 0)).expect("first");
    let newest = (1..17)
        .map(|i| save(&directory, &session, &read(&session, i)).expect("save"))
        .last()
        .expect("last");
    assert_eq!(load(&directory, &oldest).expect("evicted"), None);
    assert!(load(&directory, &newest).expect("latest").is_some());
    for i in 0..256 {
        let session = register(&directory, &format!("context-{i}"));
        save(&directory, &session, &read(&session, i)).expect("save");
    }
    assert_eq!(load(&directory, &newest).expect("globally evicted"), None);
    let count: i64 = directory
        .connection
        .lock()
        .expect("connection")
        .query_row("SELECT COUNT(*) FROM read_continuations", [], |r| r.get(0))
        .expect("count");
    assert_eq!(count, 256);
}

#[test]
fn continuations_reject_writes_recursive_calls_oversize_and_mixed_contexts() {
    assert!(ReadContinuation::new("kmp_write_memory", json!({})).is_err());
    assert!(ReadContinuation::new("kmp_wake", json!({"continuation":"read_other"})).is_err());
    let directory = SqliteAgentDirectory::volatile().expect("directory");
    let a = register(&directory, "a");
    let b = register(&directory, "b");
    assert!(save(&directory, &a, &read(&b, 1)).is_err());
    let mixed = AgentSession {
        agent_id: b.agent_id,
        context_id: a.context_id.clone(),
    };
    assert!(save(&directory, &mixed, &read(&a, 1)).is_err());
    let mut large = read(&a, 1);
    large.arguments["intent"] = json!("x".repeat(MAX_CALL_BYTES));
    assert!(save(&directory, &a, &large).is_err());
}
