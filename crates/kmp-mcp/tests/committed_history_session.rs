//! An open project session follows its committed bundle: a pull during the
//! session is caught up before the next read, a divergence resolved in
//! another terminal stops being reported, and an open that found the lock
//! held tries again on the next read (B1).

use std::path::{Path, PathBuf};

use kmp_mcp::{EmbeddedKernelMcpBackend, KernelMcpServer};
use serde_json::{Value, json};

struct Checkout {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Checkout {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("checkout");
        let root = dir.path().to_path_buf();
        Self { _dir: dir, root }
    }
    fn data_dir(&self) -> PathBuf {
        self.root.join(".kernel")
    }
    fn bundle(&self) -> PathBuf {
        self.root.join(".kmp/memory.jsonl")
    }
    fn server(&self) -> KernelMcpServer {
        let native = kmp_embedded::CommitNativeBundle::new(self.data_dir(), self.bundle());
        let backend = EmbeddedKernelMcpBackend::open_with_engine_and_commit_native(
            &self.data_dir(),
            None,
            Some(native),
        )
        .expect("backend");
        KernelMcpServer::with_embedded_backend(backend)
    }
    fn pull_from(&self, other: &Checkout) {
        let bundle = std::fs::read_to_string(other.bundle()).expect("their bundle");
        kmp_embedded::write_bundle_atomically(&self.bundle(), &bundle).expect("pull");
    }
}

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": tool, "arguments": arguments}})
    .to_string();
    let reply = server.handle_json_line(&line).await.expect("reply");
    let reply: Value = serde_json::from_str(&reply).expect("json");
    reply["result"]["structuredContent"].clone()
}

async fn write(server: &KernelMcpServer, decision: &str) -> Value {
    call(
        server,
        "kmp_write_memory",
        json!({"about": "project:team", "actor": "test", "labels": {"topic": ["deploy"]},
            "idempotency_key": format!("decision:{decision}"),
            "memories": [{"id": "m", "kind": "decision",
                "summary": format!("Decision {decision}: the deploy window is {decision}."),
                "evidence": format!("Notes {decision}.")}]}),
    )
    .await
}

async fn ask(server: &KernelMcpServer, decision: &str) -> Value {
    call(
        server,
        "kmp_ask",
        json!({"about": "project:team",
            "question": format!("What is the deploy window in decision {decision}?")}),
    )
    .await
}

fn warnings(reply: &Value) -> String {
    reply["warnings"].to_string()
}

fn lock(data_dir: &Path) -> std::fs::File {
    std::fs::create_dir_all(data_dir).expect("data dir");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(data_dir.join("commit-native-bundle.lock"))
        .expect("lock file");
    file.lock().expect("lock");
    file
}

#[tokio::test]
async fn a_pull_during_an_open_session_is_read_before_the_next_answer() {
    let teammate = Checkout::new();
    let theirs = teammate.server();
    assert_eq!(write(&theirs, "one").await["status"], "committed");
    let mine = Checkout::new();
    mine.pull_from(&teammate);
    let session = mine.server();
    assert_eq!(ask(&session, "one").await["answer_status"], "answered");

    assert_eq!(write(&theirs, "kestrel").await["status"], "committed");
    mine.pull_from(&teammate);
    let reply = ask(&session, "kestrel").await;
    assert_eq!(reply["answer_status"], "answered", "{reply}");
    assert!(!warnings(&reply).contains("committed memory"), "{reply}");
}

#[tokio::test]
async fn a_divergence_resolved_elsewhere_stops_being_reported() {
    let teammate = Checkout::new();
    let theirs = teammate.server();
    write(&theirs, "one").await;
    let mine = Checkout::new();
    mine.pull_from(&teammate);
    let session = mine.server();
    assert_eq!(write(&session, "mine").await["status"], "committed");
    write(&theirs, "theirs").await;
    mine.pull_from(&teammate);

    let reply = call(&session, "kmp_wake", json!({"about": "project:team"})).await;
    assert!(warnings(&reply).contains("different histories"), "{reply}");

    // `kmp-mcp export --overwrite-committed` in another terminal.
    let store = kmp_embedded::EmbeddedKernelStore::open(&mine.data_dir()).expect("store");
    let live = store.export_bundle_blocking().expect("export");
    kmp_embedded::write_bundle_atomically(&mine.bundle(), &live).expect("overwrite");

    let reply = call(&session, "kmp_wake", json!({"about": "project:team"})).await;
    assert!(!warnings(&reply).contains("different histories"), "{reply}");
    assert_eq!(write(&session, "after").await["status"], "committed");
}

#[tokio::test]
async fn an_open_that_found_the_lock_held_catches_up_on_the_next_read() {
    let teammate = Checkout::new();
    let theirs = teammate.server();
    write(&theirs, "osprey").await;
    let clone = Checkout::new();
    clone.pull_from(&teammate);

    let held = lock(&clone.data_dir());
    let session = clone.server();
    held.unlock().expect("unlock");

    let reply = ask(&session, "osprey").await;
    assert_eq!(reply["answer_status"], "answered", "{reply}");
}
