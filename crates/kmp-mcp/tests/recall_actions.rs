//! Execute returned calls against real memory, with no reconstructed arguments.
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
use std::collections::BTreeMap;

use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:recall-actions";

async fn request(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    let line = server
        .handle_json_line(&request.to_string())
        .await
        .expect("reply");
    let reply: Value = serde_json::from_str(&line).expect("JSON");
    assert!(reply.get("error").is_none(), "{reply}");
    reviewed_writer::review_authored_write(server, reply["result"].clone()).await
}

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let result = request(server, tool, arguments).await;
    assert_ne!(result["isError"], true, "{result}");
    result["structuredContent"].clone()
}

async fn write(server: &KernelMcpServer, key: &str, memories: Value) -> Value {
    call(
        server,
        "kmp_write_memory",
        json!({
            "about":ABOUT,"actor":"recall-test","observed_at":"2026-09-01T10:00:00Z",
            "idempotency_key":key,"memories":memories
        }),
    )
    .await["local_refs"]
        .clone()
}

async fn seed(server: &KernelMcpServer) -> Value {
    write(server, "recall-actions:source", json!([
        {"id":"c1","kind":"constraint","summary":"Export must work offline.",
         "evidence":"The requirement explicitly prohibits a network dependency for export.",
         "observed_at":"2026-09-01T08:00:00Z","occurred_at":"2026-09-01T08:00:00Z",
         "valid_from":"2026-09-01T08:00:00Z","labels":{"component":["export","offline"],"source":["spec"]}},
        {"id":"d1","kind":"decision","summary":"Export uses SQLite.",
         "evidence":"The design chooses SQLite because export must work offline.",
         "observed_at":"2026-09-01T09:00:00Z","occurred_at":"2026-09-01T09:00:00Z",
         "valid_from":"2026-09-01T09:00:00Z","labels":{"component":["export"]},
         "connect_to":[{"ref":"@c1","rel":"chosen_because","class":"causal","confidence":"high",
            "why":"Local SQLite storage satisfies the offline export constraint.",
            "evidence":"The design cites offline export as the reason for choosing SQLite."}]},
        {"id":"x1","kind":"observation","summary":"Warehouse export uses inventory snapshots.",
         "evidence":"The warehouse log records its inventory export.",
         "observed_at":"2026-09-01T10:00:00Z","labels":{"component":["warehouse"]}}
    ])).await
}

fn arguments(tool: &str, axis: &str, time: Value) -> Value {
    let mut args = json!({"about":ABOUT,"axis":axis,
        "dimensions":{"scope":"abouts","abouts":[ABOUT],"mode":"all",
            "selectors":[{"key":"component","op":"in","values":["export"]},
                {"key":"component","op":"notin","values":["warehouse"]},
                {"key":"component","op":"exists"},{"key":"incident","op":"notexists"}]},
        "budget":{"max_bytes":8000,"detail":"full","depth":3},"page":{"entries":1}});
    args.as_object_mut()
        .expect("arguments")
        .extend(time.as_object().expect("time selection").clone());
    if tool == "kmp_ask" {
        args["question"] = json!("Why does export use SQLite offline?");
        args["asked_as"] = json!("¿Por qué exporta sin conexión con SQLite?");
        args["answer_policy"] = json!("show_conflicts");
    } else {
        args["intent"] = json!("Review export evidence");
        args["role"] = json!("reviewer");
    }
    args
}

