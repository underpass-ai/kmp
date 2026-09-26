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
use kmp_testkit::WriteReceipt;
use kmp_testkit::retrieval_scorecard::{
    AskVerdict, BaselineBound, BaselineRow, GuardedDecision, GuardedScorecard, RetrievalOutcome,
    RetrievalScorecard, baseline_failures, baseline_rows, render_baseline,
};
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
    /// Which abouts the question may reach, exactly as `kmp_ask` takes
    /// `dimensions`; absent, the question stays inside `about`.
    #[serde(default)]
    dimensions: Option<Value>,
    /// Memories written to other abouts before the question is asked, each
    /// through its own ingest, so a case can read across abouts the way a
    /// store holds them: never joined by a relation.
    #[serde(default)]
    memories: Vec<SeededMemory>,
    /// Writes made through `kmp_write_memory` after the ingests, exactly as
    /// the tool takes them: the only way a case declares the one relation
    /// that crosses an about.
    #[serde(default)]
    writes: Vec<Value>,
    /// The case type, for the cases added beyond the original 35. The
    /// original cases carry none, which is how their floors stay comparable.
    #[serde(default)]
    kind: Option<String>,
    /// Refs the answer must not cite: the excluded anchor of a negated
    /// question, or the neighbour an absent anchor would be confused with.
    #[serde(default)]
    forbidden: Vec<String>,
    /// For a negative case, the words a PARTIAL must name in `missing` to
    /// count as abstaining rather than as an answer.
    #[serde(default)]
    absent: Vec<String>,
}

impl JudgedCase {
    /// Whether a right outcome includes declining to answer or to cite.
    fn is_guarded(&self) -> bool {
        self.judged.is_empty() || !self.forbidden.is_empty()
    }
}

/// What one `kmp_ask` call produced, as the scorecard reads it.
struct Asked {
    outcome: RetrievalOutcome,
    to_judged: u64,
    verdict: AskVerdict,
    confidence: String,
}

