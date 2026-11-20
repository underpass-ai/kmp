//! `kmp_guide` is a stateless topic lookup served from the engine itself, and
//! a first session needs nothing from it to wake and write.
#[path = "support/route_fixture.rs"]
mod fixture;
#[path = "support/write_neighborhood_fixture.rs"]
mod neighborhood;
use fixture::{ABOUT, call};
use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_domain::TokenEstimator;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};
use std::path::Path;

/// A card fits in a host's context next to the work: about 2,000 tokens for
/// the whole `tools/call` result as the host receives it.
const CARD_TOKEN_BUDGET: u32 = 2_000;

fn topics() -> Vec<String> {
    let listed = kmp_mcp_tools();
    let guide = listed
        .iter()
        .find(|tool| tool["name"] == "kmp_guide")
        .expect("kmp_guide is listed");
    guide["inputSchema"]["properties"]["topic"]["enum"]
        .as_array()
        .expect("topic enum")
        .iter()
        .map(|topic| topic.as_str().expect("topic").to_string())
        .collect()
}

fn kmp_mcp_tools() -> Vec<Value> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let server = KernelMcpServer::fixture();
        let reply = server
            .handle_json_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
            .await
            .expect("tools");
        let reply: Value = serde_json::from_str(&reply).expect("JSON");
        reply["result"]["tools"].as_array().expect("tools").clone()
    })
}

async fn initialize(server: &KernelMcpServer, client: &str) {
    let request = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{
        "protocolVersion":"2025-06-18","capabilities":{},
        "clientInfo":{"name":client,"version":"1"}}});
    server
        .handle_json_line(&request.to_string())
        .await
        .expect("initialize");
}

/// Every file the store directory holds, to show a lookup wrote nothing.
fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("store dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn every_card_is_served_by_the_engine_within_its_budget() {
    let topics = topics();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        // An empty store and no plugin tree: the fixture backend reads nothing.
        let server = KernelMcpServer::fixture();
        let estimator = Cl100kEstimator::shared();
        let index = call(&server, "kmp_guide", json!({})).await;
        assert_eq!(index["isError"], false, "{index}");
        let listed: Vec<&str> = index["structuredContent"]["topics"]
            .as_array()
            .expect("topics")
            .iter()
            .map(|row| row["topic"].as_str().expect("topic"))
            .collect();
        assert_eq!(listed, topics);
        for topic in &topics {
            let card = call(&server, "kmp_guide", json!({"topic":topic})).await;
            assert_eq!(card["isError"], false, "{topic}: {card}");
            let body = &card["structuredContent"];
            assert_eq!(body["topic"], json!(topic));
            assert!(!body["text"].as_str().expect("text").trim().is_empty());
            assert!(
                body["parameters"]
                    .as_object()
                    .is_some_and(|tools| !tools.is_empty()),
                "{topic} explains at least one tool"
            );
            let tokens = estimator.estimate_tokens(&card.to_string());
            assert!(
                tokens <= CARD_TOKEN_BUDGET,
                "kmp_guide topic {topic} is {tokens} tokens, over {CARD_TOKEN_BUDGET}"
            );
        }
        let relations = call(&server, "kmp_guide", json!({"topic":"relations"})).await;
        assert!(relations["structuredContent"]["relation_vocabulary"].is_array());
    });
}

#[test]
fn compiled_cards_match_the_editorial_sources() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let editorial = manifest.join("../../plugins/kmp/guide/cards");
    let compiled = manifest.join("src/guide/cards");
    for topic in topics() {
        let source = std::fs::read_to_string(editorial.join(format!("{topic}.md")))
            .unwrap_or_else(|error| panic!("{topic}: editorial card: {error}"));
        let shipped = std::fs::read_to_string(compiled.join(format!("{topic}.md")))
            .unwrap_or_else(|error| panic!("{topic}: compiled card: {error}"));
        assert_eq!(
            shipped, source,
            "copy plugins/kmp/guide/cards/{topic}.md to crates/kmp-mcp/src/guide/cards/"
        );
    }
}

