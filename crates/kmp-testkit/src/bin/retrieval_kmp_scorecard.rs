//! Scores `kmp_ask` against a judged collection, offline and without a model.
//!
//! The task benchmarks this repository already carries — LongMemEval,
//! MemoryArena, MemoryAgentBench — measure whether an agent succeeded. They
//! need the whole loop and an LLM judge, and they cannot separate a bad
//! retrieval from a good retrieval the agent then reasoned about badly. This
//! measures the retrieval alone, by comparing what came back against what a
//! reader judged, which is arithmetic and therefore something CI can hold.
use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use kmp_mcp::KernelMcpServer;
use kmp_testkit::retrieval_scorecard::{RetrievalOutcome, RetrievalScorecard};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct JudgedCollection {
    cases: Vec<JudgedCase>,
}

#[derive(Debug, Deserialize)]
struct JudgedCase {
    id: String,
    probes: String,
    about: String,
    question: String,
    answer_policy: String,
    judged: Vec<String>,
    memory: Value,
    /// Where the question stands in time, exactly as `kmp_ask` takes it:
    /// an instant, a half-open span, and the clock they read.
    #[serde(default)]
    as_of: Option<Value>,
    #[serde(default)]
    interval: Option<Value>,
    #[serde(default)]
    axis: Option<String>,
    /// For a question whose right answer is UNKNOWN within its span: the ref
    /// the proof must name as the nearest match outside it. Such a case is
    /// scored as if that ref were the one citation, so the ordinary metrics
    /// carry it without a column of their own.
    #[serde(default)]
    nearest_outside: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let cases_path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "crates/kmp-testkit/judged/retrieval_cases.json".to_string()),
    );
    let baseline_path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "docs/development/retrieval-baseline.tsv".to_string()),
    );
    let record = std::env::var("RETRIEVAL_BASELINE").as_deref() == Ok("write");

    let collection: JudgedCollection = serde_json::from_str(&fs::read_to_string(&cases_path)?)?;
    let mut outcomes = Vec::new();
    println!(
        "{:<24} {:>7} {:>7} {:>7} {:>6}  note",
        "case", "R@1", "R@5", "nDCG", "cite"
    );
    for case in &collection.cases {
        let outcome = run_case(case).await?;
        println!(
            "{:<24} {:>7.2} {:>7.2} {:>7.2} {:>6}  {}",
            case.id,
            outcome.recall_at(1),
            outcome.recall_at(5),
            outcome.ndcg_at(10),
            if outcome.answer_cites_judged() {
                "yes"
            } else {
                "no"
            },
            if outcome.is_false_unknown() {
                "FALSE UNKNOWN"
            } else {
                ""
            }
        );
        // A case that found nothing is worth its sentence: the collection
        // exists to say which behaviour broke, not only that a number moved.
        if outcome.recall_at(10) < 1.0 {
            println!("{:<24} {:>31}  {}", "", "", case.probes);
        }
        outcomes.push(outcome);
    }

    let scorecard = RetrievalScorecard::score(&outcomes);
    println!("\n{} cases", scorecard.cases);
    for (name, value) in scorecard.quality_columns() {
        println!("  {name:<24} {value:.4}");
    }
    println!(
        "  {:<24} {:.4}",
        "false_unknown_rate", scorecard.false_unknown_rate
    );
    println!(
        "  {:<24} {:.0}",
        "mean_used_bytes", scorecard.mean_used_bytes
    );
    println!(
        "  {:<24} {:.0}",
        "mean_elapsed_millis", scorecard.mean_elapsed_millis
    );

    if record {
        write_baseline(&baseline_path, &scorecard)?;
        println!("\nrecorded baseline at {}", baseline_path.display());
        return Ok(());
    }
    enforce_baseline(&baseline_path, &scorecard)
}

