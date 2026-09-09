use super::*;
use std::collections::VecDeque;
use std::sync::Barrier;
use std::sync::atomic::{AtomicUsize, Ordering};

fn register(directory: &SqliteAgentDirectory, key: &str) -> AgentContext {
    directory
        .open(&AgentOpen::Register { key: key.into() }, "guide-r1")
        .expect("register")
}

#[test]
fn restart_and_registration_retry_preserve_identity_name_and_served_topics() {
    let dir = tempfile::tempdir().expect("directory");
    let path = dir.path().join("agents.sqlite3");
    let original = {
        let registry = SqliteAgentDirectory::at(&path).expect("registry");
        let context = register(&registry, "host-task-agent-1");
        assert!(context.durable);
        registry
            .served(&context.session, "guide-r1", "write")
            .expect("served");
        let folded = registry
            .fold(&context.session, "guide-r1", "write")
            .expect("fold");
        assert!(folded.expanded.is_empty());
        assert_eq!(folded.served, ["write"]);
        context
    };
    let registry = SqliteAgentDirectory::at(&path).expect("reopen");
    let replayed = register(&registry, "host-task-agent-1");
    assert_eq!(original.identity, replayed.identity);
    assert_eq!(original.session, replayed.session);
    assert_eq!(replayed.served, ["write"]);
    assert!(replayed.expanded.is_empty());
}

#[test]
fn users_sharing_a_directory_keep_separate_contexts_and_guidance() {
    let registry = SqliteAgentDirectory::volatile().expect("registry");
    let a = register(&registry, "agent-a");
    let b = register(&registry, "agent-b");
    assert_ne!(a.identity.id, b.identity.id);
    assert_ne!(a.identity.name, b.identity.name);
    assert!(!a.durable);
    registry
        .served(&a.session, "guide-r1", "write")
        .expect("served");
    let b = registry
        .open(
            &AgentOpen::Resume {
                context: b.session.context_id.clone(),
            },
            "guide-r1",
        )
        .expect("resume");
    assert!(b.served.is_empty());
    let mixed = AgentSession {
        agent_id: a.identity.id,
        context_id: b.session.context_id,
    };
    assert!(matches!(
        registry.served(&mixed, "guide-r1", "ask"),
        Err(GuidanceError::InvalidSession(_))
    ));
}

#[test]
fn context_reset_and_guide_revision_require_new_delivery_without_erasing_history() {
    let registry = SqliteAgentDirectory::volatile().expect("registry");
    let first = register(&registry, "agent-a");
    registry
        .served(&first.session, "guide-r1", "write")
        .expect("served");
    let request = AgentOpen::NewContext {
        agent: first.identity.id.clone(),
        key: "compaction-1".into(),
    };
    let fresh = registry.open(&request, "guide-r1").expect("fresh context");
    assert_eq!(first.identity, fresh.identity);
    assert_ne!(first.session.context_id, fresh.session.context_id);
    assert!(fresh.served.is_empty());
    assert_eq!(
        fresh.session,
        registry
            .open(&request, "guide-r1")
            .expect("replay reset")
            .session
    );
    let resume = AgentOpen::Resume {
        context: first.session.context_id.clone(),
    };
    let changed = registry.open(&resume, "guide-r2").expect("new guide");
    assert!(changed.guide_changed);
    assert_eq!(changed.guide_revision, "guide-r2");
    assert!(changed.served.is_empty());
    assert!(matches!(
        registry.served(&first.session, "guide-r1", "ask"),
        Err(GuidanceError::StaleGuide)
    ));
    let rows: i64 = registry
        .connection
        .lock()
        .expect("connection")
        .query_row(
            "SELECT COUNT(*) FROM deliveries WHERE revision='guide-r1'",
            [],
            |r| r.get(0),
        )
        .expect("retained history");
    assert_eq!(rows, 1);
}

struct CollisionSource {
    agents: Mutex<VecDeque<(usize, &'static str)>>,
    contexts: AtomicUsize,
}

impl AgentIdentitySource for CollisionSource {
    fn agent(&self) -> Result<AgentIdentity, GuidanceError> {
        let (id, name) = self
            .agents
            .lock()
            .expect("agents")
            .pop_front()
            .expect("next candidate");
        Ok(AgentIdentity {
            id: AgentId::parse(&format!("agent_{id:032x}"))?,
            name: name.into(),
        })
    }
    fn context(&self) -> Result<AgentContextId, GuidanceError> {
        let id = self.contexts.fetch_add(1, Ordering::SeqCst);
        AgentContextId::parse(&format!("context_{id:032x}"))
    }
}

#[test]
fn generated_identifier_and_display_name_collisions_are_retried() {
    let source = Arc::new(CollisionSource {
        agents: Mutex::new(VecDeque::from([
            (1, "same"),
            (1, "different"),
            (2, "same"),
            (3, "free"),
        ])),
        contexts: AtomicUsize::new(1),
    });
    let registry = SqliteAgentDirectory::from_connection(
        Connection::open_in_memory().expect("connection"),
        false,
        source.clone(),
    )
    .expect("registry");
    register(&registry, "first");
    let second = register(&registry, "second");
    assert_eq!(second.identity.name, "free");
    assert!(source.agents.lock().expect("candidates").is_empty());
    assert_eq!(register(&registry, "second").session, second.session);
}

#[test]
fn independent_connections_register_one_logical_agent_once() {
    let dir = tempfile::tempdir().expect("directory");
    let path = dir.path().join("agents.sqlite3");
    let a = SqliteAgentDirectory::at(&path).expect("connection a");
    let b = SqliteAgentDirectory::at(&path).expect("connection b");
    let barrier = Arc::new(Barrier::new(2));
    let other = barrier.clone();
    let worker = std::thread::spawn(move || {
        other.wait();
        register(&b, "same-registration")
    });
    barrier.wait();
    let first = register(&a, "same-registration");
    let second = worker.join().expect("worker");
    assert_eq!(first.identity, second.identity);
    assert_eq!(first.session, second.session);
}
