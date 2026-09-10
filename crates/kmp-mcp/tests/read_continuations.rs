//! Short actions execute the same native evidence selection and progress retry.
#[path = "support/guidance_fixture.rs"]
mod fixture;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

fn request(tool: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":args}})
}

async fn seed(server: &KernelMcpServer, context: &Value) -> Value {
    let mut args = packet(context);
    args["memories"][0]["evidence"] =
        json!("Signed S1 — 原文: the route opens on Tuesday. ".repeat(90));
    let result = call(server, "kmp_write_memory", args).await;
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"]["local_refs"]["source"].clone()
}

#[tokio::test]
async fn inspect_short_actions_reconstruct_raw_proof_negotiate_budget_and_replay() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "short-inspection").await;
    let node = seed(&server, &agent["context_id"]).await;
    let selection =
        json!({"about":ABOUT,"ref":node,"include":{"raw":true},"budget":{"max_bytes":100000}});
    let full = call(&server, "kmp_inspect", selection.clone()).await;
    assert_eq!(full["structuredContent"]["page"]["has_more"], false);
    let mut args = selection.clone();
    args["context_id"] = agent["context_id"].clone();
    args["purpose"] = json!("audit");
    args["budget"]["max_bytes"] = json!(512);
    let mut result = call(&server, "kmp_inspect", args).await;
    assert_eq!(result["structuredContent"]["page"]["returned"], 0);
    let mut evidence = Vec::new();
    let mut raw = Vec::new();
    let mut count = 0;
    loop {
        assert_eq!(result["isError"], false, "{result}");
        let body = &result["structuredContent"];
        evidence.extend(
            body["evidence"]
                .as_array()
                .expect("evidence")
                .iter()
                .cloned(),
        );
        raw.extend(body["raw"].as_array().expect("raw").iter().cloned());
        if body["page"]["has_more"] == false {
            break;
        }
        count += 1;
        assert!(count < 20, "bounded page sequence");
        let action = body["next_actions"][0].clone();
        assert_eq!(action["arguments"].as_object().expect("args").len(), 1);
        assert!(
            action["arguments"]["continuation"]
                .as_str()
                .expect("handle")
                .starts_with("read_")
        );
        assert_eq!(guidance(&result)["recommendation"]["action"], action);
        let resolved = server
            .resolve_read_request(request("kmp_inspect", action["arguments"].clone()))
            .expect("resolve");
        let bound = &resolved["params"]["arguments"];
        assert_eq!(bound["about"], selection["about"]);
        assert_eq!(bound["ref"], selection["ref"]);
        assert_eq!(bound["include"], selection["include"]);
        assert_eq!(bound["context_id"], agent["context_id"]);
        assert_eq!(bound["purpose"], "audit");
        assert_eq!(bound["page"]["cursor"], body["page"]["next_cursor"]);
        let previous_returned = body["page"]["returned"].as_u64().expect("returned");
        result = call(&server, "kmp_inspect", action["arguments"].clone()).await;
        let repeated = call(&server, "kmp_inspect", action["arguments"].clone()).await;
        assert_eq!(
            result["structuredContent"], repeated["structuredContent"],
            "retry repeats one page"
        );
        let explicit = call(&server, "kmp_inspect", bound.clone()).await;
        assert_eq!(
            result["structuredContent"], explicit["structuredContent"],
            "short and full action equivalence"
        );
        if previous_returned == 0 {
            assert!(
                result["structuredContent"]["page"]["returned"]
                    .as_u64()
                    .expect("progress")
                    > 0
            );
        }
    }
    assert!(count > 0);
    assert_eq!(json!(evidence), full["structuredContent"]["evidence"]);
    assert_eq!(json!(raw), full["structuredContent"]["raw"]);
}

#[tokio::test]
async fn handles_survive_restart_but_reject_changed_evidence_mixed_arguments_and_wrong_tools() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "short-restart").await;
    let node = seed(&server, &agent["context_id"]).await;
    let first = call(&server, "kmp_inspect", json!({"about":ABOUT,"ref":node,"context_id":agent["context_id"],"budget":{"max_bytes":512}})).await;
    let action = first["structuredContent"]["next_actions"][0].clone();
    let expected = server
        .resolve_read_request(request("kmp_inspect", action["arguments"].clone()))
        .expect("resolve");
    drop(server);
    let server = KernelMcpServer::embedded(dir.path()).expect("reconnect");
    assert_eq!(
        server
            .resolve_read_request(request("kmp_inspect", action["arguments"].clone()))
            .expect("resolve"),
        expected
    );
    assert_eq!(
        call(&server, "kmp_inspect", action["arguments"].clone()).await["isError"],
        false
    );
    let mut mixed = action["arguments"].clone();
    mixed["budget"] = json!({"max_bytes":128});
    assert_eq!(call(&server, "kmp_inspect", mixed).await["isError"], true);
    assert_eq!(
        call(&server, "kmp_wake", action["arguments"].clone()).await["isError"],
        true
    );
    assert_eq!(
        call(&server, "kmp_write_memory", action["arguments"].clone()).await["isError"],
        true
    );
    let updated = call(&server, "kmp_write_memory", json!({"about":ABOUT,"context_id":agent["context_id"],"observed_at":"2026-09-09T11:00:00Z","idempotency_key":"changed-source","search_summaries":[{"ref":node,"summary_en":"The route opens on Tuesday according to S1."}]})).await;
    assert_eq!(updated["isError"], false, "{updated}");
    let stale = call(&server, "kmp_inspect", action["arguments"].clone()).await;
    assert_eq!(
        stale["structuredContent"]["feedback"][0]["code"], "READ_SELECTION_CHANGED",
        "{stale}"
    );
    let metadata =
        rusqlite::Connection::open(dir.path().join("agent-users.sqlite3")).expect("metadata");
    metadata
        .execute("UPDATE read_continuations SET expires_at=0", [])
        .expect("expire");
    let expired = call(&server, "kmp_inspect", action["arguments"].clone()).await;
    assert_eq!(
        expired["structuredContent"]["feedback"][0]["code"],
        "CONTINUATION_UNAVAILABLE"
    );
}

#[tokio::test]
async fn failed_retention_keeps_a_complete_executable_action() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "retention-failure").await;
    let node = seed(&server, &agent["context_id"]).await;
    let metadata =
        rusqlite::Connection::open(dir.path().join("agent-users.sqlite3")).expect("metadata");
    metadata.execute_batch("CREATE TRIGGER refuse_read BEFORE INSERT ON read_continuations BEGIN SELECT RAISE(ABORT,'test retention unavailable'); END;").expect("failure injection");
    let result = call(&server, "kmp_inspect", json!({"about":ABOUT,"ref":node,"context_id":agent["context_id"],"budget":{"max_bytes":512}})).await;
    assert_eq!(result["isError"], false);
    let action = &result["structuredContent"]["next_actions"][0];
    assert_eq!(action["arguments"]["about"], ABOUT);
    assert_eq!(
        call(&server, "kmp_inspect", action["arguments"].clone()).await["isError"],
        false
    );
    assert!(
        result["content"]
            .to_string()
            .contains("could not be retained")
    );
}