#[tokio::test]
async fn the_lookup_reads_and_writes_no_store_and_refuses_session_arguments() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let before = files(dir.path());
    let card = call(&server, "kmp_guide", json!({"topic":"write"})).await;
    assert_eq!(card["isError"], false, "{card}");
    assert_eq!(files(dir.path()), before, "a lookup writes nothing");
    for removed in [
        json!({"registration_key":"agent"}),
        json!({"context_id":"context_00000000000000000000000000000000"}),
        json!({"topic":"write","fold":true}),
    ] {
        let refused = call(&server, "kmp_guide", removed.clone()).await;
        assert_eq!(refused["isError"], true, "{removed}: {refused}");
    }
    for (tool, arguments) in [
        (
            "kmp_wake",
            json!({"about":ABOUT,"context_id":"context_00000000000000000000000000000000"}),
        ),
        (
            "kmp_ask",
            json!({"about":ABOUT,"question":"q","purpose":"answer"}),
        ),
    ] {
        let refused = call(&server, tool, arguments).await;
        assert_eq!(refused["isError"], true, "{tool}: {refused}");
        assert_eq!(
            refused["structuredContent"]["error"]["code"],
            "invalid_argument"
        );
    }
}

#[tokio::test]
async fn a_first_wake_answers_and_a_write_needs_no_guide_call() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    initialize(&server, "first-session-host").await;
    let wake = call(&server, "kmp_wake", json!({"about":ABOUT})).await;
    assert_eq!(wake["isError"], false, "{wake}");
    assert!(
        wake["structuredContent"]["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains(ABOUT) && summary.contains("Nothing")),
        "{wake}"
    );
    let mut packet = fixture::packet();
    packet.as_object_mut().expect("packet").remove("actor");
    let written = call(&server, "kmp_write_memory", packet.clone()).await;
    assert_eq!(written["isError"], false, "{written}");
    assert_eq!(written["structuredContent"]["accepted"], true, "{written}");
    // The default writer is part of the packet: a retry replays it.
    let replayed = call(&server, "kmp_write_memory", packet).await;
    assert_eq!(
        replayed["structuredContent"]["status"], "replayed",
        "{replayed}"
    );
    let source = &written["structuredContent"]["local_refs"]["source"];
    let raw = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":source,"include":{"raw":true}}),
    )
    .await;
    assert!(
        raw.to_string().contains("first-session-host"),
        "the writer is the MCP client: {raw}"
    );
    let wake = call(&server, "kmp_wake", json!({"about":ABOUT})).await;
    assert_eq!(wake["isError"], false, "{wake}");
    assert_ne!(wake["structuredContent"]["summary"], json!(null));
}

