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

fn work_guidance(result: &Value) -> Value {
    let blocks = result["content"]
        .as_array()
        .expect("content")
        .iter()
        .filter_map(|item| serde_json::from_str::<Value>(item["text"].as_str()?).ok())
        .filter_map(|body| body.get("kmp_guidance").cloned())
        .collect::<Vec<_>>();
    assert_eq!(blocks.len(), 1, "exactly one guidance block: {result}");
    blocks.into_iter().next().expect("guidance")
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
        assert_eq!(first["scheme"].as_array().expect("scheme").len(), 9);
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
        assert_eq!(
            folded["served"],
            json!(["guide:kmp-agent:verb:write", "write"])
        );
        assert_eq!(before, store.export_bundle().await.expect("after"));
        first
    };
    let restarted = KernelMcpServer::embedded(dir.path()).expect("restart");
    let resumed = success(&restarted, json!({"context_id":first["context_id"]})).await;
    assert_eq!(resumed["agent"], first["agent"]);
    assert_eq!(
        resumed["served"],
        json!(["guide:kmp-agent:verb:write", "write"])
    );
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
async fn condense_help_opens_a_reusable_topic_and_records_work_across_fold() {
    const ABOUT: &str = "project:condense-guidance";
    const SOURCE: &str = "project:condense-guidance:source";
    const TARGET: &str = "project:condense-guidance:target";

    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    sync(&server, None).await;
    let agent = success(&server, json!({"registration_key":"condense-reader"})).await;
    assert!(agent["scheme"].as_array().expect("scheme").iter().any(
        |row| row == &json!({"topic":"condense","purpose":"Write a compact card for one stored body"})
    ));

    let seeded = call(
        &server,
        "kmp_ingest",
        json!({
            "about": ABOUT,
            "idempotency_key": "condense-guidance:seed",
            "memory": {
                "dimensions": [{"id": "conversation:condense-guidance", "kind": "conversation"}],
                "entries": [
                    {"id": SOURCE, "kind": "observation", "text": "A long source body whose repeated delivery is worth replacing with a concise reader card.",
                     "coordinates": [{"dimension":"conversation","scope_id":"conversation:condense-guidance","sequence":1}]},
                    {"id": TARGET, "kind": "claim", "text": "The target depends on the source.",
                     "coordinates": [{"dimension":"conversation","scope_id":"conversation:condense-guidance","sequence":2}]}
                ],
                "relations": [{
                    "from": SOURCE, "to": TARGET, "rel": "supports",
                    "class": "evidential", "confidence": "high",
                    "why": "The source directly supports the target.",
                    "evidence": "Native condense guidance fixture."
                }]
            }
        }),
    )
    .await;
    assert_eq!(seeded["isError"], false, "{seeded}");
    let traced = call(
        &server,
        "kmp_trace",
        json!({
            "about": ABOUT, "from": SOURCE, "to": TARGET,
            "search": {"proof": true, "proof_refs": []},
            "budget": {"max_bytes": 40000}
        }),
    )
    .await;
    assert_eq!(traced["isError"], false, "{traced}");
    let descriptor = traced["structuredContent"]["objects"]
        .as_array()
        .expect("proof objects")
        .iter()
        .find(|object| object["ref"] == SOURCE)
        .expect("source descriptor")["descriptor"]
        .clone();
    let mut condense = json!({
        "about": ABOUT,
        "ref": SOURCE,
        "language": "en",
        "scope": "node_body",
        "card": "Source supports target.",
        "source": {
            "revision": descriptor["revision"],
            "record_digest": descriptor["record_digest"]
        },
        "expect": {"absent": true},
        "context_id": agent["context_id"]
    });
    let first_work = call(&server, "kmp_condense", condense.clone()).await;
    assert_eq!(first_work["isError"], false, "{first_work}");
    assert_eq!(
        first_work["structuredContent"]["card"]["authored_by"],
        agent["agent"]["name"]
    );
    let first_guidance = work_guidance(&first_work);
    assert_eq!(first_guidance["usage"]["attempts"], 1);
    assert_eq!(first_guidance["topic_served_in_context"], false);
    assert_eq!(first_guidance["help"]["tool"], "kmp_guide");
    assert_eq!(first_guidance["help"]["arguments"]["topic"], "condense");

    let opened = success(&server, first_guidance["help"]["arguments"].clone()).await;
    assert_eq!(opened["card"]["ref"], "guide:kmp-agent:card:condense");
    assert_eq!(opened["expanded"], json!(["condense"]));
    let extended = call(
        &server,
        opened["next_actions"][0]["tool"].as_str().expect("tool"),
        opened["next_actions"][0]["arguments"].clone(),
    )
    .await;
    assert_eq!(extended["isError"], false, "{extended}");

    condense["card"] = json!("Source supports the target claim.");
    condense["expect"] = json!({
        "card_revision": first_work["structuredContent"]["card"]["card_revision"]
    });
    let repeated_work = call(&server, "kmp_condense", condense).await;
    assert_eq!(repeated_work["isError"], false, "{repeated_work}");
    let repeated_guidance = work_guidance(&repeated_work);
    assert_eq!(repeated_guidance["usage"]["attempts"], 2);
    assert_eq!(repeated_guidance["topic_served_in_context"], true);
    assert!(repeated_guidance["help"].is_null());

    let repeated_topic = success(
        &server,
        json!({"context_id":agent["context_id"],"topic":"condense"}),
    )
    .await;
    assert_eq!(repeated_topic["card"], opened["card"]);
    let folded = success(
        &server,
        json!({"context_id":agent["context_id"],"topic":"condense","fold":true}),
    )
    .await;
    assert_eq!(folded["expanded"], json!([]));
    assert_eq!(
        folded["served"],
        json!(["condense", "guide:kmp-agent:verb:condense"])
    );
    assert_eq!(
        folded["used"],
        json!([{"tool":"kmp_condense","attempts":2,"rejected":0,"unknown":0},
               {"tool":"kmp_inspect","attempts":1,"rejected":0,"unknown":0}])
    );
    drop(server);
    let restarted = KernelMcpServer::embedded(dir.path()).expect("restart");
    let resumed = success(&restarted, json!({"context_id":agent["context_id"]})).await;
    assert_eq!(resumed["expanded"], folded["expanded"]);
    assert_eq!(resumed["served"], folded["served"]);
    assert_eq!(resumed["used"], folded["used"]);
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
        "write", "wake", "ask", "time", "audit", "condense", "relate", "view", "guide",
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

#[tokio::test]
async fn observing_a_new_guide_body_starts_a_new_delivery_and_usage_view() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    sync(&server, None).await;
    let agent = success(&server, json!({"registration_key":"revision-reader"})).await;
    let prior = call(
        &server,
        "kmp_wake",
        json!({"about":"guide:kmp-agent","context_id":agent["context_id"]}),
    )
    .await;
    assert_eq!(prior["isError"], false);
    sync(&server, Some("inspected-revision")).await;
    let body = call(&server, "kmp_inspect", json!({"about":"guide:kmp-agent","ref":"guide:kmp-agent:verb:write","context_id":agent["context_id"]})).await;
    assert_eq!(body["isError"], false, "{body}");
    let context = success(&server, json!({"context_id":agent["context_id"]})).await;
    assert_eq!(context["agent"], agent["agent"]);
    assert_eq!(context["guide_revision"], "inspected-revision");
    assert_eq!(context["served"], json!(["guide:kmp-agent:verb:write"]));
    assert_eq!(
        context["used"],
        json!([{"tool":"kmp_inspect","attempts":1,"rejected":0,"unknown":0}])
    );
    let metadata =
        rusqlite::Connection::open(dir.path().join("agent-users.sqlite3")).expect("metadata");
    let retained: i64 = metadata
        .query_row(
            "SELECT attempts FROM uses WHERE context_id=?1 AND revision=?2 AND tool='kmp_wake'",
            rusqlite::params![
                agent["context_id"].as_str().expect("context"),
                agent["guide_revision"].as_str().expect("revision")
            ],
            |row| row.get(0),
        )
        .expect("old usage");
    assert_eq!(retained, 1);
}
