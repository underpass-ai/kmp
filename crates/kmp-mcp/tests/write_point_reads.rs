//! A write reads its about point by point (DESIGN L6, write in O(delta)):
//! the dimensions and labels it holds, whether the refs a relation or an
//! evidence names stand in it, and the sequence frontier of each coordinate
//! it writes. Each must answer exactly as the about's neighbourhood did.

use kmp_domain::{MemoryAboutIndexReader, MemoryWriteFactsRequest};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:point-writes";

async fn ingest(server: &KernelMcpServer, key: &str, memory: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":"kmp_ingest","arguments":{"about":ABOUT,"idempotency_key":key,"memory":memory}}});
    let wire = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    serde_json::from_str::<Value>(&wire).expect("JSON")["result"].clone()
}

fn entry(id: &str, scope: &str, dimension: &str, sequence: Option<u32>) -> Value {
    let mut coordinate = json!({"dimension":dimension,"scope_id":scope,
        "occurred_at":"2026-09-01T10:00:00Z","valid_from":"2026-09-01T10:00:00Z"});
    if let Some(sequence) = sequence {
        coordinate["sequence"] = json!(sequence);
    }
    json!({"id":format!("{ABOUT}:{id}"),"kind":"observation",
        "text":format!("Observation {id} about the reserve valve."),"coordinates":[coordinate]})
}

fn ok(result: &Value) {
    assert_ne!(result["isError"], true, "{result}");
}

fn refused(result: &Value, why: &str) {
    assert_eq!(result["isError"], true, "{result}");
    assert!(result.to_string().contains(why), "{result}");
}