#[derive(Debug, Deserialize)]
struct SeededMemory {
    about: String,
    memory: Value,
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
    // Evaluation arms, off by default: Ask re-ranking by TypeSafe Jev
    // (`narrow` or `wide`, answered from `KMP_TYPESAFE_CASSETTE`) and a
    // tighter entry cap. An arm reports and never gates the recorded floors.
    let arm = std::env::var("RETRIEVAL_RERANK").ok();
    let max_entries = std::env::var("RETRIEVAL_MAX_ENTRIES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10);
    // The anchored ask gate (`ask-gate.json` beside each case's store):
    // `anchored`, or `anchored-strict` without PARTIAL. An arm like the others.
    let ask_gate = std::env::var("RETRIEVAL_ASK_GATE").ok();
    let gated = arm.is_none() && ask_gate.is_none() && max_entries == 10;
    let mut bytes_to_judged = Vec::new();

    let collection: JudgedCollection = serde_json::from_str(&fs::read_to_string(&cases_path)?)?;
    // The original 35 are scored on their own, so their recorded floors stay
    // comparable; the rest adds positives and guarded cases around them.
    let mut original = Vec::new();
    let mut positives = Vec::new();
    let mut guarded = Vec::new();
    println!(
        "{:<24} {:>7} {:>7} {:>7} {:>6} {:>8} {:>7}  note",
        "case", "R@1", "R@5", "nDCG", "cite", "verdict", "conf"
    );
    for case in &collection.cases {
        // The cassette an arm replays holds the original cases' requests
        // only; a case beyond them would be a miss, not a measurement.
        if arm.is_some() && case.kind.is_some() {
            continue;
        }
        let asked = run_case(case, arm.as_deref(), ask_gate.as_deref(), max_entries).await?;
        let outcome = asked.outcome;
        let decision = case.is_guarded().then(|| GuardedDecision {
            kind: case.kind.clone().unwrap_or_else(|| "original".to_string()),
            negative: case.judged.is_empty(),
            verdict: asked.verdict,
            cited_forbidden: case
                .forbidden
                .iter()
                .any(|item| outcome.cited.contains(item)),
            high_confidence: asked.confidence == "high",
        });
        let false_answer = decision
            .as_ref()
            .is_some_and(GuardedDecision::is_false_answer);
        println!(
            "{:<24} {:>7.2} {:>7.2} {:>7.2} {:>6} {:>8} {:>7}  {}",
            case.id,
            outcome.recall_at(1),
            outcome.recall_at(5),
            outcome.ndcg_at(10),
            if outcome.answer_cites_judged() {
                "yes"
            } else {
                "no"
            },
            asked.verdict.label(),
            asked.confidence,
            if outcome.is_false_unknown() {
                "FALSE UNKNOWN"
            } else if false_answer {
                "FALSE ANSWER"
            } else {
                ""
            }
        );
        // A case that found nothing is worth its sentence: the collection
        // exists to say which behaviour broke, not only that a number moved.
        if (!case.judged.is_empty() && outcome.recall_at(10) < 1.0)
            || (case.kind.is_some() && outcome.is_false_unknown())
            || false_answer
        {
            println!("{:<24} {:>31}  {}", "", "", case.probes);
        }
        if case.kind.is_none() {
            bytes_to_judged.push(asked.to_judged as f64);
            original.push(outcome.clone());
        }
        if !case.judged.is_empty() {
            positives.push(outcome);
        }
        guarded.extend(decision);
    }

    let scorecard = RetrievalScorecard::score(&original);
    println!(
        "\n{} cases (the original judged collection)",
        scorecard.cases
    );
    for (name, value) in scorecard.quality_columns() {
        println!("  {name:<32} {value:.4}");
    }
    println!(
        "  {:<32} {:.4}",
        "false_unknown_rate", scorecard.false_unknown_rate
    );
    println!(
        "  {:<32} {:.0}",
        "mean_used_bytes", scorecard.mean_used_bytes
    );
    println!(
        "  {:<32} {:.0}",
        "mean_elapsed_millis", scorecard.mean_elapsed_millis
    );

    println!(
        "  {:<32} {:.0}",
        "mean_bytes_to_judged",
        bytes_to_judged.iter().sum::<f64>() / bytes_to_judged.len().max(1) as f64
    );
    let every = RetrievalScorecard::score(&positives);
    let decisions = GuardedScorecard::score(&guarded);
    let rows = baseline_rows(&scorecard, &every, &decisions);
    println!(
        "\nthe whole collection: {} with a judged answer, {} guarded (false_* rates are ceilings)",
        every.cases, decisions.cases
    );
    for row in rows.iter().filter(|row| row.extended) {
        let digits = if row.bound == BaselineBound::Exact {
            0
        } else {
            4
        };
        println!("  {:<32} {:.digits$}", row.name, row.value);
    }
    if !gated {
        println!(
            "\narm rerank={} ask_gate={} max_entries={max_entries}: reported, not gated",
            arm.as_deref().unwrap_or("off"),
            ask_gate.as_deref().unwrap_or("off")
        );
        return Ok(());
    }
    if record {
        fs::write(&baseline_path, render_baseline(&rows))?;
        println!("\nrecorded baseline at {}", baseline_path.display());
        return Ok(());
    }
    enforce_baseline(&baseline_path, &rows)
}

async fn run_case(
    case: &JudgedCase,
    arm: Option<&str>,
    ask_gate: Option<&str>,
    max_entries: u64,
) -> Result<Asked, Box<dyn Error>> {
    // A fresh store per case, so one case cannot weight another's terms: the
    // BM25 collection is whatever the store holds.
    let data_dir = std::env::temp_dir().join(format!("kmp-retrieval-{}", case.id));
    let _ = fs::remove_dir_all(&data_dir);
    fs::create_dir_all(&data_dir)?;
    if let Some(arm) = arm {
        fs::write(
            data_dir.join("typesafe.json"),
            r#"{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.13.0","timeout_ms":20000}"#,
        )?;
        fs::write(
            data_dir.join("rerank.json"),
            match arm {
                "wide" => r#"{"pool_size":400,"excerpt_chars":300}"#,
                _ => r#"{"pool_size":40}"#,
            },
        )?;
    }
    if let Some(gate) = ask_gate {
        fs::write(
            data_dir.join("ask-gate.json"),
            match gate {
                "anchored-strict" => r#"{"mode":"anchored","partial":false}"#,
                _ => r#"{"mode":"anchored","partial":true}"#,
            },
        )?;
    }
    // `RETRIEVAL_BOOKS=<dir>` keeps each case's verdict book between runs:
    // a second run over the same seeds answers every rerank from the book.
    let books = std::env::var("RETRIEVAL_BOOKS").ok().map(PathBuf::from);
    if let Some(books) = &books {
        for suffix in ["", "-wal"] {
            let kept = books.join(format!("{}.sqlite3{suffix}", case.id));
            if kept.is_file() {
                fs::copy(&kept, data_dir.join(format!("judgements.sqlite3{suffix}")))?;
            }
        }
    }
    let server = KernelMcpServer::embedded(&data_dir)?;

    let receipt = call(
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
    WriteReceipt::read("kmp_ingest", &receipt).require_accepted()?;
    for (index, seeded) in case.memories.iter().enumerate() {
        let receipt = call(
            &server,
            10 + index as u64,
            "kmp_ingest",
            json!({
                "about": seeded.about,
                "idempotency_key": format!("judged:{}:{}", case.id, seeded.about),
                "memory": seeded.memory
            }),
        )
        .await?;
        WriteReceipt::read("kmp_ingest", &receipt).require_accepted()?;
    }
    // A case is only scorable once every seeded write actually landed. A
    // proposal still waiting on a review has written nothing, and scoring it
    // would grade the store for memory it never held (#691).
    for (index, write) in case.writes.iter().enumerate() {
        commit_judged_write(&server, &case.id, 30 + index as u64, write).await?;
    }

    let mut arguments = json!({
        "about": case.about,
        "question": case.question,
        "answer_policy": case.answer_policy,
        "depth": 3,
        "budget": {"tokens": 2048, "detail": "balanced", "max_entries": max_entries}
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
    if let Some(dimensions) = &case.dimensions {
        arguments["dimensions"] = dimensions.clone();
    }
    let started = Instant::now();
    let answer = call(&server, 2, "kmp_ask", arguments).await?;
    let elapsed_millis = started.elapsed().as_millis() as u64;
    if let Some(books) = &books {
        fs::create_dir_all(books)?;
        for suffix in ["", "-wal"] {
            let book = data_dir.join(format!("judgements.sqlite3{suffix}"));
            if book.is_file() {
                fs::copy(&book, books.join(format!("{}.sqlite3{suffix}", case.id)))?;
            }
        }
    }
    if let Ok(path) = std::env::var("RETRIEVAL_ANSWERS") {
        use std::io::Write;
        let mut log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(log, "{} {}", case.id, answer)?;
    }
    let _ = fs::remove_dir_all(&data_dir);
    if answer["warnings"]
        .to_string()
        .contains("rerank unavailable")
        || answer["warnings"].to_string().contains("rerank disabled")
    {
        return Err(format!("case `{}`: {}", case.id, answer["warnings"]).into());
    }
    // What a reader must take in before the first judged memory: the proof
    // items up to and including it, or all of them when none is judged.
    let to_judged = {
        let items = answer["proof"]["evidence"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let stop = items
            .iter()
            .position(|item| memory_ref(item).is_some_and(|r| case.judged.contains(&r)))
            .map_or(items.len(), |index| index + 1);
        items[..stop]
            .iter()
            .map(|item| item.to_string().len() as u64)
            .sum::<u64>()
    };

    let verdict = AskVerdict::read(&answer, &case.absent);
    let confidence = answer["proof"]["confidence"]
        .as_str()
        .unwrap_or("none")
        .to_string();

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
        return Ok(Asked {
            outcome: RetrievalOutcome {
                judged: BTreeSet::from([expected.clone()]),
                retrieved: named.clone(),
                cited: named.into_iter().collect(),
                unknown: false,
                used_bytes: answer["projection"]["budget"]["used_bytes"]
                    .as_u64()
                    .unwrap_or_default(),
                elapsed_millis,
            },
            to_judged,
            verdict,
            confidence,
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

    Ok(Asked {
        outcome: RetrievalOutcome {
            judged: case.judged.iter().cloned().collect(),
            retrieved,
            cited,
            unknown: answer["answer"].as_str() == Some("UNKNOWN"),
            used_bytes: answer["projection"]["budget"]["used_bytes"]
                .as_u64()
                .unwrap_or_default(),
            elapsed_millis,
        },
        to_judged,
        verdict,
        confidence,
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

/// Commits a judged case's write, resolving a review the kernel asks for.
///
/// A judged case is a deterministic fixture and its expected citations include
/// what these writes record, so the case resolves its own review through the
/// continuation the write returned — deliberately, and saying so on stderr. A
/// generic helper accepting a review on an agent's behalf would be recording a
/// judgement nobody made; this is the fixture making its own (#691).
async fn commit_judged_write(
    server: &KernelMcpServer,
    case: &str,
    id: u64,
    write: &Value,
) -> Result<(), Box<dyn Error>> {
    let written = call(server, id, "kmp_write_memory", write.clone()).await?;
    let receipt = WriteReceipt::read("kmp_write_memory", &written);
    if receipt.is_accepted() {
        return Ok(());
    }
    if !receipt.needs_review() {
        return Err(format!(
            "case `{case}` write {id}: {}",
            receipt.require_accepted().expect_err("not accepted")
        )
        .into());
    }
    let action = &written["next_actions"][0];
    let tool = action["tool"]
        .as_str()
        .ok_or_else(|| format!("case `{case}` write {id}: the review returned no verb"))?;
    eprintln!("case `{case}`: resolving the review its write asked for through `{tool}`");
    let resolved = call(server, id + 1_000, tool, action["arguments"].clone()).await?;
    WriteReceipt::read(tool, &resolved)
        .require_accepted()
        .map_err(|error| format!("case `{case}` write {id}, after review: {error}"))?;
    Ok(())
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

fn enforce_baseline(path: &Path, rows: &[BaselineRow]) -> Result<(), Box<dyn Error>> {
    let failures = baseline_failures(&fs::read_to_string(path)?, rows)?;
    if failures.is_empty() {
        println!("\nretrieval baseline holds");
        return Ok(());
    }
    Err(format!("retrieval quality regressed:\n  {}", failures.join("\n  ")).into())
}