async fn run_case(case: &JudgedCase) -> Result<RetrievalOutcome, Box<dyn Error>> {
    // A fresh store per case, so one case cannot weight another's terms: the
    // BM25 collection is whatever the store holds.
    let data_dir = std::env::temp_dir().join(format!("kmp-retrieval-{}", case.id));
    let _ = fs::remove_dir_all(&data_dir);
    fs::create_dir_all(&data_dir)?;
    let server = KernelMcpServer::embedded(&data_dir)?;

    call(
        &server,
        1,
        "kmp_ingest",
        json!({
            "about": case.about,
            "idempotency_key": format!("judged:{}", case.id),
            "memory": case.memory
        }),
    )
    .await?;

    let mut arguments = json!({
        "about": case.about,
        "question": case.question,
        "answer_policy": case.answer_policy,
        "depth": 3,
        "budget": {"tokens": 2048, "detail": "balanced", "max_entries": 10}
    });
    if let Some(as_of) = &case.as_of {
        arguments["as_of"] = as_of.clone();
    }
    if let Some(interval) = &case.interval {
        arguments["interval"] = interval.clone();
    }
    if let Some(axis) = &case.axis {
        arguments["axis"] = json!(axis);
    }
    let started = Instant::now();
    let answer = call(&server, 2, "kmp_ask", arguments).await?;
    let elapsed_millis = started.elapsed().as_millis() as u64;
    let _ = fs::remove_dir_all(&data_dir);

    if let Some(expected) = &case.nearest_outside {
        // The right answer is UNKNOWN, and the proof must say what lies
        // nearest outside the span: that ref is the one citation this case
        // scores, and only when the answer was honestly UNKNOWN.
        let unknown = answer["answer"].as_str() == Some("UNKNOWN");
        let named = answer["proof"]["nearest_outside"]["ref"]
            .as_str()
            .filter(|_| unknown)
            .map(str::to_string)
            .into_iter()
            .collect::<Vec<_>>();
        return Ok(RetrievalOutcome {
            judged: BTreeSet::from([expected.clone()]),
            retrieved: named.clone(),
            cited: named.into_iter().collect(),
            unknown: false,
            used_bytes: answer["projection"]["budget"]["used_bytes"]
                .as_u64()
                .unwrap_or_default(),
            elapsed_millis,
        });
    }

    let retrieved = answer["proof"]["evidence"]
        .as_array()
        .map(|items| items.iter().filter_map(memory_ref).collect::<Vec<_>>())
        .unwrap_or_default();
    let cited = answer["because"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["ref"].as_str().map(strip_prefix))
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    Ok(RetrievalOutcome {
        judged: case.judged.iter().cloned().collect(),
        retrieved,
        cited,
        unknown: answer["answer"].as_str() == Some("UNKNOWN"),
        used_bytes: answer["projection"]["budget"]["used_bytes"]
            .as_u64()
            .unwrap_or_default(),
        elapsed_millis,
    })
}

/// The memory a returned citation stands for.
///
/// A response addresses evidence as `entry:<ref>` or `detail:<ref>`; a reader
/// judges the memory, not the envelope it arrived in.
fn memory_ref(item: &Value) -> Option<String> {
    item["id"].as_str().map(strip_prefix)
}

fn strip_prefix(value: &str) -> String {
    value
        .strip_prefix("entry:")
        .or_else(|| value.strip_prefix("detail:"))
        .unwrap_or(value)
        .to_string()
}

async fn call(
    server: &KernelMcpServer,
    id: u64,
    name: &str,
    arguments: Value,
) -> Result<Value, Box<dyn Error>> {
    let request = json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
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

fn write_baseline(path: &Path, scorecard: &RetrievalScorecard) -> Result<(), Box<dyn Error>> {
    let mut out = String::from(
        "# Recorded retrieval quality. A number may rise freely; lowering one is a\n\
         # reviewed change that says why. Cost is reported by the scorecard and\n\
         # deliberately not recorded here — a floor that rose because responses grew\n\
         # would be a gate rewarding waste.\n\
         #\n\
         # Refresh deliberately, never to make a red build green:\n\
         #   RETRIEVAL_BASELINE=write cargo run -p kmp-testkit --bin retrieval_kmp_scorecard\n\
         metric\tfloor\n",
    );
    out.push_str(&format!("cases\t{}\n", scorecard.cases));
    for (name, value) in scorecard.quality_columns() {
        // Truncated, never rounded. A floor recorded above the number it was
        // taken from is not a floor, and would fail the build that wrote it.
        out.push_str(&format!(
            "{name}\t{:.4}\n",
            (value * 10_000.0).floor() / 10_000.0
        ));
    }
    fs::write(path, out)?;
    Ok(())
}

fn enforce_baseline(path: &Path, scorecard: &RetrievalScorecard) -> Result<(), Box<dyn Error>> {
    let recorded = fs::read_to_string(path)?;
    let mut failures = Vec::new();
    for line in recorded.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("metric\t") {
            continue;
        }
        let (metric, floor) = line
            .split_once('\t')
            .ok_or_else(|| format!("malformed baseline row: {line}"))?;
        if metric == "cases" {
            let expected: usize = floor.parse()?;
            if scorecard.cases != expected {
                failures.push(format!(
                    "the collection changed size: {expected} judged cases recorded, {} run",
                    scorecard.cases
                ));
            }
            continue;
        }
        let floor: f64 = floor.parse()?;
        let measured = scorecard
            .quality_columns()
            .into_iter()
            .find(|(name, _)| *name == metric)
            .map(|(_, value)| value)
            .ok_or_else(|| format!("baseline names an unknown metric: {metric}"))?;
        // A hair of tolerance, so a float that lands one ulp low does not fail
        // a build for a change that moved nothing.
        if measured + 1e-9 < floor {
            failures.push(format!("{metric} fell to {measured:.4}, below {floor:.4}"));
        }
    }
    if failures.is_empty() {
        println!("\nretrieval baseline holds");
        return Ok(());
    }
    Err(format!("retrieval quality regressed:\n  {}", failures.join("\n  ")).into())
}
