//! A single-destination trace is a bounded bidirectional search (DESIGN L7):
//! a near destination returns the path the unbounded walk found, and one
//! beyond the work limits is partial, never absent.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:bounded-trace";
const AT: &str = "2026-09-01T10:00:00Z";

async fn call(server: &KernelMcpServer, tool: &str, args: Value) -> Value {
    let wire = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":args}})
            .to_string(),
        )
        .await
        .expect("answered");
    let reply: Value = serde_json::from_str(&wire).expect("json");
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    reply["result"]["structuredContent"].clone()
}

fn entry(i: usize) -> String {
    format!("{ABOUT}:entry:observation:f-{i:04}")
}

/// Observation i builds on observation i - 1: a chain of `n` entries.
fn chain(n: usize) -> Value {
    let entries = (0..n)
        .map(|i| {
            json!({"id": entry(i), "kind": "observation", "text": format!("Observation {i}."),
            "coordinates": [{"dimension": "task", "scope_id": "proof", "observed_at": AT}]})
        })
        .collect::<Vec<_>>();
    let relations = (1..n)
        .map(|i| {
            json!({"from": entry(i), "to": entry(i - 1), "rel": "uses_background",
            "class": "evidential", "confidence": "high",
            "why": format!("Observation {i} builds on observation {}.", i - 1),
            "evidence": format!("Fixture link {i}->{}.", i - 1)})
        })
        .collect::<Vec<_>>();
    json!({"about": ABOUT, "idempotency_key": "chain",
        "memory": {"dimensions": [{"id": "proof", "kind": "task"}],
            "entries": entries, "relations": relations}})
}

#[tokio::test]
async fn a_single_destination_trace_is_bounded_and_partial_beyond_its_limits() {
    let dir = tempfile::tempdir().expect("dir");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    call(&server, "kmp_ingest", chain(600)).await;

    let near = call(
        &server,
        "kmp_trace",
        json!({"about": ABOUT, "from": entry(599), "to": entry(579), "page": {"entries": 50}}),
    )
    .await;
    assert_eq!(near["trace"].as_array().map(Vec::len), Some(20), "{near}");
    assert_eq!(near["trace"][0]["from"], entry(599));
    assert_eq!(near["trace"][19]["to"], entry(579));
    assert!(
        near.get("search").is_none(),
        "a found path keeps its shape: {near}"
    );

    let far = call(
        &server,
        "kmp_trace",
        json!({"about": ABOUT, "from": entry(599), "to": entry(0)}),
    )
    .await;
    assert_eq!(far["trace"], json!([]), "{far}");
    let search = &far["search"];
    assert_eq!(search["direction"], "bidirectional", "{far}");
    assert_eq!(search["stop_reason"], "depth_budget", "{far}");
    assert_eq!(search["unreached_targets"], json!([entry(0)]));
    assert_eq!(search["from"], entry(599));
    let warnings = far["warnings"].to_string();
    assert!(warnings.contains("not proof"), "{warnings}");
    let widen = &search["widen"];
    assert_eq!(widen["tool"], "kmp_trace");
    assert_eq!(widen["arguments"]["to"], json!(entry(0)));
    assert_eq!(widen["arguments"]["search"]["direction"], "bidirectional");
    assert_eq!(widen["arguments"]["search"]["max_depth"], 1024);

    let widened = call(&server, "kmp_trace", widen["arguments"].clone()).await;
    assert!(
        widened.get("search").is_none(),
        "the largest allowance reaches the far end as a path: {widened}"
    );
    assert_eq!(widened["page"]["total"], 599, "{widened}");
    assert_eq!(widened["trace"][0]["from"], entry(599));

    // search.widen follows every stored relation, structural links
    // included, like the trace it widens: from the about anchor through its
    // structural link, where a bounded target search cannot even start.
    let widened_from_about = call(
        &server,
        "kmp_trace",
        json!({"about": ABOUT, "from": ABOUT, "to": entry(300),
            "search": {"direction": "bidirectional", "max_nodes": 4096}}),
    )
    .await;
    let plain_from_about = call(
        &server,
        "kmp_trace",
        json!({"about": ABOUT, "from": ABOUT, "to": entry(300)}),
    )
    .await;
    assert_eq!(widened_from_about, plain_from_about);
    assert!(
        !plain_from_about["trace"]
            .as_array()
            .expect("trace")
            .is_empty(),
        "{plain_from_about}"
    );

    let reverse = call(
        &server,
        "kmp_trace",
        json!({"about": ABOUT, "from": entry(0), "to": entry(5)}),
    )
    .await;
    assert_eq!(reverse["trace"], json!([]));
    assert!(
        reverse.get("search").is_none(),
        "an exhausted search keeps the old answer: {reverse}"
    );
}
