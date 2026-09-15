//! A local lexical control for the fixed #538 multi-passage fixture.
//!
//! This is deliberately narrower than an external memory-system or reader
//! evaluation. Both arms receive the same frozen question, source passages,
//! observed cut and source-body allowance. It records lexical retrieval only;
//! the repository's separate native route controls exercise expansion after a
//! route has been deliberately selected.

use std::{cmp::Ordering, collections::BTreeSet, time::Instant};

use kmp_mcp::KernelMcpServer;
use rusqlite::{Connection, params};
use serde_json::{Value, json};

const ABOUT: &str = "project:retrieval-support";
const SOURCE: &str = include_str!("fixtures/proof_support.json");
const CUT: &str = "2026-09-10T10:00:00Z";
const SOURCE_BODY_BUDGET: usize = 512;
const NATIVE_RESPONSE_BUDGET: usize = 200_000;
// Frozen before either arm runs. The FTS5 expression is every token in the
// question, quoted and joined with standard SQLite OR syntax.
const QUESTION: &str = "Who is accountable for R19 in Alba at the boundary?";
const FTS5_PROBE: &str = "\"Who\" OR \"is\" OR \"accountable\" OR \"for\" OR \"R19\" OR \"in\" OR \"Alba\" OR \"at\" OR \"the\" OR \"boundary\"";
const REQUIRED: [&str; 3] = ["alias", "role", "boundary"];

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let reply = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":tool,"arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("native response");
    let result = serde_json::from_str::<Value>(&reply).expect("JSON")["result"].clone();
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}

async fn seed(server: &KernelMcpServer) -> Value {
    let pending = call(
        server,
        "kmp_write_memory",
        serde_json::from_str(SOURCE).expect("source fixture"),
    )
    .await;
    assert_eq!(pending["status"], "needs_review", "{pending}");
    let next = &pending["next_actions"][0];
    let committed = call(server, "kmp_write_memory", next["arguments"].clone()).await;
    assert_eq!(committed["status"], "committed", "{committed}");
    committed["local_refs"].clone()
}

fn source_memory(id: &str) -> Value {
    serde_json::from_str::<Value>(SOURCE).expect("source fixture")["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .find(|memory| memory["id"] == id)
        .expect("known source memory")
        .clone()
}

fn source_text(id: &str) -> String {
    source_memory(id)["evidence"]
        .as_str()
        .expect("source text")
        .to_owned()
}

fn pack_source_bodies(candidates: impl IntoIterator<Item = String>) -> (Vec<String>, usize) {
    let mut used = 0;
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for id in candidates {
        if !seen.insert(id.clone()) {
            continue;
        }
        let bytes = source_text(&id).len();
        if used + bytes <= SOURCE_BODY_BUDGET {
            used += bytes;
            selected.push(id);
        }
    }
    (selected, used)
}

fn source_id_for_evidence(text: &str) -> Option<String> {
    serde_json::from_str::<Value>(SOURCE).expect("source fixture")["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .find(|memory| memory["evidence"] == text)
        .and_then(|memory| memory["id"].as_str())
        .map(str::to_owned)
}

fn native_source_candidates(packet: &Value) -> Vec<String> {
    let mut seen = BTreeSet::new();
    packet["proof"]["evidence"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["text"].as_str())
        .filter_map(source_id_for_evidence)
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn fts5_source_candidates() -> (Vec<String>, u128, u128) {
    let preparation_started = Instant::now();
    let db = Connection::open_in_memory().expect("in-memory FTS5");
    db.execute_batch(
        "CREATE VIRTUAL TABLE sources USING fts5(id UNINDEXED, text, tokenize='unicode61 remove_diacritics 2');",
    )
    .expect("FTS5 available in the bundled SQLite control");
    for memory in serde_json::from_str::<Value>(SOURCE).expect("source fixture")["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .filter(|memory| {
            kmp_domain::compare_temporal_instants(
                memory["observed_at"].as_str().expect("observed time"),
                CUT,
            )
            .expect("valid source time")
                != Ordering::Greater
        })
    {
        db.execute(
            "INSERT INTO sources(id, text) VALUES (?1, ?2)",
            params![
                memory["id"].as_str().expect("id"),
                memory["evidence"].as_str().expect("source text")
            ],
        )
        .expect("source inserted");
    }
    let preparation = preparation_started.elapsed().as_micros();
    let query_started = Instant::now();
    let mut statement = db
        .prepare("SELECT id, text FROM sources WHERE sources MATCH ?1 ORDER BY bm25(sources), id")
        .expect("FTS5 query");
    let rows = statement
        .query_map([FTS5_PROBE], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("FTS5 rows");
    let mut candidates = Vec::new();
    for row in rows {
        let (id, _text) = row.expect("row");
        candidates.push(id);
    }
    (candidates, preparation, query_started.elapsed().as_micros())
}

#[tokio::test]
async fn local_fts5_control_compares_same_question_cut_and_source_body_budget() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let seed_started = Instant::now();
    seed(&server).await;
    let seed_micros = seed_started.elapsed().as_micros();
    let direct_started = Instant::now();
    let mut page = call(
        &server,
        "kmp_ask",
        json!({"about":ABOUT,"question":QUESTION,"as_of":{"time":CUT},"axis":"observed",
            "budget":{"max_bytes":NATIVE_RESPONSE_BUDGET,"detail":"full","max_entries":10}}),
    )
    .await;
    let mut native_candidates = Vec::new();
    let mut native_transport_bytes = 0_u64;
    let mut pages = 0_u32;
    loop {
        pages += 1;
        assert!(pages < 20, "native continuation terminates");
        native_transport_bytes += page["projection"]["budget"]["used_bytes"]
            .as_u64()
            .expect("native byte accounting");
        native_candidates.extend(native_source_candidates(&page));
        if page["projection"]["page"]["has_more"] != true {
            break;
        }
        let next = &page["projection"]["next_action"];
        page = call(
            &server,
            next["tool"].as_str().expect("continuation tool"),
            next["arguments"].clone(),
        )
        .await;
    }
    let direct_query_micros = direct_started.elapsed().as_micros();
    let (native, native_body_bytes) = pack_source_bodies(native_candidates);
    let (fts5_candidates, preparation_micros, query_micros) = fts5_source_candidates();
    let (fts5, fts5_body_bytes) = pack_source_bodies(fts5_candidates);

    assert!(native_body_bytes <= SOURCE_BODY_BUDGET);
    assert!(fts5_body_bytes <= SOURCE_BODY_BUDGET);
    assert!(!fts5.iter().any(|id| id == "future"));
    println!(
        "{}",
        json!({
            "control": "proof-support-fts5-lexical-v1",
            "question": QUESTION,
            "observed_cut": CUT,
            "source_body_budget_bytes": SOURCE_BODY_BUDGET,
            "native_response_budget_bytes": NATIVE_RESPONSE_BUDGET,
            "required_source_ids": REQUIRED,
            "native": {
                "selected_source_ids": native,
                "complete_support": REQUIRED.iter().all(|id| native.iter().any(|actual| actual == id)),
                "source_body_bytes": native_body_bytes,
                "pages": pages,
                "seed_micros": seed_micros,
                "structured_content_bytes": native_transport_bytes,
                "query_micros": direct_query_micros,
            },
            "fts5": {
                "query": FTS5_PROBE,
                "selected_source_ids": fts5,
                "complete_support": REQUIRED.iter().all(|id| fts5.iter().any(|actual| actual == id)),
                "source_body_bytes": fts5_body_bytes,
                "preparation_micros": preparation_micros,
                "query_micros": query_micros,
            },
        })
    );
}
