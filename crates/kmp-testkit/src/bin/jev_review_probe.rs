//! The review without `focus` of each judged case, alone: what
//! `jev_kmp_scorecard` asks first, with the same seeding and arguments, and
//! nothing after it. It prints one JSON line per case with every `missing`
//! item of every page, the suspects, the warnings, Jev's usage and the wall
//! milliseconds of the review, for a scorer that reads the corpus's gold.
//!
//! Made to compare review settings such as `KMP_EVAL_PARTNER_FACTS`
//! (`docs/development/jev-evaluation.md`) without recording every other arm
//! of the scorecard. Jev answers as in the scorecard: set
//! `KMP_TYPESAFE_CASSETTE` (replay by default). Each case's store lives in
//! `$JEV_PROBE_ROOT/<case id>` (a temporary directory by default); a verdict
//! book already there is kept and everything else is seeded afresh, so a
//! run can answer from the book a previous run left.
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use kmp_mcp::KernelMcpServer;
use kmp_testkit::WriteReceipt;
use serde::Deserialize;
use serde_json::{Value, json};

const TYPESAFE: &str = r#"{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.13.0","timeout_ms":20000}"#;
const BOOK_FILES: [&str; 2] = ["judgements.sqlite3", "judgements.sqlite3-wal"];

#[derive(Debug, Deserialize)]
struct Collection {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    about: String,
    abouts: Vec<String>,
    memories: Vec<Seeded>,
}

#[derive(Debug, Deserialize)]
struct Seeded {
    about: String,
    memory: Value,
}

/// What one review page adds to the probe's line: its missing items and
/// suspects, its warnings, and the continuation to the next page.
fn read_page(page: &Value, line: &mut Value) -> Option<Value> {
    for (key, field) in [("missing", "missing"), ("suspect", "suspect")] {
        if let (Some(items), Some(into)) = (page[field].as_array(), line[key].as_array_mut()) {
            into.extend(items.iter().cloned());
        }
    }
    if let (Some(warnings), Some(into)) =
        (page["warnings"].as_array(), line["warnings"].as_array_mut())
    {
        into.extend(warnings.iter().cloned());
    }
    page["next_actions"][0]["arguments"]
        .as_object()
        .cloned()
        .map(Value::Object)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cases = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "crates/kmp-testkit/judged/jev_cases.json".to_string());
    if std::env::var("KMP_TYPESAFE_CASSETTE").is_err() {
        return Err("set KMP_TYPESAFE_CASSETTE: Jev answers from a cassette or records one".into());
    }
    let root = std::env::var("JEV_PROBE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("kmp-jev-review-probe"));
    let collection: Collection = serde_json::from_str(&fs::read_to_string(&cases)?)?;
    for case in &collection.cases {
        let server = seeded(case, &root.join(&case.id)).await?;
        let mut line = json!({
            "case": case.id,
            "partner_facts": std::env::var("KMP_EVAL_PARTNER_FACTS").ok(),
            "missing": [], "suspect": [], "warnings": [],
        });
        let started = Instant::now();
        let mut page = call(
            &server,
            json!({"mode": "review", "about": case.about,
                   "dimensions": {"scope": "abouts", "abouts": case.abouts},
                   "max_pairs": 40, "page": {"entries": 20}}),
        )
        .await?;
        line["review_ms"] = json!(started.elapsed().as_millis() as u64);
        line["jev"] = page["jev"].clone();
        while let Some(next) = read_page(&page, &mut line) {
            page = call(&server, next).await?;
        }
        println!("{line}");
    }
    Ok(())
}

async fn seeded(
    case: &Case,
    data_dir: &std::path::Path,
) -> Result<KernelMcpServer, Box<dyn Error>> {
    let kept = BOOK_FILES
        .iter()
        .filter_map(|name| Some((*name, fs::read(data_dir.join(name)).ok()?)))
        .collect::<Vec<_>>();
    let _ = fs::remove_dir_all(data_dir);
    fs::create_dir_all(data_dir)?;
    for (name, bytes) in kept {
        fs::write(data_dir.join(name), bytes)?;
    }
    fs::write(data_dir.join("typesafe.json"), TYPESAFE)?;
    let server = KernelMcpServer::embedded(data_dir)?;
    for seeded in &case.memories {
        let receipt = tool(
            &server,
            "kmp_ingest",
            json!({"about": seeded.about,
                   "idempotency_key": format!("judged:{}:{}", case.id, seeded.about),
                   "memory": seeded.memory}),
        )
        .await?;
        WriteReceipt::read("kmp_ingest", &receipt).require_accepted()?;
    }
    Ok(server)
}

async fn call(server: &KernelMcpServer, arguments: Value) -> Result<Value, Box<dyn Error>> {
    tool(server, "kmp_curate", arguments).await
}

async fn tool(
    server: &KernelMcpServer,
    name: &str,
    arguments: Value,
) -> Result<Value, Box<dyn Error>> {
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    })
    .to_string();
    let response = server
        .handle_json_line(&request)
        .await
        .ok_or_else(|| format!("tool `{name}` produced no response"))?;
    let value: Value = serde_json::from_str(&response)?;
    if value["result"]["isError"].as_bool() == Some(true) {
        return Err(format!("tool `{name}` failed: {}", value["result"]).into());
    }
    Ok(value["result"]["structuredContent"].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_adds_its_items_and_names_the_next() {
        let mut line = json!({"missing": [], "suspect": [], "warnings": []});
        let first = json!({
            "missing": [{"item_id": "m1"}], "suspect": [{"item_id": "s1"}],
            "warnings": ["w"],
            "next_actions": [{"arguments": {"review_token": "t", "page": {"cursor": "20"}}}]
        });
        let next = read_page(&first, &mut line).expect("a continuation");
        assert_eq!(next["page"]["cursor"], "20");
        let last = json!({"missing": [{"item_id": "m2"}]});
        assert_eq!(read_page(&last, &mut line), None);
        assert_eq!(line["missing"].as_array().map(Vec::len), Some(2));
        assert_eq!(line["suspect"].as_array().map(Vec::len), Some(1));
        assert_eq!(line["warnings"], json!(["w"]));
    }
}