async fn sequences(dir: &std::path::Path, scope: &str) -> Vec<(String, u32)> {
    let kernel = kmp_embedded::EmbeddedKernel::open(dir).expect("reader");
    let scope = scope.to_string();
    let edges = kernel
        .store()
        .read_points(move |reads| reads.outgoing(&scope, Some("contains_entry")))
        .await
        .expect("edges");
    edges
        .into_iter()
        .map(|edge| {
            (
                edge.target_node_id
                    .rsplit(':')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                edge.explanation.sequence().expect("sequence"),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_write_reads_its_about_point_by_point_and_answers_as_the_neighbourhood_did() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let dimensions =
        json!([{"id":"work:main","kind":"work"},{"id":"component:valve","kind":"component"}]);
    ok(&ingest(
        &server,
        "w1",
        json!({"dimensions":dimensions,
        "entries":[entry("e1","work:main","work",Some(3)), entry("e2","work:main","work",None),
                   entry("e3","component:valve","component",None)],
        "evidence":[{"id":format!("evidence:{ABOUT}:ev1"),"supports":[format!("{ABOUT}:e1")],
                     "text":"Operator log 14 records the frozen reserve valve."}],
        "relations":[]}),
    )
    .await);

    // The store answers the catalogue: both dimensions, their kinds.
    let kernel = kmp_embedded::EmbeddedKernel::open(dir.path()).expect("reader");
    let facts = kernel
        .store()
        .memory_write_facts(&MemoryWriteFactsRequest::new(ABOUT))
        .await
        .expect("facts")
        .expect("answered");
    assert!(facts.exists);
    let kinds = facts.dimensions.values().cloned().collect::<Vec<_>>();
    assert_eq!(kinds.len(), 2, "{facts:?}");
    assert!(
        kinds.contains(&Some("work".to_string())) && kinds.contains(&Some("component".to_string()))
    );
    drop(kernel);

    // Frontiers continue from what the store holds; named refs that stand in
    // the about are accepted, and a relation may start at the anchor.
    ok(&ingest(&server, "w2", json!({"dimensions":[],
        "entries":[entry("e4","work:main","work",None), entry("e5","component:valve","component",Some(10))],
        "evidence":[{"id":format!("evidence:{ABOUT}:ev2"),"supports":[format!("{ABOUT}:e2")],
                     "text":"Ticket M-201 closes the replacement."}],
        "relations":[{"from":format!("{ABOUT}:e4"),"to":format!("{ABOUT}:e1"),"rel":"motivates",
                      "class":"motivational","why":"the frozen valve motivated the check",
                      "evidence":"Operator log 14","confidence":"high"},
                     {"from":ABOUT,"to":format!("{ABOUT}:e3"),"rel":"relates_to","class":"structural"}]})).await);
    ok(&ingest(
        &server,
        "w3",
        json!({"dimensions":[],
        "entries":[entry("e6","component:valve","component",None)],"relations":[]}),
    )
    .await);

    // Refs that stand nowhere in the about are refused, as before.
    refused(&ingest(&server, "w4", json!({"dimensions":[],
        "entries":[entry("e7","work:main","work",None)],
        "relations":[{"from":format!("{ABOUT}:e7"),"to":format!("{ABOUT}:missing"),"rel":"motivates",
                      "class":"motivational","why":"w","evidence":"e","confidence":"high"}]})).await, "unknown refs");
    refused(
        &ingest(
            &server,
            "w5",
            json!({"dimensions":[],
        "entries":[entry("e8","work:main","work",None)],
        "evidence":[{"id":format!("evidence:{ABOUT}:ev3"),"supports":[format!("{ABOUT}:nowhere")],
                     "text":"Nothing supports this."}],"relations":[]}),
        )
        .await,
        "supports unknown ref",
    );

    let work = sequences(
        dir.path(),
        &facts
            .dimensions
            .iter()
            .find(|(_, k)| k.as_deref() == Some("work"))
            .expect("work")
            .0
            .clone(),
    )
    .await;
    let component = sequences(
        dir.path(),
        &facts
            .dimensions
            .iter()
            .find(|(_, k)| k.as_deref() == Some("component"))
            .expect("component")
            .0
            .clone(),
    )
    .await;
    assert_eq!(
        work,
        vec![("e1".into(), 3), ("e2".into(), 4), ("e4".into(), 5)]
    );
    assert_eq!(
        component,
        vec![("e3".into(), 1), ("e5".into(), 10), ("e6".into(), 11)]
    );

    // A label resembling one the about holds is still named.
    let warned = ingest(
        &server,
        "w6",
        json!({"dimensions":[{"id":"Work-Main","kind":"work"}],
        "entries":[entry("e9","Work-Main","work",None)],"relations":[]}),
    )
    .await;
    ok(&warned);
    assert!(warned.to_string().contains("resembl"), "{warned}");
}

/// Two writers take turns on one about; each keeps the frontiers its own
/// writes left, and neither may reuse a sequence the other wrote.
#[tokio::test]
async fn writers_taking_turns_never_reuse_a_sequence() {
    let dir = tempfile::tempdir().expect("store");
    let first = KernelMcpServer::embedded(dir.path()).expect("first writer");
    let second = KernelMcpServer::embedded(dir.path()).expect("second writer");
    let dimensions = json!([{"id":"work:main","kind":"work"}]);
    let mut expected = Vec::new();
    for turn in 0..8u32 {
        let server = if turn % 3 == 2 { &second } else { &first };
        let id = format!("t{turn}");
        ok(&ingest(
            server,
            &format!("turn-{turn}"),
            json!({"dimensions":dimensions,
            "entries":[entry(&id,"work:main","work",None)],"relations":[]}),
        )
        .await);
        expected.push((id, turn + 1));
    }
    let kernel = kmp_embedded::EmbeddedKernel::open(dir.path()).expect("reader");
    let facts = kernel
        .store()
        .memory_write_facts(&MemoryWriteFactsRequest::new(ABOUT))
        .await
        .expect("facts")
        .expect("answered");
    let scope = facts.dimensions.keys().next().expect("work").clone();
    drop(kernel);
    let mut held = sequences(dir.path(), &scope).await;
    held.sort_by_key(|(_, sequence)| *sequence);
    assert_eq!(held, expected);
}