#[tokio::test]
async fn a_continuation_accepts_the_value_it_is_bound_to_and_refuses_another() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut packet = fixture::packet();
    packet["memories"][0]["evidence"] = json!("S1 records the route opening. ".repeat(90));
    let written = call(&server, "kmp_write_memory", packet).await;
    assert_eq!(written["isError"], false, "{written}");
    let source = written["structuredContent"]["local_refs"]["source"].clone();
    let first = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":source,"include":{"raw":true},"budget":{"max_bytes":512}}),
    )
    .await;
    let handle = first["structuredContent"]["next_actions"][0]["arguments"]["continuation"].clone();
    assert!(handle.is_string(), "{first}");
    let same = call(
        &server,
        "kmp_inspect",
        json!({"continuation":handle,"about":ABOUT}),
    )
    .await;
    assert_eq!(same["isError"], false, "{same}");
    let other = call(
        &server,
        "kmp_inspect",
        json!({"continuation":handle,"about":"project:other"}),
    )
    .await;
    assert_eq!(other["isError"], true, "{other}");
    let message = other["structuredContent"]["error"]["message"]
        .as_str()
        .expect("message");
    assert!(message.contains("bound to about"), "{message}");
    let extra = call(
        &server,
        "kmp_inspect",
        json!({"continuation":handle,"budget":{"max_bytes":4096}}),
    )
    .await;
    assert_eq!(extra["isError"], true, "never silently drop an argument");

    // A write resuming its review takes the same actor beside the handle.
    let pending = call(&server, "kmp_write_memory", neighborhood::packet()).await;
    let body = &pending["structuredContent"];
    assert_eq!(body["status"], "needs_review", "{pending}");
    let handle = body["next_actions"][0]["arguments"]["continuation"].clone();
    let wrong = call(
        &server,
        "kmp_write_memory",
        json!({"continuation":handle,"actor":"someone-else"}),
    )
    .await;
    assert_eq!(wrong["isError"], true, "{wrong}");
    let resumed = call(
        &server,
        "kmp_write_memory",
        json!({"continuation":handle,"actor":"writer","about":neighborhood::ABOUT}),
    )
    .await;
    assert_eq!(resumed["isError"], false, "{resumed}");
    assert_eq!(
        resumed["structuredContent"]["status"], "committed",
        "{resumed}"
    );
}

#[tokio::test]
async fn a_0_27_agent_directory_beside_the_store_is_left_alone() {
    let dir = tempfile::tempdir().expect("store");
    let legacy = dir.path().join("agent-users.sqlite3");
    {
        let connection = rusqlite::Connection::open(&legacy).expect("legacy");
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE agents (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, registration_key TEXT NOT NULL UNIQUE);
                 CREATE TABLE contexts (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL REFERENCES agents(id), context_key TEXT NOT NULL, revision TEXT NOT NULL);
                 CREATE TABLE read_continuations (id TEXT PRIMARY KEY, context_id TEXT NOT NULL REFERENCES contexts(id), tool TEXT NOT NULL, arguments TEXT NOT NULL, fingerprint TEXT NOT NULL, expires_at INTEGER NOT NULL);
                 CREATE TABLE open_continuations (id TEXT PRIMARY KEY, tool TEXT NOT NULL, arguments TEXT NOT NULL, fingerprint TEXT NOT NULL UNIQUE, expires_at INTEGER NOT NULL);
                 INSERT INTO agents VALUES ('agent_1','traveler-1','key');
                 INSERT INTO contexts VALUES ('context_1','agent_1','registration','r1');
                 INSERT INTO open_continuations VALUES ('read_0123456789abcdef0123456789abcdef','kmp_wake','{}','f',unixepoch()+86400);
                 PRAGMA user_version=3;",
            )
            .expect("legacy schema");
    }
    let bytes = std::fs::read(&legacy).expect("legacy bytes");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut packet = fixture::packet();
    packet["memories"][0]["evidence"] = json!("S1 records the route opening. ".repeat(90));
    let written = call(&server, "kmp_write_memory", packet).await;
    let source = written["structuredContent"]["local_refs"]["source"].clone();
    let first = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":source,"budget":{"max_bytes":512}}),
    )
    .await;
    let action = first["structuredContent"]["next_actions"][0].clone();
    let next = call(&server, "kmp_inspect", action["arguments"].clone()).await;
    assert_eq!(next["isError"], false, "{next}");
    // A handle a 0.27.0 server retained is not resolved: it was its state.
    let old = call(
        &server,
        "kmp_wake",
        json!({"continuation":"read_0123456789abcdef0123456789abcdef"}),
    )
    .await;
    assert_eq!(
        old["structuredContent"]["feedback"][0]["code"],
        "CONTINUATION_UNAVAILABLE"
    );
    assert_eq!(std::fs::read(&legacy).expect("legacy bytes"), bytes);
    assert!(dir.path().join("continuations.sqlite3").is_file());
}