#[tokio::test]
async fn returned_calls_preserve_time_language_and_labels_and_reconstruct_proof() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let refs = seed(&server).await;
    for tool in ["kmp_wake", "kmp_ask"] {
        for axis in ["occurred", "observed", "validity"] {
            for time in [
                json!({"interval":{"start":"2026-09-01T07:00:00Z","end":"2026-09-01T10:00:00Z"}}),
                json!({"as_of":{"ref":refs["d1"]}}),
                json!({"as_of":{"time":"2026-09-01T09:30:00Z"}}),
            ] {
                let initial = arguments(tool, axis, time);
                let mut full_args = initial.clone();
                full_args["budget"]["max_bytes"] = json!(100_000);
                full_args.as_object_mut().expect("arguments").remove("page");
                let full = call(&server, tool, full_args).await;
                let mut args = initial.clone();
                let mut sections = BTreeMap::<String, Vec<Value>>::new();
                let mut complete = false;
                for _ in 0..100 {
                    let page = call(&server, tool, args).await;
                    assert_eq!(page["projection"]["core_text_shortened"], false);
                    assert_eq!(page["proof"]["axis"], full["proof"]["axis"]);
                    assert_eq!(page["proof"]["interval"], full["proof"]["interval"]);
                    assert_eq!(page["proof"]["as_of"], full["proof"]["as_of"]);
                    let bytes = serde_json::to_vec(&page).expect("page JSON").len();
                    assert_eq!(page["projection"]["budget"]["used_bytes"], bytes);
                    assert!(bytes <= 8000);
                    for (name, counts) in page["projection"]["sections"]
                        .as_object()
                        .expect("sections")
                    {
                        let path = format!("/{}", name.replace('.', "/"));
                        let values = page
                            .pointer(&path)
                            .expect("section")
                            .as_array()
                            .expect("array");
                        let skip = if sections.contains_key(name) {
                            counts["core"].as_u64().expect("core") as usize
                        } else {
                            0
                        };
                        sections
                            .entry(name.clone())
                            .or_default()
                            .extend(values.iter().skip(skip).cloned());
                    }
                    if page["projection"]["page"]["has_more"] == false {
                        assert!(page["projection"]["next_action"].is_null());
                        complete = true;
                        break;
                    }
                    let action = &page["projection"]["next_action"];
                    assert_eq!(action["tool"], tool);
                    args = action["arguments"].clone();
                    for key in [
                        "about",
                        "question",
                        "asked_as",
                        "answer_policy",
                        "role",
                        "intent",
                        "axis",
                        "as_of",
                        "interval",
                        "dimensions",
                    ] {
                        assert_eq!(args[key], initial[key], "preserve {key}");
                    }
                    assert!(
                        args["budget"].get("max_entries").is_none(),
                        "unlimited is omitted, not invalid zero"
                    );
                    assert_eq!(args["page"]["entries"], 1);
                }
                assert!(complete, "finite proof must finish");
                for (name, values) in sections {
                    let path = format!("/{}", name.replace('.', "/"));
                    assert_eq!(
                        &json!(values),
                        full.pointer(&path).expect("full section"),
                        "{tool} {axis}: {name}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn stalled_recall_actions_negotiate_progress_without_guessing() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    seed(&server).await;
    for tool in ["kmp_wake", "kmp_ask"] {
        let mut args = arguments(
            tool,
            "ingested",
            json!({"interval":{"start":"2026-09-01T00:00:00Z"}}),
        );
        args["budget"]["max_bytes"] = json!(512);
        let page = call(&server, tool, args).await;
        assert_eq!(page["projection"]["page"]["returned"], 0);
        let action = &page["projection"]["next_action"];
        assert!(
            action["arguments"]["budget"]["max_bytes"]
                .as_u64()
                .expect("budget")
                >= page["projection"]["page"]["minimum_progress_bytes"]
                    .as_u64()
                    .expect("minimum")
        );
        let next = call(&server, tool, action["arguments"].clone()).await;
        assert!(
            next["projection"]["page"]["returned"]
                .as_u64()
                .expect("progress")
                > 0
        );
        assert_eq!(next["projection"]["core_text_shortened"], false);
    }
}

#[tokio::test]
async fn selection_changes_and_malformed_cursors_return_executable_restarts() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    seed(&server).await;
    for tool in ["kmp_wake", "kmp_ask"] {
        let initial = arguments(
            tool,
            "observed",
            json!({"interval":{"start":"2026-09-01T07:00:00Z"}}),
        );
        let page = call(&server, tool, initial.clone()).await;
        let mut changed = page["projection"]["next_action"]["arguments"].clone();
        changed["axis"] = json!("occurred");
        let error = request(&server, tool, changed.clone()).await;
        assert_eq!(error["isError"], true);
        assert_eq!(
            error["structuredContent"]["error"]["code"], "conflict",
            "{error}"
        );
        let action = &error["structuredContent"]["feedback"][0]["action"];
        assert_eq!(action["tool"], tool);
        assert!(action["arguments"]["page"].get("cursor").is_none());
        assert_eq!(action["arguments"]["axis"], "occurred");
        call(&server, tool, action["arguments"].clone()).await;
        changed["page"]["cursor"] = json!("malformed");
        let error = request(&server, tool, changed).await;
        assert_eq!(
            error["structuredContent"]["error"]["code"],
            "invalid_argument"
        );
        let action = &error["structuredContent"]["feedback"][0]["action"];
        call(&server, tool, action["arguments"].clone()).await;
    }
}
