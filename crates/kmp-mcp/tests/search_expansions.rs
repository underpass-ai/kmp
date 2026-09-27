//! Judged search expansions at write and at ask (P15, Doc2Query--), over
//! the embedded store. The judge is scripted in front of the store, exactly
//! where the embedded backend answers the write dispatcher's internal
//! `kmp_curate` `judge_expansions` call.

use kmp_mcp::{
    EmbeddedKernelMcpBackend, KernelMcpServer, KernelMcpToolBackend, KernelMcpToolFuture,
};
use serde_json::{Value, json};

const ABOUT: &str = "project:paraphrase";
const ROLLOUT: &str = "The rollout slipped because the auditors had not signed off.";

/// The embedded store with a judge that reads every expansion naming
/// `launch` as belonging (0.9) and every other as not (0.1).
struct JudgingBackend {
    inner: EmbeddedKernelMcpBackend,
}

impl KernelMcpToolBackend for JudgingBackend {
    fn backend_name(&self) -> &'static str {
        "embedded"
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        if name == "kmp_curate" && arguments["mode"] == "judge_expansions" {
            let verdicts = arguments["items"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|item| {
                    let yes = item["expansions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|expansion| {
                            let text = expansion.as_str().unwrap_or_default().to_lowercase();
                            if text.contains("launch") || text.contains("lanzamiento") {
                                0.9
                            } else {
                                0.1
                            }
                        })
                        .collect::<Vec<_>>();
                    json!({"ref": item["ref"], "yes": yes})
                })
                .collect::<Vec<_>>();
            let answer = json!({"structuredContent": {
                "enabled": true, "accept_at": 0.5, "judged_by": "jev-test noul>=0.50",
                "verdicts": verdicts, "jev": {"requests": 1, "input_tokens": 120}
            }});
            return Box::pin(async move { Ok(answer) });
        }
        self.inner.call_tool(name, arguments)
    }
}

async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc":"2.0", "id":1, "method":"tools/call",
        "params":{"name":name, "arguments":arguments}})
    .to_string();
    let response = server.handle_json_line(&line).await.expect("MCP response");
    serde_json::from_str(&response).expect("JSON")
}

fn structured(response: &Value) -> &Value {
    assert!(response.get("error").is_none(), "{response}");
    assert_ne!(response["result"]["isError"], true, "{response}");
    &response["result"]["structuredContent"]
}

fn write(expansions: Value) -> Value {
    json!({
        "about": ABOUT, "actor": "agent:test",
        "memories": [
            {"id": "rollout", "kind": "decision", "summary": ROLLOUT,
             "evidence": "release notes", "labels": {"work": ["main"]},
             "search_expansions": expansions},
            {"id": "canteen", "kind": "observation",
             "summary": "The canteen menu changed on Tuesday for the whole building.",
             "evidence": "notice board", "labels": {"work": ["main"]}}
        ]
    })
}

fn judged(dir: &std::path::Path) -> KernelMcpServer {
    let inner = EmbeddedKernelMcpBackend::open(dir).expect("store");
    KernelMcpServer::with_backend(JudgingBackend { inner })
}

async fn ask(server: &KernelMcpServer, question: &str) -> Value {
    let response = call(
        server,
        "kmp_ask",
        json!({"about": ABOUT, "question": question, "answer_policy": "best_effort"}),
    )
    .await;
    structured(&response).clone()
}

fn reached_by_expansion(answer: &Value) -> bool {
    answer.to_string().contains("\"reached_by\":\"expansion\"")
}

#[tokio::test]
async fn judged_expansions_are_stored_and_reach_a_paraphrase_outside_the_core() {
    let dir = tempfile::tempdir().expect("dir");
    let server = judged(dir.path());
    let written = call(
        &server,
        "kmp_write_memory",
        write(json!([
            "Why was the launch postponed?",
            "the rollout slipped",
            "missing auditor approval"
        ])),
    )
    .await;
    let written = structured(&written);
    assert_eq!(written["status"], "committed", "{written}");
    let report = &written["search_expansions"];
    let stored = report["stored"].as_object().expect("stored");
    assert_eq!(stored.len(), 1, "{report}");
    assert_eq!(
        stored.values().next().expect("one memory"),
        &json!(["Why was the launch postponed?"])
    );
    let refused = report["refused"].as_array().expect("refused");
    assert_eq!(refused.len(), 2, "{report}");
    assert!(refused[0]["why"].as_str().expect("why").contains("repeats"));
    assert!(refused[1]["why"].as_str().expect("why").contains("0.10"));
    assert_eq!(report["jev"]["input_tokens"], 120);

    let after = ask(&server, "Why was the launch postponed?").await;
    assert!(reached_by_expansion(&after), "{after}");
    assert!(after.to_string().contains(ROLLOUT), "{after}");
    assert!(
        !after.to_string().contains("search_expansions"),
        "expansions are never shown: {after}"
    );
}

#[tokio::test]
async fn without_the_opt_in_nothing_is_stored_and_the_write_says_why() {
    let dir = tempfile::tempdir().expect("dir");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let written = call(
        &server,
        "kmp_write_memory",
        write(json!(["Why was the launch postponed?"])),
    )
    .await;
    let written = structured(&written);
    assert_eq!(written["status"], "committed");
    assert_eq!(written["search_expansions"]["stored"], json!({}));
    assert!(
        written["search_expansions"]["not_stored"]
            .as_str()
            .expect("reason")
            .contains("opted in"),
        "{written}"
    );
    let after = ask(&server, "Why was the launch postponed?").await;
    assert!(!reached_by_expansion(&after), "{after}");
}

#[tokio::test]
async fn expansions_attach_to_an_existing_memory_and_a_refused_packet_writes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let server = judged(dir.path());
    let written = call(&server, "kmp_write_memory", write(json!([]))).await;
    let reference = structured(&written)["local_refs"]["rollout"]
        .as_str()
        .expect("ref")
        .to_string();

    let refused = call(
        &server,
        "kmp_write_memory",
        json!({"about": ABOUT, "actor": "agent:test",
               "search_summaries": [{"ref": reference, "search_expansions": ["missing auditor approval"]}]}),
    )
    .await;
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    assert!(
        refused.to_string().contains("EXPANSIONS_NOT_STORED"),
        "{refused}"
    );

    let attached = call(
        &server,
        "kmp_write_memory",
        json!({"about": ABOUT, "actor": "agent:test",
               "search_summaries": [{"ref": reference,
                   "search_expansions": ["¿Por qué se retrasó el lanzamiento?"]}]}),
    )
    .await;
    let attached = structured(&attached);
    assert_eq!(attached["status"], "committed", "{attached}");
    assert_eq!(
        attached["search_expansions"]["stored"][&reference],
        json!(["¿Por qué se retrasó el lanzamiento?"])
    );
    let after = ask(&server, "¿Por qué se retrasó el lanzamiento?").await;
    assert!(reached_by_expansion(&after), "{after}");
}

#[tokio::test]
async fn a_malformed_proposal_is_refused_before_anything_is_written() {
    let dir = tempfile::tempdir().expect("dir");
    let server = judged(dir.path());
    let seven = json!(["a b", "c d", "e f", "g h", "i j", "k l", "m n"]);
    let refused = call(&server, "kmp_write_memory", write(seven)).await;
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    let refused = call(&server, "kmp_write_memory", write(json!("launch"))).await;
    assert_eq!(refused["result"]["isError"], true, "{refused}");
}
