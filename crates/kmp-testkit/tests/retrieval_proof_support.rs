//! Multi-passage judgments supplement the historical Ask relevance baseline.
//! These are native regression controls, not independent agent evaluations.
use std::collections::BTreeSet;

use kmp_mcp::KernelMcpServer;
use kmp_testkit::retrieval_scorecard::RetrievalOutcome;
use serde_json::{Value, json};

const ABOUT: &str = "project:retrieval-support";
const SOURCE: &str = include_str!("fixtures/proof_support.json");

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
    assert_eq!(pending["accepted"], false, "{pending}");
    let next = &pending["next_actions"][0];
    assert_eq!(next["tool"], "kmp_write_memory", "{pending}");
    let committed = call(server, "kmp_write_memory", next["arguments"].clone()).await;
    assert_eq!(committed["status"], "committed", "{committed}");
    assert_eq!(committed["accepted"], true, "{committed}");
    committed["local_refs"].clone()
}

fn query(refs: &Value) -> Value {
    json!({"about":ABOUT,"at":{"time":"2026-09-10T10:00:00Z"},
        "axis":"observed","refs":[refs["role"]],"include":{"dependencies":true},
        "limit":{"entries":1},"budget":{"max_bytes":500000}})
}

fn bodies(packet: &Value) -> Vec<Value> {
    ["/entries", "/proof/entries"]
        .iter()
        .flat_map(|path| {
            packet
                .pointer(path)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .cloned()
        .collect()
}

/// A ref counts only when its stored body and exact source evidence arrived.
/// Group member refs and omission counters cannot stand in for a passage.
fn judge(packet: &Value, refs: &Value, required: &[&str]) -> RetrievalOutcome {
    let source: Value = serde_json::from_str(SOURCE).expect("source fixture");
    let entries = bodies(packet);
    let delivered = source["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .filter(|memory| {
            let reference = &refs[memory["id"].as_str().expect("local id")];
            entries
                .iter()
                .any(|entry| entry["ref"] == *reference && entry["text"] == memory["summary"])
                && packet["proof"]["evidence"]
                    .as_array()
                    .expect("evidence")
                    .iter()
                    .any(|item| item["text"] == memory["evidence"])
        })
        .map(|memory| {
            refs[memory["id"].as_str().expect("local id")]
                .as_str()
                .expect("ref")
                .to_string()
        })
        .collect();
    RetrievalOutcome {
        judged: required
            .iter()
            .map(|id| refs[*id].as_str().expect("judged ref").to_string())
            .collect(),
        retrieved: delivered,
        cited: BTreeSet::new(),
        unknown: false,
        used_bytes: serde_json::to_vec(packet).expect("packet").len() as u64,
        elapsed_millis: 0,
    }
}

#[tokio::test]
async fn focused_role_needs_both_older_identity_and_boundary_qualifier() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let refs = seed(&server).await;
    let args = query(&refs);
    let mut control = args.clone();
    control["include"] = json!({"evidence":true,"relations":true});
    let plain = call(&server, "kmp_goto", control).await;
    let expanded = call(&server, "kmp_goto", args).await;
    let required = ["role", "alias", "boundary"];
    let before = judge(&plain, &refs, &required);
    let after = judge(&expanded, &refs, &required);
    assert_eq!(before.recall_at(10), 1.0 / 3.0);
    assert!(!before.has_complete_support_at(10));
    assert!(after.has_complete_support_at(10));
    assert_eq!(plain["entries"], expanded["entries"]);
    assert!(
        !bodies(&expanded)
            .iter()
            .any(|entry| entry["ref"] == refs["future"])
    );
    println!(
        "identity + qualifier: required passage recall {:.3} -> {:.3}; complete false -> true",
        before.recall_at(10),
        after.recall_at(10)
    );
}

#[tokio::test]
async fn hard_labels_exclude_support_and_unfiltered_navigation_recovers_it() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let refs = seed(&server).await;
    let mut args = query(&refs);
    args["dimensions"] = json!({"selectors":[{"key":"source","op":"in","values":["S2"]}]});
    let filtered = call(&server, "kmp_goto", args).await;
    let required = ["role", "alias", "boundary"];
    assert!(!judge(&filtered, &refs, &required).has_complete_support_at(10));
    assert_eq!(bodies(&filtered).len(), 1);
    // The hard selector removes the relation before group traversal. A group
    // with no remaining structural omissions still lacks the judged support.
    assert_eq!(
        filtered["proof"]["groups"][0]["member_refs"],
        json!([refs["role"]])
    );
    assert_eq!(
        filtered["proof"]["groups"][0]["unavailable_in_selection"],
        0
    );
    let fallback = call(&server, "kmp_goto", query(&refs)).await;
    assert!(judge(&fallback, &refs, &required).has_complete_support_at(10));
}

#[tokio::test]
async fn chronological_read_retains_two_scoped_names_without_equating_them() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let refs = seed(&server).await;
    let packet = call(
        &server,
        "kmp_forward",
        json!({"about":ABOUT,"axis":"observed",
        "interval":{"start":"2026-09-01T00:00:00Z","end":"2026-09-02T00:00:00Z"},
        "include":{"dependencies":true},"budget":{"max_bytes":500000}}),
    )
    .await;
    assert!(judge(&packet, &refs, &["alias", "other_alias"]).has_complete_support_at(10));
    assert_eq!(bodies(&packet).len(), 2);
    // The native proof includes structural label membership. It must not
    // turn identical names in two workshops into an identity relation.
    assert!(
        packet["proof"]["path"]
            .as_array()
            .expect("relations")
            .iter()
            .all(|link| link["rel"] != "same_entity_as")
    );
}

#[tokio::test]
async fn bounded_pages_recover_the_same_required_passages_as_one_full_packet() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let refs = seed(&server).await;
    let mut args = query(&refs);
    let full = call(&server, "kmp_goto", args.clone()).await;
    args["budget"]["max_bytes"] = json!(5000);
    let mut page = call(&server, "kmp_goto", args).await;
    assert_eq!(page["page"]["has_more"], true, "exercise continuation");
    let mut joined = json!({"entries":[],"proof":{"entries":[],"evidence":[]}});
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls < 30, "finite continuation");
        assert_eq!(page["selection"], full["selection"]);
        for path in ["/entries", "/proof/entries", "/proof/evidence"] {
            joined
                .pointer_mut(path)
                .expect("section")
                .as_array_mut()
                .expect("array")
                .extend(
                    page.pointer(path)
                        .expect("section")
                        .as_array()
                        .expect("array")
                        .iter()
                        .cloned(),
                );
        }
        if page["page"]["has_more"] == false {
            break;
        }
        let next = &page["next_actions"][0];
        page = call(
            &server,
            next["tool"].as_str().expect("tool"),
            next["arguments"].clone(),
        )
        .await;
    }
    let required = ["role", "alias", "boundary"];
    assert!(judge(&joined, &refs, &required).has_complete_support_at(10));
    assert_eq!(
        judge(&joined, &refs, &required).retrieved,
        judge(&full, &refs, &required).retrieved
    );
    println!("complete support after {calls} native pages");
}
