use std::io::{Read, Write};
use std::net::TcpListener;

use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "support/unbridged_server.rs"]
mod unbridged_server;

async fn call(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc":"2.0", "id":id, "method":"tools/call",
        "params":{"name":name, "arguments":arguments}})
    .to_string();
    let response = server.handle_json_line(&line).await.expect("MCP response");
    let value: Value = serde_json::from_str(&response).expect("JSON");
    assert!(value.get("error").is_none(), "{value}");
    assert_ne!(value["result"]["isError"], true, "{value}");
    value["result"]["structuredContent"].clone()
}

fn ingest(about: &str, count: usize) -> Value {
    let entries = (0..count).map(|n| json!({
        "id":format!("{about}:entry:{n}"), "kind":"observation",
        "text":format!("An automobile was repaired by a technician. {}", "Workshop records. ".repeat(50)),
        "coordinates":[{"dimension":"task", "scope_id":"test", "sequence":n+1,
            "occurred_at":if n+1==count { "2027-01-01T00:00:00Z" } else { "2026-01-01T00:00:00Z" }}]
    })).collect::<Vec<_>>();
    json!({"about":about, "idempotency_key":format!("seed:{about}"), "memory":{
        "dimensions":[{"id":"test", "kind":"task"}], "entries":entries,
        "relations":[], "evidence":[]}})
}

/// One HTTP request only. The listener then disappears: any model call on a
/// continuation would fail instead of silently generating another selection.
fn sidecar(separate_channels: bool) -> (String, std::thread::JoinHandle<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind");
    let endpoint = format!("http://{}/rank", listener.local_addr().expect("address"));
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("request");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(20)))
            .expect("timeout");
        let mut data = Vec::new();
        let header_end = loop {
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).expect("HTTP headers");
            data.push(byte[0]);
            if data.ends_with(b"\r\n\r\n") {
                break data.len();
            }
        };
        let headers = String::from_utf8_lossy(&data);
        let length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|n| n.trim().parse::<usize>().expect("length"))
            })
            .expect("content length");
        data.resize(header_end + length, 0);
        stream.read_exact(&mut data[header_end..]).expect("body");
        let request: Value = serde_json::from_slice(&data[header_end..]).expect("JSON request");
        let question = request["question"].as_str().expect("question");
        let candidates = request["sources"]
            .as_array()
            .expect("sources")
            .iter()
            .map(|s| json!([s["entry_ref"], s["text_sha256"]]))
            .collect::<Vec<_>>();
        let mut response = json!({"model_revision":"test@revision",
            "question_sha256":format!("{:x}", Sha256::digest(question.as_bytes())), "candidates":candidates});
        if separate_channels {
            response["lexical_candidates"] = response["candidates"].clone();
            response["candidates"] = json!([]);
        }
        let response = response.to_string();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).expect("response");
        request
    });
    (endpoint, handle)
}

#[tokio::test]
async fn real_mcp_semantics_admit_before_encoding_and_freeze_pagination() {
    verify_admission_and_pagination(false).await;
}

#[tokio::test]
async fn real_mcp_accepts_independent_lexical_proof_and_freezes_it_between_pages() {
    verify_admission_and_pagination(true).await;
}

async fn verify_admission_and_pagination(separate_channels: bool) {
    let data = tempfile::tempdir().expect("store");
    let (endpoint, sidecar) = sidecar(separate_channels);
    std::fs::write(
        data.path().join("semantic-retrieval.json"),
        json!({
        "endpoint":endpoint,"model_revision":"test@revision"})
        .to_string(),
    )
    .expect("configuration");
    let server = unbridged_server::open(data.path());
    call(&server, 1, "kmp_ingest", ingest("project:semantic", 15)).await;
    call(&server, 2, "kmp_ingest", ingest("project:other", 2)).await;
    let mut arguments = json!({"about":"project:semantic", "question":"mechanic fixed car",
        "as_of":{"time":"2026-06-01T00:00:00Z"}, "axis":"occurred",
        "budget":{"max_bytes":4000,"detail":"full"}});
    let mut found = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        let response = call(&server, 3 + pages, "kmp_ask", arguments.clone()).await;
        assert_eq!(response["answer"], "UNKNOWN");
        assert!(
            response["because"]
                .as_array()
                .expect("citations")
                .is_empty()
        );
        assert!(serde_json::to_vec(&response).expect("JSON").len() <= 4000);
        for item in response["proof"]["evidence"].as_array().expect("proof") {
            if item["metadata"]["reached_by"] == "semantic" {
                if separate_channels {
                    assert_eq!(item["metadata"]["retrieval_channel"], "bm25");
                }
                found.insert(item["supports"][0].as_str().expect("ref").to_string());
            }
        }
        pages += 1;
        assert!(pages <= 30, "cursor must make progress");
        if response["projection"]["page"]["has_more"] != true {
            break;
        }
        arguments["page"] = json!({"cursor":response["projection"]["page"]["next_cursor"]});
    }
    assert!(pages > 1);
    assert_eq!(found.len(), 14);
    let encoded = sidecar.join().expect("one request");
    assert_eq!(
        encoded["sources"].as_array().expect("source batch").len(),
        14
    );
    assert!(!encoded.to_string().contains("project:other"));
    assert!(!encoded.to_string().contains("project:semantic:entry:14"));
}

#[tokio::test]
async fn unavailable_encoder_preserves_ordinary_retrieval_with_a_warning() {
    let data = tempfile::tempdir().expect("store");
    let listener = TcpListener::bind("127.0.0.1:0").expect("unused port");
    let endpoint = format!("http://{}/rank", listener.local_addr().expect("port"));
    drop(listener);
    std::fs::write(
        data.path().join("semantic-retrieval.json"),
        json!({
        "endpoint":endpoint,"model_revision":"test@revision"})
        .to_string(),
    )
    .expect("configuration");
    let server = unbridged_server::open(data.path());
    call(&server, 1, "kmp_ingest", ingest("project:offline", 2)).await;
    let result = call(
        &server,
        2,
        "kmp_ask",
        json!({"about":"project:offline", "question":"automobile repaired technician",
        "budget":{"max_bytes":10000,"detail":"full"}}),
    )
    .await;
    assert_ne!(result["answer"], "UNKNOWN");
    assert!(
        result["warnings"]
            .to_string()
            .contains("semantic retrieval unavailable")
    );
}
