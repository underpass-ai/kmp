//! Which declared relations a span read admits, and on what clock.
//!
//! A relation carries its own clocks, independently of its endpoints, and a
//! bounded Wake, Ask or Relate read leaves out a link that stands later than
//! the span on the selected clock, so a replacement that was not known yet
//! replaces nothing. A new declaration that names no observation is observed
//! at the instant it was ingested, which is what makes a link written today
//! invisible to a reading of last winter. Both halves are the contract
//! `plugins/kmp/guide/verbs/time.md` states, the second at lines 246-247 and
//! the first at 282-290.
//!
//! Every case writes the same two January decisions and the same `supersedes`
//! between them through the real embedded kernel, and differs only in the
//! clocks the relation carries. Each is read twice: a span that closes before
//! any declaration could have been made, and one that closes after. A clock
//! that bounds a link shows up as the difference between the two.

use kmp_mcp::KernelMcpServer;
use kmp_testkit::WriteReceipt;
use serde_json::{Value, json};

const ABOUT: &str = "service:clocked";
const OLD: &str = "service:clocked:decision:old";
const NEW: &str = "service:clocked:decision:new";
const SPAN_START: &str = "2026-01-01T00:00:00Z";
/// After both facts, before any declaration any case makes.
const EARLY_END: &str = "2026-02-01T00:00:00Z";
/// After a declaration made in March.
const LATE_END: &str = "2026-04-01T00:00:00Z";
/// When the replacing decision was taken, so the earliest honest moment the
/// replacement could have been declared.
const DECIDED: &str = "2026-01-10T10:00:00Z";
/// A declaration made well after both facts but inside `LATE_END`.
const MARCH: &str = "2026-03-20T10:00:00Z";
/// Later than every span these cases read.
const AFTER_SPAN: &str = "2026-05-01T00:00:00Z";

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
        "coordinates":[{"dimension":"release","scope_id":"release:winter",
            "occurred_at":occurred_at,"sequence":sequence}]})
}

fn memory(clocks: Option<Value>) -> Value {
    let mut relation = json!({"from":NEW,"to":OLD,"rel":"supersedes","class":"evidential",
        "why":"One percent was too small to see anything.",
        "evidence":"Canary dashboards, first week of January.","confidence":"high"});
    if let Some(clocks) = clocks {
        relation["clocks"] = clocks;
    }
    json!({
        "dimensions":[{"id":"release:winter","kind":"release"}],
        "entries":[
            entry(OLD, "2026-01-05T10:00:00Z", 1, "The rollout starts with a canary of one percent."),
            entry(NEW, DECIDED, 2, "The rollout starts with a canary of five percent."),
        ],
        "relations":[relation]
    })
}

/// What one span read returned: the declared edges as `from rel to`, the clocks
/// the kernel reports for each, and the lifecycle state of each fact.
struct Reading {
    declared: Vec<String>,
    clocks: Vec<Value>,
    states: Value,
}

impl Reading {
    fn replaced(&self) -> bool {
        self.states[OLD] == json!("superseded")
    }
}

/// Writes the memory now and reads one span of it back.
async fn read_span(clocks: Option<Value>, end: &str) -> Reading {
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
            "interval":{"start":SPAN_START,"end":end},
            "budget":{"depth":3},"page":{"entries":64}}),
    )
    .await;
    let relations = page["declared"].as_array().expect("declared");
    // Both facts fall inside every span these cases read, so an absent edge is
    // the clock rule and never a fact that was left out.
    let facts = page["facts"].as_array().expect("facts");
    assert_eq!(facts.len(), 2, "both facts must be inside the span: {page}");
    Reading {
        declared: relations
            .iter()
            .map(|relation| {
                format!(
                    "{} {} {}",
                    relation["from"].as_str().unwrap_or_default(),
                    relation["rel"].as_str().unwrap_or_default(),
                    relation["to"].as_str().unwrap_or_default()
                )
            })
            .collect(),
        clocks: relations
            .iter()
            .map(|relation| relation["clocks"].clone())
            .collect(),
        states: Value::Object(
            facts
                .iter()
                .map(|fact| {
                    (
                        fact["ref"].as_str().unwrap_or_default().to_string(),
                        fact["state"].clone(),
                    )
                })
                .collect(),
        ),
    }
}

