//! Returned inspection calls recover the selected proof without guessing arguments.
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:inspect-actions";
const SECTIONS: [&str; 4] = ["/evidence", "/links/incoming", "/links/outgoing", "/raw"];

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
        "about":ABOUT,"actor":"native-inspection","observed_at":"2026-09-01T10:00:00Z",
        "idempotency_key":"inspect-actions:source","labels":{"component":["export"]},
        "memories":[
            {"id":"constraint","kind":"constraint","summary":"C1: Export must work offline.",
             "evidence":"The source requirement prohibits network access during ledger export."},
            {"id":"decision","kind":"decision","summary":"D1: Export uses SQLite.",
             "evidence":"Signed source — 原文. SQLite keeps the ledger on this device. ".repeat(45),
             "connect_to":[{"ref":"@constraint","rel":"chosen_because","class":"motivational","confidence":"high",
                "why":"SQLite satisfies the source requirement for a device-local export.",
                "evidence":"D1 explicitly chooses SQLite to meet C1's offline export constraint."}]},
            {"id":"check","kind":"observation","summary":"V1: Offline export passed the source check.",
             "evidence":"V1 records the same exported ledger with the network disabled.",
             "connect_to":[{"ref":"@decision","rel":"supports","class":"evidential","confidence":"high",
                "why":"The observed export verifies the stated offline property of the chosen storage.",
                "evidence":"V1 records a complete export while the network was disabled."}]}
        ]
    })).await["local_refs"].clone()
}

#[tokio::test]
async fn returned_calls_reconstruct_selected_inspection_and_negotiate_progress() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let refs = seed(&server).await;
    for (include, reuse) in [
        (
            json!({"details":true,"incoming":true,"outgoing":true,"raw":true}),
            false,
        ),
        (
            json!({"details":true,"incoming":true,"outgoing":true,"raw":true}),
            true,
        ),
        (
            json!({"details":false,"incoming":false,"outgoing":true,"raw":false}),
            true,
        ),
        (
            json!({"details":true,"incoming":true,"outgoing":false,"raw":true}),
            false,
        ),
    ] {
        let mut args = json!({"about":ABOUT,"ref":refs["decision"],"include":include,
            "budget":{"max_bytes":100000}});
        let full = call(&server, "kmp_inspect", args.clone()).await;
        assert_eq!(full["page"]["has_more"], false);
        assert_eq!(
            full["page"]["required_bytes"],
            serde_json::to_vec(&full)
                .expect("valid inspection response")
                .len()
        );
        args["budget"]["max_bytes"] = json!(512);
        let mut page = call(&server, "kmp_inspect", args.clone()).await;
        assert_eq!(page["object"], full["object"]);
        assert_eq!(page["page"]["returned"], 0);
        let mut collected = full.clone();
        for path in SECTIONS {
            *collected.pointer_mut(path).expect("section") = json!([]);
        }
        let mut pages = 0;
        loop {
            pages += 1;
            assert!(pages < 100, "inspection must make bounded progress");
            assert_eq!(
                page["page"]["required_bytes"],
                full["page"]["required_bytes"]
            );
            let size = serde_json::to_vec(&page)
                .expect("valid inspection response")
                .len();
            let allowance = args["budget"]["max_bytes"]
                .as_u64()
                .expect("valid inspection response") as usize;
            if size > allowance {
                assert!(
                    page["warnings"]
                        .as_array()
                        .expect("valid inspection response")
                        .iter()
                        .any(|w| {
                            let text = w.as_str().unwrap_or_default();
                            text.contains(&format!("{size}-byte floor"))
                                && text.contains(&format!("max_bytes {allowance}"))
                        })
                );
            }
            for path in SECTIONS {
                collected
                    .pointer_mut(path)
                    .expect("valid inspection response")
                    .as_array_mut()
                    .expect("valid inspection response")
                    .extend(
                        page.pointer(path)
                            .expect("valid inspection response")
                            .as_array()
                            .expect("valid inspection response")
                            .iter()
                            .cloned(),
                    );
            }
            if page["page"]["has_more"] == false {
                assert_eq!(page["next_actions"], json!([]));
                break;
            }
            let action = page["next_actions"][0].clone();
            assert_eq!(action["tool"], "kmp_inspect");
            assert_eq!(action["arguments"]["about"], ABOUT);
            assert_eq!(action["arguments"]["ref"], refs["decision"]);
            assert_eq!(action["arguments"]["include"], include);
            assert_eq!(
                action["arguments"]["page"]["cursor"],
                page["page"]["next_cursor"]
            );
            let stalled = page["page"]["returned"] == 0;
            if stalled {
                assert!(
                    action["arguments"]["budget"]["max_bytes"]
                        .as_u64()
                        .expect("valid inspection response")
                        >= page["page"]["minimum_progress_bytes"]
                            .as_u64()
                            .expect("valid inspection response")
                );
                if full["page"]["required_bytes"]
                    .as_u64()
                    .expect("valid inspection response")
                    <= 10_000
                {
                    assert_eq!(
                        action["arguments"]["budget"]["max_bytes"],
                        full["page"]["required_bytes"]
                    );
                }
            }
            args = action["arguments"].clone();
            if reuse {
                args["page"]["repeat_object"] = json!(false);
            }
            page = call(
                &server,
                action["tool"].as_str().expect("valid inspection response"),
                args.clone(),
            )
            .await;
            if stalled {
                assert!(
                    page["page"]["returned"]
                        .as_u64()
                        .expect("valid inspection response")
                        > 0
                );
            }
            if reuse {
                assert_eq!(page["object_reused"], true);
                assert_eq!(page["object"], json!({"ref":refs["decision"]}));
            } else {
                assert_eq!(page["object"], full["object"]);
            }
        }
        for path in ["/object", "/evidence", "/links", "/raw"] {
            assert_eq!(collected.pointer(path), full.pointer(path), "{path}");
        }
        assert!(pages > 1);
    }
}

