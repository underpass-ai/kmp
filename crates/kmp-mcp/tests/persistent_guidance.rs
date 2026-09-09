use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn call(server: &KernelMcpServer, tool: &str, args: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":args}});
    let line = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    serde_json::from_str::<Value>(&line).expect("json")["result"].clone()
}

async fn success(server: &KernelMcpServer, args: Value) -> Value {
    let result = call(server, "kmp_guide", args).await;
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}

async fn sync(server: &KernelMcpServer, revision: Option<&str>) {
    let mut requests: Vec<Value> = serde_json::from_str(include_str!(
        "../../../plugins/kmp/guide/guide.requests.json"
    ))
    .expect("assets");
    for request in &mut requests {
        if let Some(revision) = revision {
            request["idempotency_key"] = json!(format!(
                "guide-update:{}:{revision}",
                request["about"].as_str().expect("about")
            ));
            for node in request["memory"]["entries"].as_array_mut().expect("nodes") {
                node["metadata"]["guide_revision"] = json!(revision);
            }
        }
        let result = call(server, "kmp_ingest", request.clone()).await;
        assert_eq!(result["isError"], false, "{result}");
    }
}

#[tokio::test]
async fn native_identity_survives_restart_and_guidance_does_not_enter_memory() {
    let dir = tempfile::tempdir().expect("store");
    let first = {
        let server = KernelMcpServer::embedded(dir.path()).expect("server");
        sync(&server, None).await;
        let store = EmbeddedKernelStore::open(dir.path()).expect("inspect store");
        let before = store.export_bundle().await.expect("before");
        let first = success(&server, json!({"registration_key":"native-agent-a"})).await;
        assert_eq!(first["durable"], true);
        assert_eq!(first["scheme"].as_array().expect("scheme").len(), 8);
        let replay = success(&server, json!({"registration_key":"native-agent-a"})).await;
        assert_eq!(first["agent"], replay["agent"]);
        assert_eq!(first["context_id"], replay["context_id"]);
        let opened = success(
            &server,
            json!({"context_id":first["context_id"],"topic":"write"}),
        )
        .await;
        assert!(
            opened["card"]["text"]
                .as_str()
                .expect("card")
                .contains("idempotency_key")
        );
        assert_eq!(opened["served"], json!(["write"]));
        let action = &opened["next_actions"][0];
        let extended = call(
            &server,
            action["tool"].as_str().expect("tool"),
            action["arguments"].clone(),
        )
        .await;
        assert_eq!(extended["isError"], false);
        let folded = success(
            &server,
            json!({"context_id":first["context_id"],"topic":"write","fold":true}),
        )
        .await;
        assert_eq!(folded["expanded"], json!([]));
        assert_eq!(folded["served"], json!(["write"]));
        assert_eq!(before, store.export_bundle().await.expect("after"));
        first
    };
    let restarted = KernelMcpServer::embedded(dir.path()).expect("restart");
    let resumed = success(&restarted, json!({"context_id":first["context_id"]})).await;
    assert_eq!(resumed["agent"], first["agent"]);
    assert_eq!(resumed["served"], json!(["write"]));
    assert_eq!(resumed["expanded"], json!([]));
}

#[tokio::test]
async fn shared_mcp_connection_distinguishes_agents_and_context_resets() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    sync(&server, None).await;
    let a = success(&server, json!({"registration_key":"agent-a"})).await;
    let b = success(&server, json!({"registration_key":"agent-b"})).await;
    assert_ne!(a["agent"], b["agent"]);
    assert_ne!(a["context_id"], b["context_id"]);
    success(
        &server,
        json!({"context_id":a["context_id"],"topic":"time"}),
    )
    .await;
    let b = success(&server, json!({"context_id":b["context_id"]})).await;
    assert_eq!(b["served"], json!([]));
    let reset = json!({"agent_id":a["agent"]["id"],"context_key":"compaction-1"});
    let fresh = success(&server, reset.clone()).await;
    assert_eq!(fresh["agent"], a["agent"]);
    assert_ne!(fresh["context_id"], a["context_id"]);
    assert_eq!(fresh["served"], json!([]));
    assert_eq!(
        fresh["context_id"],
        success(&server, reset).await["context_id"]
    );
    assert_eq!(
        success(&server, json!({"context_id":a["context_id"]})).await["served"],
        json!(["time"])
    );
}

#[tokio::test]
async fn changed_guide_is_rediscovered_and_invalid_identity_does_not_register() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    sync(&server, None).await;
    let a = success(&server, json!({"registration_key":"agent-a","topic":"ask"})).await;
    sync(&server, Some("next-guide-revision")).await;
    let changed = success(&server, json!({"context_id":a["context_id"]})).await;
    assert_eq!(changed["guide_changed"], true);
    assert_eq!(changed["agent"], a["agent"]);
    assert_eq!(changed["served"], json!([]));
    let invalid = call(
        &server,
        "kmp_guide",
        json!({"context_id":"context_ffffffffffffffffffffffffffffffff"}),
    )
    .await;
    assert_eq!(invalid["isError"], true);
    assert_eq!(
        invalid["structuredContent"]["error"]["code"],
        "invalid_argument"
    );
    let card = success(&server, json!({"context_id":a["context_id"],"topic":"ask"})).await;
    assert_eq!(card["served"], json!(["ask"]));
    assert_eq!(card["guide_revision"], "next-guide-revision");
}

#[tokio::test]
async fn scheme_cards_link_to_live_verbs_and_their_json_examples_execute() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    sync(&server, None).await;
    let agent = success(&server, json!({"registration_key":"card-execution"})).await;
    // Start with the write card so the later examples have actual memory.
    for topic in [
        "write", "wake", "ask", "time", "audit", "relate", "view", "guide",
    ] {
        let card = success(
            &server,
            json!({"context_id":agent["context_id"],"topic":topic}),
        )
        .await;
        for action in card["next_actions"].as_array().expect("extended actions") {
            let result = call(
                &server,
                action["tool"].as_str().expect("tool"),
                action["arguments"].clone(),
            )
            .await;
            assert_eq!(result["isError"], false, "{topic}: {result}");
        }
        let text = card["card"]["text"].as_str().expect("card text");
        if let Some(tool) = match topic {
            "write" => Some("kmp_write_memory"),
            "ask" => Some("kmp_ask"),
            "relate" => Some("kmp_relate"),
            _ => None,
        } {
            let example = text
                .split("```json\n")
                .nth(1)
                .expect("worked JSON")
                .split("```")
                .next()
                .expect("end");
            let result = call(
                &server,
                tool,
                serde_json::from_str(example.trim()).expect("JSON example"),
            )
            .await;
            assert_eq!(result["isError"], false, "{topic}: {result}");
            if topic == "write" {
                // Relate's example assumes both named abouts already exist.
                let mut other: Value = serde_json::from_str(example.trim()).expect("source");
                other["about"] = json!("project:other");
                other["idempotency_key"] = json!("fixture-other");
                let result = call(&server, tool, other).await;
                assert_eq!(result["isError"], false, "{result}");
            }
        }
    }
}
