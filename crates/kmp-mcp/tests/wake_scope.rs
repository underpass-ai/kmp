//! Historical proof and about context must identify their different provenance.
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:wake-scope";

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

async fn seed(server: &KernelMcpServer) -> Value {
    call(server, "kmp_write_memory", json!({
        "about":ABOUT,"actor":"scope-test","observed_at":"2026-09-01T10:00:00Z",
        "idempotency_key":"wake-scope:source","memories":[
            {"id":"c1","kind":"constraint","summary":"C1: Export must work offline.",
             "evidence":"The requirement explicitly prohibits a network dependency for export.",
             "observed_at":"2026-09-01T08:00:00Z","labels":{"component":["export"]}},
            {"id":"d1","kind":"decision","summary":"D1: Export uses SQLite.",
             "evidence":"The design chooses SQLite because export must work offline.",
             "observed_at":"2026-09-01T09:00:00Z","labels":{"component":["export"]},
             "connect_to":[{"ref":"@c1","rel":"chosen_because","class":"causal","confidence":"high",
                "why":"Local SQLite storage satisfies the offline export constraint.",
                "evidence":"The design cites offline export as the reason for choosing SQLite."}]},
            {"id":"x1","kind":"observation","summary":"X1: The warehouse exports a separate inventory.",
             "evidence":"The warehouse log records its inventory export.",
             "observed_at":"2026-09-01T10:00:00Z","labels":{"component":["warehouse"],"source":["inventory-log"]}}
        ]
    })).await["local_refs"].clone()
}

fn claims(packet: &Value) -> Vec<&str> {
    packet["proof"]["evidence"]
        .as_array()
        .expect("evidence")
        .iter()
        .flat_map(|e| e["supports"].as_array().expect("supports"))
        .filter_map(Value::as_str)
        .collect()
}

fn scope_contains(packet: &Value, group: &str, path: &str) -> bool {
    packet["scope"][group]
        .as_array()
        .expect("scope group")
        .contains(&json!(path))
}

#[tokio::test]
async fn historical_proof_is_separate_from_later_about_context() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let refs = seed(&server).await;
    for temporal in [
        json!({"interval":{"start":"2026-09-01T08:00:00Z","end":"2026-09-01T09:00:00Z"}}),
        json!({"as_of":{"ref":refs["c1"]}}),
    ] {
        let mut query = json!({"about":ABOUT,"axis":"observed","intent":"Review the source window",
            "budget":{"max_bytes":100000,"detail":"full"}});
        query
            .as_object_mut()
            .expect("query")
            .extend(temporal.as_object().expect("selection").clone());
        let packet = call(&server, "kmp_wake", query).await;
        assert!(
            !packet["projection"]["page"]["has_more"]
                .as_bool()
                .expect("page")
        );
        assert!(claims(&packet).contains(&refs["c1"].as_str().expect("C1")));
        assert!(!claims(&packet).contains(&refs["d1"].as_str().expect("D1")));
        assert!(
            packet["wake"]["current_state"]
                .to_string()
                .contains(refs["d1"].as_str().expect("D1"))
        );
        assert!(scope_contains(&packet, "selection", "proof"));
        assert!(scope_contains(&packet, "context", "wake.current_state"));
        assert_eq!(packet["scope"]["context_time"], "unbounded");
        assert!(scope_contains(&packet, "request", "wake.objective"));
        assert_eq!(packet["wake"]["objective"], "Review the source window");
        for group in ["selection", "context", "request"] {
            for path in packet["scope"][group].as_array().expect("paths") {
                let pointer = format!("/{}", path.as_str().expect("path").replace('.', "/"));
                assert!(
                    packet.pointer(&pointer).is_some(),
                    "declared path {pointer} must exist"
                );
            }
        }
    }
}

