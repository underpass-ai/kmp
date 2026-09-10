//! Native optional context, identity and memory isolation.
use kmp_adapter_embedded::EmbeddedKernelStore;
#[path = "support/guidance_fixture.rs"]
mod fixture;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

#[tokio::test]
async fn context_supplies_actor_tracks_real_use_and_keeps_retries_idempotent() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "native-writer").await;
    let args = packet(&agent["context_id"]);
    // A missing first-use mark never prevents a valid operation.
    let written = call(&server, "kmp_write_memory", args.clone()).await;
    assert_eq!(written["isError"], false, "{written}");
    let advice = guidance(&written);
    assert_eq!(advice["usage"]["attempts"], 1);
    assert_eq!(advice["signals"]["accepted"], true);
    assert_eq!(advice["help"]["arguments"]["topic"], "write");
    let node = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":written["structuredContent"]["local_refs"]["source"]}),
    )
    .await;
    assert_eq!(
        node["structuredContent"]["object"]["metadata"]["writer_actor"],
        agent["agent"]["name"]
    );
    let help = &advice["help"];
    let card = call(
        &server,
        help["tool"].as_str().expect("tool"),
        help["arguments"].clone(),
    )
    .await;
    assert_eq!(card["isError"], false, "{card}");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let before_retry = store.export_bundle().await.expect("bundle");
    let replay = call(&server, "kmp_write_memory", args).await;
    assert_eq!(replay["isError"], false, "{replay}");
    assert_eq!(
        replay["structuredContent"]["receipt"],
        written["structuredContent"]["receipt"]
    );
    let advice = guidance(&replay);
    assert_eq!(advice["usage"]["attempts"], 2);
    assert_eq!(advice["help"], Value::Null);
    assert_eq!(
        before_retry,
        store.export_bundle().await.expect("after retry")
    );
    drop(store);
    drop(server);
    let resumed = KernelMcpServer::embedded(dir.path()).expect("restart");
    let context = call(
        &resumed,
        "kmp_guide",
        json!({"context_id":agent["context_id"]}),
    )
    .await;
    assert_eq!(context["structuredContent"]["agent"], agent["agent"]);
    assert_eq!(
        context["structuredContent"]["used"],
        json!([{"tool":"kmp_write_memory","attempts":2,"rejected":0,"unknown":0}])
    );
}

#[tokio::test]
async fn invalid_context_cannot_write_and_optional_guidance_preserves_the_memory_packet() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "selection-reader").await;
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let before = store.export_bundle().await.expect("before");
    let invalid = call(
        &server,
        "kmp_write_memory",
        packet(&json!("context_ffffffffffffffffffffffffffffffff")),
    )
    .await;
    assert_eq!(invalid["isError"], true);
    assert_eq!(
        invalid["structuredContent"]["error"]["code"],
        "invalid_argument"
    );
    assert_eq!(before, store.export_bundle().await.expect("unchanged"));
    let mut args = packet(&agent["context_id"]);
    args["actor"] = json!("explicit-writer");
    let written = call(&server, "kmp_write_memory", args).await;
    assert_eq!(written["isError"], false, "{written}");
    let node = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":written["structuredContent"]["local_refs"]["source"]}),
    )
    .await;
    assert_eq!(
        node["structuredContent"]["object"]["metadata"]["writer_actor"],
        "explicit-writer"
    );
    let mut selection = json!({"about":ABOUT,"axis":"observed","interval":{"start":"2026-09-01T00:00:00Z","end":"2026-09-10T00:00:00Z"},"budget":{"max_bytes":2048}});
    let bare = call(&server, "kmp_wake", selection.clone()).await;
    selection["context_id"] = agent["context_id"].clone();
    let contextual = call(&server, "kmp_wake", selection).await;
    assert_eq!(contextual["isError"], false, "{contextual}");
    assert_eq!(bare["structuredContent"], contextual["structuredContent"]);
    assert_eq!(bare["content"][0], contextual["content"][0]);
    guidance(&contextual);
}

#[tokio::test]
async fn guide_reads_record_returned_bodies_without_recursive_help() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "guide-reader").await;
    let card = call(
        &server,
        "kmp_guide",
        json!({"context_id":agent["context_id"],"topic":"write"}),
    )
    .await;
    let action = &card["structuredContent"]["next_actions"][0];
    assert_eq!(action["arguments"]["context_id"], agent["context_id"]);
    let body = call(
        &server,
        action["tool"].as_str().expect("tool"),
        action["arguments"].clone(),
    )
    .await;
    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(guidance(&body)["help"], Value::Null);
    let context = call(
        &server,
        "kmp_guide",
        json!({"context_id":agent["context_id"]}),
    )
    .await;
    assert_eq!(
        context["structuredContent"]["served"],
        json!(["guide:kmp-agent:verb:write", "write"])
    );
    assert_eq!(context["structuredContent"]["expanded"], json!(["write"]));
    assert_eq!(context["structuredContent"]["used"][0]["attempts"], 1);
}

#[tokio::test]
async fn an_advisory_storage_failure_cannot_reverse_an_accepted_write() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "metadata-failure").await;
    let metadata =
        rusqlite::Connection::open(dir.path().join("agent-users.sqlite3")).expect("metadata");
    metadata.execute_batch("CREATE TRIGGER refuse_usage BEFORE INSERT ON uses BEGIN SELECT RAISE(ABORT, 'test usage unavailable'); END;").expect("failure injection");
    let written = call(&server, "kmp_write_memory", packet(&agent["context_id"])).await;
    assert_eq!(written["isError"], false, "{written}");
    assert_eq!(written["structuredContent"]["accepted"], true);
    assert!(written["structuredContent"]["receipt"].is_object());
    assert_eq!(guidance(&written)["usage"]["recorded"], false);
    let inspected = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":written["structuredContent"]["local_refs"]["source"]}),
    )
    .await;
    assert_eq!(inspected["isError"], false, "{inspected}");
    assert_eq!(
        inspected["structuredContent"]["object"]["text"],
        "The route opens on Tuesday."
    );
}
