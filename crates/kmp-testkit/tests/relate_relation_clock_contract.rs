//! Which declared relations a span read is allowed to drop.
//!
//! `kmp_relate` bounds a relation by the relation's own clock, so a link
//! learned after the span end does not apply inside it. That rule needs a
//! relation with no clock of its own to survive, because not knowing when a
//! link was learned is not evidence that it was learned late. Since relations
//! gained their own clocks, every declaration written without one is stamped
//! with the ingestion instant, which makes it later than any span a reader can
//! ask about — so the rule now drops the whole declared graph of a historical
//! reading, and the lifecycle states that graph carries with it.
//!
//! Each case here writes the same two March decisions and the same
//! `supersedes` between them, and differs only in the clock the relation
//! carries. They are run against the real embedded kernel.

use kmp_mcp::KernelMcpServer;
use kmp_testkit::WriteReceipt;
use serde_json::{Value, json};

const ABOUT: &str = "service:clocked";
const OLD: &str = "service:clocked:decision:old";
const NEW: &str = "service:clocked:decision:new";
const SPAN_START: &str = "2026-03-01T00:00:00Z";
const SPAN_END: &str = "2026-04-01T00:00:00Z";

async fn call(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
        "params":{"name":name,"arguments":arguments}})
    .to_string();
    let response = server.handle_json_line(&request).await.expect("response");
    let value: Value = serde_json::from_str(&response).expect("JSON");
    assert_ne!(
        value["result"]["isError"].as_bool(),
        Some(true),
        "{}",
        value["result"]
    );
    value["result"]["structuredContent"].clone()
}

fn entry(id: &str, occurred_at: &str, sequence: u32, text: &str) -> Value {
    json!({"id":id,"kind":"decision","text":text,
        "coordinates":[{"dimension":"release","scope_id":"release:spring",
            "occurred_at":occurred_at,"sequence":sequence}]})
}

/// The whole memory, with the one relation carrying whatever clocks the case
/// is about. `None` is the shape a writer produces when it declares a link and
/// says nothing about when the link itself came about.
fn memory(clocks: Option<Value>) -> Value {
    let mut relation = json!({"from":NEW,"to":OLD,"rel":"supersedes","class":"evidential",
        "why":"One percent was too small to see anything.",
        "evidence":"Canary dashboards, first week of March.","confidence":"high"});
    if let Some(clocks) = clocks {
        relation["clocks"] = clocks;
    }
    json!({
        "dimensions":[{"id":"release:spring","kind":"release"}],
        "entries":[
            entry(OLD, "2026-03-01T10:00:00Z", 1, "The rollout starts with a canary of one percent."),
            entry(NEW, "2026-03-10T10:00:00Z", 2, "The rollout starts with a canary of five percent."),
        ],
        "relations":[relation]
    })
}

/// Writes the memory today and reads March back: the declared edges as
/// `from rel to`, and the lifecycle state each fact carries.
async fn declared_in_march(clocks: Option<Value>) -> (Vec<String>, Value) {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let receipt = call(
        &server,
        1,
        "kmp_ingest",
        json!({"about":ABOUT,"idempotency_key":"relation-clock-contract",
            "memory":memory(clocks)}),
    )
    .await;
    WriteReceipt::read("kmp_ingest", &receipt)
        .require_accepted()
        .expect("the seed write must land before anything is read");

    let page = call(
        &server,
        2,
        "kmp_relate",
        json!({"about":ABOUT,"dimensions":{"scope":"current_about"},
            "interval":{"start":SPAN_START,"end":SPAN_END},
            "budget":{"depth":3},"page":{"entries":64}}),
    )
    .await;
    let declared = page["declared"]
        .as_array()
        .expect("declared")
        .iter()
        .map(|relation| {
            format!(
                "{} {} {}",
                relation["from"].as_str().unwrap_or_default(),
                relation["rel"].as_str().unwrap_or_default(),
                relation["to"].as_str().unwrap_or_default()
            )
        })
        .collect();
    let states = page["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .map(|fact| {
            (
                fact["ref"].as_str().unwrap_or_default().to_string(),
                fact["state"].clone(),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    (declared, Value::Object(states))
}

/// The regression: a relation the writer declared without a clock of its own
/// belongs to the span its facts fall in, and carries the replacement with it.
#[tokio::test]
async fn a_declaration_with_no_clock_of_its_own_stands_inside_the_span_its_facts_fall_in() {
    let (declared, states) = declared_in_march(None).await;
    assert_eq!(
        declared,
        [format!("{NEW} supersedes {OLD}")],
        "a link written without its own clock was dropped from the span its \
         own facts fall in; states were {states}"
    );
    assert_eq!(states[OLD], json!("superseded"), "{states}");
    assert_eq!(states[NEW], json!("current"), "{states}");
}

/// The escape a writer has today, and must keep after the regression is
/// fixed: saying when the link itself came about.
#[tokio::test]
async fn a_declaration_that_says_when_it_came_about_stands_in_that_span() {
    let (declared, states) =
        declared_in_march(Some(json!({"occurred_at":"2026-03-10T10:00:00Z"}))).await;
    assert_eq!(declared, [format!("{NEW} supersedes {OLD}")], "{states}");
    assert_eq!(states[OLD], json!("superseded"), "{states}");
}

/// The negative the bounding rule exists for, which no fix may give up: a
/// link observed after the span end did not apply inside it, so it is absent
/// and the replacement it declares has not happened yet.
#[tokio::test]
async fn a_declaration_observed_after_the_span_does_not_apply_inside_it() {
    let (declared, states) =
        declared_in_march(Some(json!({"observed_at":"2026-05-01T00:00:00Z"}))).await;
    assert!(declared.is_empty(), "{declared:?} / {states}");
    assert_eq!(states[OLD], json!("current"), "{states}");
}
