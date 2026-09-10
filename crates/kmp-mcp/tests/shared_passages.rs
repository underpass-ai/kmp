//! Opt-in display sharing cannot alter native evidence, paging or authorization.
#[path = "support/shared_dependency_checks.rs"]
mod dependency_checks;
#[path = "support/guidance_fixture.rs"]
mod fixture;
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use kmp_proto_mapping::context_projection::expand_packet;
use serde_json::{Value, json};

async fn rpc(server: &KernelMcpServer, method: &str) -> Value {
    let result = server
        .handle_json_line(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":{}}).to_string())
        .await
        .expect("valid native fixture");
    serde_json::from_str::<Value>(&result).expect("valid native fixture")["result"].clone()
}

#[tokio::test]
async fn only_the_opted_in_host_advertises_shared_prose_and_input_calls_do_not_change() {
    let server = KernelMcpServer::fixture();
    let inline = rpc(&server, "tools/list").await;
    let server = server.with_shared_passages(true);
    let shared = rpc(&server, "tools/list").await;
    assert_eq!(
        inline["tools"].as_array().expect("array").len(),
        shared["tools"].as_array().expect("array").len()
    );
    for (a, b) in inline["tools"]
        .as_array()
        .expect("valid native fixture")
        .iter()
        .zip(shared["tools"].as_array().expect("array"))
    {
        assert_eq!(a["inputSchema"], b["inputSchema"]);
        if b["outputSchema"]["properties"].get("passages").is_some() {
            assert!(!b["name"].as_str().expect("string").contains("write"));
            assert!(!b["name"].as_str().expect("string").contains("view"));
        }
    }
    assert!(
        rpc(&server, "initialize").await["instructions"]
            .as_str()
            .expect("valid native fixture")
            .contains("same response")
    );
    let error = call(
        &server,
        "kmp_inspect",
        json!({"about":"missing","ref":"absent","unsupported":true}),
    )
    .await;
    assert_eq!(error["isError"], true);
    assert!(error["structuredContent"].get("passages").is_none());
}

#[tokio::test]
async fn all_nine_native_read_walks_match_inline_packets_after_expansion() {
    let dir = tempfile::tempdir().expect("valid native fixture");
    let mut server = KernelMcpServer::embedded(dir.path()).expect("valid native fixture");
    let agent = open(&server, "shared-passage-reads").await;
    let mut write = packet(&agent["context_id"]);
    write["memories"][0]["evidence"]=json!("S1 records the route opening on Tuesday, subject to explicit permission; R8 is not included. ".repeat(30));
    write["memories"].as_array_mut().expect("mutable array").push(json!({"id":"decision","kind":"decision","observed_at":"2026-09-09T11:00:00Z","summary":"Schedule delivery on Tuesday.","evidence":"S2 schedules delivery to match S1 route opening. ".repeat(30),"connect_to":[{"ref":"source","rel":"chosen_because","class":"motivational","why":"The route opening motivates Tuesday delivery.","evidence":"S2 explicitly chooses Tuesday because S1 records the route opening. ".repeat(20)}]}));
    let receipt = call(&server, "kmp_write_memory", write.clone()).await;
    let receipt = reviewed_writer::review_authored_write(&server, receipt).await;
    assert_eq!(receipt["isError"], false, "{receipt}");
    let refs = &receipt["structuredContent"]["local_refs"];
    let mut queries = vec![
        (
            "kmp_inspect",
            json!({"about":ABOUT,"ref":refs["source"],"include":{"raw":true}}),
        ),
        ("kmp_wake", json!({"about":ABOUT})),
        (
            "kmp_ask",
            json!({"about":ABOUT,"question":"When does the route open?"}),
        ),
        (
            "kmp_trace",
            json!({"about":ABOUT,"from":refs["decision"],"to":refs["source"]}),
        ),
        ("kmp_relate", json!({"about":ABOUT})),
    ];
    for (tool, key, cursor) in [
        (
            "kmp_forward",
            "from",
            json!({"time":"2026-09-09T00:00:00Z"}),
        ),
        ("kmp_rewind", "from", json!({"time":"2026-09-10T00:00:00Z"})),
        ("kmp_goto", "at", json!({"ref":refs["source"]})),
        ("kmp_near", "around", json!({"ref":refs["source"]})),
    ] {
        let mut args = json!({"about":ABOUT,"axis":"observed","include":{"evidence":true,"relations":true,"raw_refs":true}});
        args[key] = cursor;
        queries.push((tool, args));
    }
    let mut encoded_pages = 0;
    for (tool, mut args) in queries {
        args["context_id"] = agent["context_id"].clone();
        args["budget"] = if tool == "kmp_inspect" {
            json!({"max_bytes":512})
        } else {
            json!({"max_bytes":512,"detail":"full"})
        };
        for page in 0..100 {
            server = server.with_shared_passages(false);
            let inline = call(&server, tool, args.clone()).await;
            assert_eq!(inline["isError"], false, "{tool}: {inline}");
            server = server.with_shared_passages(true);
            let shared = call(&server, tool, args.clone()).await;
            assert_eq!(shared["isError"], false, "{tool}: {shared}");
            let body = &shared["structuredContent"];
            assert!(body.to_string().len() <= inline["structuredContent"].to_string().len());
            encoded_pages += usize::from(body.get("passages").is_some());
            let mut expanded = expand_packet(body.clone()).expect("valid native fixture");
            if expanded.pointer("/projection/budget/used_bytes").is_some() {
                assert_eq!(
                    body["projection"]["budget"]["used_bytes"],
                    body.to_string().len()
                );
                expanded["projection"]["budget"]["used_bytes"] =
                    inline["structuredContent"]["projection"]["budget"]["used_bytes"].clone();
            }
            assert_eq!(expanded, inline["structuredContent"], "{tool} page {page}");
            assert_eq!(
                guidance(&shared)["recommendation"]["action"],
                guidance(&inline)["recommendation"]["action"]
            );
            let action = if body.get("projection").is_some() {
                body["projection"]["next_action"].clone()
            } else {
                body["next_actions"][0].clone()
            };
            if action.is_null() {
                break;
            }
            assert!(page < 99, "finite walk");
            args = action["arguments"].clone();
        }
    }
    assert!(
        encoded_pages > 0,
        "the real native corpus exercises sharing"
    );
    // Display reads have not changed the immutable receipt or committed again.
    let replay = call(&server, "kmp_write_memory", write).await;
    assert_eq!(replay["structuredContent"]["status"], "replayed");
    assert_eq!(
        replay["structuredContent"]["local_refs"],
        receipt["structuredContent"]["local_refs"]
    );
    assert!(replay["structuredContent"].get("passages").is_none());
}