#[tokio::test]
async fn dimensional_context_survives_an_empty_historical_selection() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let refs = seed(&server).await;
    let query = json!({"about":ABOUT,"dimensions":{"selectors":[{"key":"component","op":"in","values":["warehouse"]}]},
        "budget":{"max_bytes":100000,"detail":"full"}});
    for (predicate, expected) in [
        (
            json!({"key":"component","op":"in","values":["warehouse"]}),
            vec!["x1"],
        ),
        (
            json!({"key":"component","op":"notin","values":["warehouse"]}),
            vec!["c1", "d1"],
        ),
        (json!({"key":"source","op":"exists"}), vec!["x1"]),
        (json!({"key":"source","op":"notexists"}), vec!["c1", "d1"]),
    ] {
        let mut filtered = query.clone();
        filtered["dimensions"]["selectors"] = json!([predicate]);
        let packet = call(&server, "kmp_wake", filtered.clone()).await;
        assert_eq!(
            packet["scope"]["dimensions"]["selectors"],
            filtered["dimensions"]["selectors"]
        );
        let selected = claims(&packet);
        for id in ["c1", "d1", "x1"] {
            let reference = refs[id].as_str().expect("ref");
            assert_eq!(
                selected.contains(&reference),
                expected.contains(&id),
                "{predicate}: {id}"
            );
            if !expected.contains(&id) {
                assert!(
                    !packet["wake"]["current_state"]
                        .to_string()
                        .contains(reference)
                );
            }
        }
    }
    let current = call(&server, "kmp_wake", query.clone()).await;
    assert_eq!(
        current["scope"]["dimensions"]["selectors"],
        query["dimensions"]["selectors"]
    );
    assert_eq!(current["scope"]["dimensions"]["scope"], "current_about");
    assert!(claims(&current).contains(&refs["x1"].as_str().expect("X1")));
    assert!(
        !current["wake"]
            .to_string()
            .contains(refs["d1"].as_str().expect("D1"))
    );
    assert!(
        current["labels"]
            .as_array()
            .expect("labels")
            .iter()
            .all(|label| label["value"] != "export")
    );
    assert!(
        current["labels"]
            .as_array()
            .expect("labels")
            .iter()
            .any(|label| label["value"] == "inventory-log")
    );
    let mut historical = query;
    historical["axis"] = json!("observed");
    historical["interval"] = json!({"start":"2026-09-01T08:00:00Z","end":"2026-09-01T09:00:00Z"});
    let empty = call(&server, "kmp_wake", historical).await;
    assert!(claims(&empty).is_empty());
    assert_eq!(empty["proof"]["abouts_empty_in_selection"], json!([ABOUT]));
    assert_eq!(empty["resume_cursor"], Value::Null);
    assert_eq!(
        empty["wake"]["current_state"],
        current["wake"]["current_state"]
    );
    assert_eq!(empty["labels"], current["labels"]);
    assert_eq!(empty["scope"], current["scope"]);
    let absent = request(
        &server,
        "kmp_wake",
        json!({"about":"project:wake-scope-missing"}),
    )
    .await;
    assert_eq!(absent["isError"], true);
    assert_eq!(absent["structuredContent"]["error"]["code"], "not_found");
}

#[tokio::test]
async fn scope_is_unchanged_and_counted_inside_a_tight_response_floor() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    seed(&server).await;
    let full = call(
        &server,
        "kmp_wake",
        json!({"about":ABOUT,"budget":{"max_bytes":100000,"detail":"full"}}),
    )
    .await;
    let tight = call(
        &server,
        "kmp_wake",
        json!({"about":ABOUT,"budget":{"max_bytes":512,"detail":"full"}}),
    )
    .await;
    assert_eq!(tight["scope"], full["scope"]);
    let bytes = serde_json::to_vec(&tight).expect("serialized packet").len();
    assert_eq!(tight["projection"]["budget"]["used_bytes"], bytes);
    if bytes > 512 {
        assert!(!tight["warnings"].as_array().expect("warnings").is_empty());
    }
}