#[tokio::test]
async fn changed_object_returns_a_fresh_call_that_does_not_reuse_the_old_object() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let refs = seed(&server).await;
    let query = json!({"about":ABOUT,"ref":refs["decision"],
        "include":{"details":true,"incoming":false,"outgoing":true,"raw":true},
        "budget":{"max_bytes":512}});
    let first = call(&server, "kmp_inspect", query.clone()).await;
    call(&server,"kmp_write_memory",json!({"about":ABOUT,"actor":"native-inspection",
        "observed_at":"2026-09-01T11:00:00Z","idempotency_key":"inspect-actions:summary",
        "search_summaries":[{"ref":refs["decision"],"summary_en":"D1 selects SQLite for the offline ledger export."}]})).await;
    let mut stale = first["next_actions"][0]["arguments"].clone();
    stale["page"]["repeat_object"] = json!(false);
    let error = request(&server, "kmp_inspect", stale.clone()).await;
    assert_eq!(error["isError"], true);
    assert_eq!(error["structuredContent"]["error"]["code"], "conflict");
    let feedback = &error["structuredContent"]["feedback"][0];
    assert_eq!(feedback["code"], "READ_SELECTION_CHANGED");
    let restart = &feedback["action"];
    assert_eq!(restart["tool"], "kmp_inspect");
    assert!(restart["arguments"].get("page").is_none());
    stale
        .as_object_mut()
        .expect("valid inspection response")
        .remove("page");
    assert_eq!(restart["arguments"], stale);
    let fresh = call(&server, "kmp_inspect", restart["arguments"].clone()).await;
    assert!(fresh.get("object_reused").is_none());
    assert_ne!(fresh["object"], first["object"]);
    assert_eq!(
        fresh["object"]["metadata"]["summary_en"],
        "D1 selects SQLite for the offline ledger export."
    );
}
