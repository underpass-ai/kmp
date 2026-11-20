//! A replayed store answers `kmp_wake` with the same label catalogue as its
//! source (#909). Labelled memories in two abouts, a relabel among them, go
//! out through `kmp-mcp export` and back in through `kmp-mcp import` into a
//! fresh store on a hermetic machine: HOME, XDG and both stores are temporary.
//! The whole catalogue, every page of it, must come back unchanged.

use std::path::Path;
use std::process::{Command, Output};

use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const MAIN: &str = "project:catalogue-main";
const OTHER: &str = "project:catalogue-other";
const AT: &str = "2026-09-01T10:00:00Z";

async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":name,"arguments":arguments}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    let response: Value = serde_json::from_str(&response).expect("JSON");
    assert!(response.get("error").is_none(), "{response}");
    assert_ne!(response["result"]["isError"], true, "{response}");
    response["result"]["structuredContent"].clone()
}

fn labelled(about: &str, key: &str, labels: Value, memories: Value) -> Value {
    json!({
        "about": about,
        "actor": "fixture",
        "source_kind": "human",
        "labels": labels,
        "occurred_at": AT,
        "observed_at": AT,
        "idempotency_key": key,
        "options": {"strict": false},
        "memories": memories
    })
}

const MOVED: &str = "project:catalogue-main:decision:moved";

fn memory(id: &str, kind: &str, summary: &str) -> Value {
    json!({"id": id, "kind": kind, "summary": summary,
        "evidence": format!("Fixture record for {id}.")})
}

async fn seed(store: &Path) {
    let server = KernelMcpServer::embedded(store).expect("source server");
    for write in [
        labelled(
            MAIN,
            "catalogue:corte-3",
            json!({"task": ["corte-3"], "agentic_process": ["implementation"]}),
            json!([
                json!({"id": "a", "ref": MOVED, "kind": "decision",
                    "summary": "Corte 3 keeps the stream as the source of truth.",
                    "evidence": "Fixture record for a."}),
                memory(
                    "b",
                    "observation",
                    "Corte 3 projections rebuild from the log."
                ),
            ]),
        ),
        labelled(
            MAIN,
            "catalogue:corte-4",
            json!({"task": ["corte-4"], "agentic_process": ["code-review"], "component": ["store"]}),
            json!([memory(
                "c",
                "decision",
                "Corte 4 reviews every store migration."
            )]),
        ),
        labelled(
            OTHER,
            "catalogue:other",
            json!({"component": ["viewer"], "source": ["registry"]}),
            json!([memory("d", "observation", "The viewer reads the registry.")]),
        ),
    ] {
        let written = call(&server, "kmp_write_memory", write).await;
        assert_eq!(written["accepted"], true, "{written}");
    }
    let relabelled = call(
        &server,
        "kmp_relabel",
        json!({"about": MAIN, "ref": MOVED, "actor": "fixture",
            "observed_at": "2026-09-02T10:00:00Z",
            "add": {"task": ["corte-5"]}, "remove": {"task": ["corte-3"]},
            "why": "The decision moved to the next cut.",
            "idempotency_key": "catalogue:relabel"}),
    )
    .await;
    assert_eq!(relabelled["accepted"], true, "{relabelled}");
}

/// Every label the wake catalogue holds for `about`, following the packet's
/// own continuation until it has nothing more, sorted so two stores compare.
async fn catalogue(store: &Path, about: &str) -> Vec<Value> {
    let server = KernelMcpServer::embedded(store).expect("server");
    let mut arguments = json!({"about": about, "budget": {"max_bytes": 100000}});
    let mut labels = Vec::new();
    for _ in 0..64 {
        let packet = call(&server, "kmp_wake", arguments).await;
        labels.extend(packet["labels"].as_array().cloned().unwrap_or_default());
        let projection = &packet["projection"];
        if projection["page"]["has_more"] != true {
            labels.sort_by_key(Value::to_string);
            return labels;
        }
        arguments = projection["next_action"]["arguments"].clone();
    }
    panic!("the wake for {about} never finished paging");
}

struct Machine {
    home: tempfile::TempDir,
}

impl Machine {
    fn run(&self, store: &Path, args: &[&str]) -> Output {
        let output = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"))
            .args(args)
            .current_dir(self.home.path())
            .env("HOME", self.home.path())
            .env("XDG_DATA_HOME", self.home.path().join(".local/share"))
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .env("KMP_MCP_DATA_DIR", store)
            .env("KMP_VIEWER_ADDR", "off")
            .env_remove("KMP_MCP_BACKEND")
            .output()
            .expect("kmp-mcp runs");
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

#[tokio::test]
async fn export_then_import_keeps_the_wake_label_catalogue() {
    let machine = Machine {
        home: tempfile::tempdir().expect("home"),
    };
    let source = tempfile::tempdir().expect("source");
    seed(source.path()).await;
    let bundle = machine.home.path().join("full.jsonl");
    let bundle = bundle.to_str().expect("utf8");
    machine.run(source.path(), &["export", bundle]);
    let replayed = tempfile::tempdir().expect("replayed");
    machine.run(replayed.path(), &["import", bundle]);

    for about in [MAIN, OTHER] {
        let expected = catalogue(source.path(), about).await;
        let pairs = expected
            .iter()
            .filter(|label| label["about"] == about)
            .map(|label| format!("{}={}", label["key"], label["value"]))
            .collect::<Vec<_>>();
        assert!(
            pairs.len() >= 2,
            "the source must hold a real catalogue for {about}: {expected:?}"
        );
        assert_eq!(
            catalogue(replayed.path(), about).await,
            expected,
            "replay changed the label catalogue of {about}"
        );
    }
    let main = catalogue(source.path(), MAIN)
        .await
        .iter()
        .map(|label| format!("{}={} {}", label["key"], label["value"], label["entries"]))
        .collect::<Vec<_>>();
    assert_eq!(
        main,
        [
            r#""agentic_process"="code-review" 1"#,
            r#""component"="store" 1"#,
            r#""task"="corte-3" 1"#,
            r#""task"="corte-4" 1"#,
            r#""task"="corte-5" 1"#,
            r#""agentic_process"="implementation" 2"#,
        ],
        "the relabel moved one entry from corte-3 to corte-5"
    );
}