fn replacement() -> String {
    format!("{NEW} supersedes {OLD}")
}

/// The contract: a declaration that names no observation is observed at the
/// instant it was ingested, which is now. It therefore belongs to no earlier
/// reading, and the replacement it carries has not happened in one.
#[tokio::test]
async fn a_declaration_with_no_clock_of_its_own_is_observed_at_ingestion_and_does_not_reach_back() {
    for end in [EARLY_END, LATE_END] {
        let reading = read_span(None, end).await;
        assert!(
            reading.declared.is_empty(),
            "a link first observed at ingestion cannot stand in a span ending {end}: {:?}",
            reading.declared
        );
        assert!(
            !reading.replaced(),
            "nor may it replace anything there: {}",
            reading.states
        );
    }
}

/// What the judged relate cases now say, and the smallest thing a writer
/// records to put a historical declaration in its own span: when it was
/// observed.
#[tokio::test]
async fn a_declaration_that_says_when_it_was_observed_stands_in_that_span() {
    for end in [EARLY_END, LATE_END] {
        let reading = read_span(Some(json!({"observed_at": DECIDED})), end).await;
        assert_eq!(reading.declared, [replacement()], "at {end}");
        assert!(reading.replaced(), "at {end}: {}", reading.states);
    }
}

/// The other escape, kept so a repair of the judged data never becomes the
/// only way a historical declaration can be expressed.
#[tokio::test]
async fn a_declaration_that_says_when_it_came_about_stands_in_that_span() {
    let reading = read_span(Some(json!({"occurred_at": DECIDED})), LATE_END).await;
    assert_eq!(reading.declared, [replacement()]);
    assert!(reading.replaced(), "{}", reading.states);
}

/// The negative the bounding rule exists for: a link observed after the span
/// is absent from it, and the replacement it declares has not happened yet.
#[tokio::test]
async fn a_declaration_observed_after_the_span_does_not_apply_inside_it() {
    let reading = read_span(Some(json!({"observed_at": AFTER_SPAN})), LATE_END).await;
    assert!(reading.declared.is_empty(), "{:?}", reading.declared);
    assert!(!reading.replaced(), "{}", reading.states);
}

/// An observation equal to its own ingestion is a real observation, not a
/// missing one. A canonical restoration states both explicitly, so the pair is
/// a given rather than a race against the kernel's own clock.
///
/// The two spans are what make this a test: were an equal pair read as no
/// clock at all, the link would stand in the early span too.
#[tokio::test]
async fn an_explicit_observation_equal_to_its_ingestion_still_bounds_the_link() {
    let clocks = json!({"observed_at": MARCH, "ingested_at": MARCH});
    let late = read_span(Some(clocks.clone()), LATE_END).await;
    assert_eq!(late.declared, [replacement()], "March is inside this span");
    assert_eq!(
        late.clocks,
        [json!({"observed_at": MARCH, "ingested_at": MARCH})],
        "the kernel must report back the pair that was declared"
    );
    assert!(late.replaced(), "{}", late.states);

    let early = read_span(Some(clocks), EARLY_END).await;
    assert!(
        early.declared.is_empty(),
        "an equal pair is still an observation, and March is after this span: {:?}",
        early.declared
    );
    assert!(!early.replaced(), "{}", early.states);
}

/// A restoration keeps its historical ingestion and leaves observation
/// unknown. That ingestion is the only clock the link has, and on the default
/// clock it is what bounds it — otherwise a restored declaration would reach
/// every span, including spans that closed before it was ever recorded.
#[tokio::test]
async fn a_restored_declaration_is_bounded_by_the_historical_ingestion_it_preserved() {
    let clocks = json!({"ingested_at": MARCH});
    let late = read_span(Some(clocks.clone()), LATE_END).await;
    assert_eq!(late.declared, [replacement()], "March is inside this span");
    assert_eq!(
        late.clocks,
        [json!({"ingested_at": MARCH})],
        "the restored ingestion stands and no observation was invented for it"
    );

    let early = read_span(Some(clocks), EARLY_END).await;
    assert!(
        early.declared.is_empty(),
        "a link recorded in March cannot stand in a span that closed in February: {:?}",
        early.declared
    );
    assert!(!early.replaced(), "{}", early.states);
}
