//! Scores TypeSafe Jev working with KMP against a judged corpus: what
//! `kmp_curate` finds and doubts, what its pre-write check holds back, and
//! where `kmp_ask` puts the answer with and without re-ranking.
//!
//! Jev is a remote model, so the score is only repeatable against recorded
//! answers. `scripts/ci/jev-baseline.sh` points the embedded server at the
//! cassette beside the corpus (`KMP_TYPESAFE_CASSETTE`, replay by default):
//! no key, no network, and a request that was never recorded fails the run
//! instead of being guessed. Re-recording with a real key
//! (`KMP_TYPESAFE_CASSETTE_MODE=record`) is how a new model or prompt is
//! measured.
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use kmp_mcp::KernelMcpServer;
use kmp_testkit::WriteReceipt;
use serde::Deserialize;
use serde_json::{Value, json};

const ACTOR: &str = "jev-scorecard";
const TYPESAFE: &str = r#"{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.13.0","timeout_ms":20000}"#;
/// The narrow pool: the lexical ranker's forty best, read whole.
const RERANK_NARROW: &str = r#"{"pool_size":40}"#;
/// The wide pool: up to four hundred admitted passages, 300-character
/// excerpts each.
const RERANK_WIDE: &str = r#"{"pool_size":400,"excerpt_chars":300}"#;
const ASK_TOP: usize = 5;

#[derive(Debug, Deserialize)]
struct JudgedCollection {
    cases: Vec<JudgedCase>,
}

#[derive(Debug, Deserialize)]
struct JudgedCase {
    id: String,
    probes: String,
    about: String,
    abouts: Vec<String>,
    memories: Vec<SeededMemory>,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
struct SeededMemory {
    about: String,
    memory: Value,
}

#[derive(Debug, Deserialize)]
struct Expected {
    missing: Vec<MissingPair>,
    distractors: Vec<Distractor>,
    declared_good: Vec<[String; 3]>,
    declared_bad: Vec<[String; 3]>,
    precheck: Vec<Precheck>,
    ask: Vec<AskCase>,
}

#[derive(Debug, Deserialize)]
struct MissingPair {
    pair: [String; 2],
    types: Vec<String>,
    kind: String,
}

#[derive(Debug, Deserialize)]
struct Distractor {
    pair: [String; 2],
}

#[derive(Debug, Deserialize)]
struct Precheck {
    pair: [String; 2],
    from: String,
    rel: String,
    why: String,
    evidence: String,
    doubt: bool,
}

#[derive(Debug, Deserialize)]
struct AskCase {
    question: String,
    gold: Vec<String>,
    shape: String,
}

/// One missing item as a review returned it.
#[derive(Debug, Clone)]
struct Found {
    item_id: String,
    from: String,
    to: String,
    suggested: Option<String>,
    proposed_by: String,
}

#[derive(Debug, Default)]
struct Tally {
    hits: f64,
    total: f64,
}

impl Tally {
    fn add(&mut self, hit: bool) {
        self.total += 1.0;
        if hit {
            self.hits += 1.0;
        }
    }

    fn rate(&self) -> f64 {
        if self.total == 0.0 {
            1.0
        } else {
            self.hits / self.total
        }
    }
}

#[derive(Debug, Default)]
struct Scores {
    missing_found: Tally,
    missing_typed: Tally,
    distractors_rejected: Tally,
    suspect_recall: Tally,
    suspect_precision: Tally,
    suspect_recall_no_direction: Tally,
    suspect_precision_no_direction: Tally,
    missing_found_by_jev: Tally,
    good_kept: Tally,
    precheck_available: Tally,
    precheck_right: Tally,
    precheck_right_no_direction: Tally,
    ask_mrr_plain: Vec<f64>,
    ask_mrr_rerank: Vec<f64>,
    ask_mrr_wide: Vec<f64>,
    ask_top_plain: Tally,
    ask_top_rerank: Tally,
    ask_top_wide: Tally,
    ask_ms: [u128; 3],
    jev_requests: u64,
    jev_input_tokens: u64,
    /// Bytes an agent reads to curate by hand: every page of kmp_relate over
    /// the same selection, facts and proposals included.
    relate_bytes: u64,
    /// Bytes it reads with kmp_curate: every page of the review.
    review_bytes: u64,
    /// The same review on a store without Jev: kernel pairs, untyped.
    review_bytes_without_jev: u64,
    missing_found_without_jev: Tally,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let cases_path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "crates/kmp-testkit/judged/jev_cases.json".to_string()),
    );
    let baseline_path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "docs/development/jev-baseline.tsv".to_string()),
    );
    let record = std::env::var("JEV_BASELINE").as_deref() == Ok("write");
    if std::env::var("KMP_TYPESAFE_CASSETTE").is_err() {
        return Err("set KMP_TYPESAFE_CASSETTE (scripts/ci/jev-baseline.sh does)".into());
    }

    let collection: JudgedCollection = serde_json::from_str(&fs::read_to_string(&cases_path)?)?;
    let mut scores = Scores::default();
    for case in &collection.cases {
        println!("case {}: {}", case.id, case.probes);
        run_case(case, &mut scores).await?;
    }

    let columns = columns(&scores);
    println!("\n{} cases", collection.cases.len());
    for (name, value) in &columns {
        println!("  {name:<28} {value:.4}");
    }
    println!(
        "  (cost, not gated)            {} curate Jev requests, {} input tokens",
        scores.jev_requests, scores.jev_input_tokens
    );
    println!(
        "  (agent load, not gated)      curate by hand reads {} bytes through kmp_relate; kmp_curate without Jev {} bytes; with Jev {} bytes",
        scores.relate_bytes, scores.review_bytes_without_jev, scores.review_bytes
    );
    let asks = scores.ask_mrr_plain.len().max(1) as u128;
    println!(
        "  (latency, not gated)         ask ms/question: plain {}, narrow {}, wide {}",
        scores.ask_ms[0] / asks,
        scores.ask_ms[1] / asks,
        scores.ask_ms[2] / asks
    );

    if std::env::var("JEV_REPORT_ONLY").as_deref() == Ok("1") {
        return Ok(());
    }
    if record {
        write_baseline(&baseline_path, collection.cases.len(), &columns)?;
        println!("\nrecorded baseline at {}", baseline_path.display());
        return Ok(());
    }
    enforce_baseline(&baseline_path, collection.cases.len(), &columns)
}

async fn run_case(case: &JudgedCase, scores: &mut Scores) -> Result<(), Box<dyn Error>> {
    let plain = seeded_server(case, "plain", &[]).await?;
    let judged = seeded_server(case, "judged", &[("typesafe.json", TYPESAFE)]).await?;
    let reranked = seeded_server(
        case,
        "reranked",
        &[("typesafe.json", TYPESAFE), ("rerank.json", RERANK_NARROW)],
    )
    .await?;
    let wide = seeded_server(
        case,
        "wide",
        &[("typesafe.json", TYPESAFE), ("rerank.json", RERANK_WIDE)],
    )
    .await?;
    let expected = &case.expected;

    // Review, every page of it.
    let dimensions = json!({"scope": "abouts", "abouts": case.abouts});
    let mut page = call(
        &judged,
        10,
        "kmp_curate",
        json!({"mode": "review", "about": case.about, "dimensions": dimensions,
               "max_pairs": 40, "page": {"entries": 20}}),
    )
    .await?;
    refuse_unrecorded(&page)?;
    let token = page["review_token"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if let Some(usage) = page["jev"].as_object() {
        scores.jev_requests += usage["requests"].as_u64().unwrap_or(0);
        scores.jev_input_tokens += usage["input_tokens"].as_u64().unwrap_or(0);
    }
    scores.review_bytes += page.to_string().len() as u64;
    let mut found = Vec::new();
    let mut flagged = BTreeSet::new();
    let mut flagged_no_direction = BTreeSet::new();
    let mut id = 11;
    loop {
        for item in page["missing"].as_array().into_iter().flatten() {
            found.push(Found {
                item_id: text(&item["item_id"]),
                from: text(&item["from"]["ref"]),
                to: text(&item["to"]["ref"]),
                suggested: item["suggested_rel"].as_str().map(str::to_string),
                proposed_by: text(&item["proposed_by"]),
            });
        }
        for item in page["suspect"].as_array().into_iter().flatten() {
            let declaration = [
                text(&item["from"]["ref"]),
                text(&item["rel"]),
                text(&item["to"]["ref"]),
            ];
            if item["reasons"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|reason| reason != "direction")
            {
                flagged_no_direction.insert(declaration.clone());
            }
            flagged.insert(declaration);
        }
        let Some(next) = page["next_actions"][0]["arguments"].as_object().cloned() else {
            break;
        };
        id += 1;
        page = call(&judged, id, "kmp_curate", Value::Object(next)).await?;
        scores.review_bytes += page.to_string().len() as u64;
    }
    // kmp_curate without Jev: what the structure alone gives the agent.
    let mut bare = call(
        &plain,
        40,
        "kmp_curate",
        json!({"mode": "review", "about": case.about, "dimensions": dimensions,
               "max_pairs": 40, "page": {"entries": 20}}),
    )
    .await?;
    scores.review_bytes_without_jev += bare.to_string().len() as u64;
    let mut bare_pairs = Vec::new();
    let mut bare_id = 41;
    loop {
        for item in bare["missing"].as_array().into_iter().flatten() {
            bare_pairs.push([text(&item["from"]["ref"]), text(&item["to"]["ref"])]);
        }
        let Some(next) = bare["next_actions"][0]["arguments"].as_object().cloned() else {
            break;
        };
        bare = call(&plain, bare_id, "kmp_curate", Value::Object(next)).await?;
        scores.review_bytes_without_jev += bare.to_string().len() as u64;
        bare_id += 1;
    }
    for gold in &expected.missing {
        scores
            .missing_found_without_jev
            .add(bare_pairs.iter().any(|pair| {
                (pair[0] == gold.pair[0] && pair[1] == gold.pair[1])
                    || (pair[0] == gold.pair[1] && pair[1] == gold.pair[0])
            }));
    }
    // The manual alternative: read the whole selection through kmp_relate.
    let mut reading = call(
        &plain,
        50,
        "kmp_relate",
        json!({"about": case.about, "dimensions": {"scope": "abouts", "abouts": case.abouts},
               "page": {"entries": 256}, "budget": {"max_bytes": 200000}}),
    )
    .await?;
    scores.relate_bytes += reading.to_string().len() as u64;
    let mut relate_id = 51;
    while let Some(next) = reading["next_actions"][0]["arguments"].as_object().cloned() {
        reading = call(&plain, relate_id, "kmp_relate", Value::Object(next)).await?;
        scores.relate_bytes += reading.to_string().len() as u64;
        relate_id += 1;
    }

    // Missing relations: found at all, then typed as a reader accepts.
    for gold in &expected.missing {
        let hit = found.iter().find(|item| same_pair(item, &gold.pair));
        scores.missing_found.add(hit.is_some());
        scores
            .missing_found_by_jev
            .add(hit.is_some_and(|item| item.proposed_by == "jev"));
        let typed = hit
            .and_then(|item| item.suggested.as_ref())
            .is_some_and(|rel| gold.types.contains(rel));
        if hit.is_some() {
            scores.missing_typed.add(typed);
        }
        println!(
            "  missing  {:<28} {:<7} {}",
            gold.kind,
            match hit {
                None => "absent".to_string(),
                Some(item) => item.proposed_by.clone(),
            },
            hit.and_then(|item| item.suggested.clone())
                .map(|rel| format!("{rel}{}", if typed { "" } else { "  (not accepted)" }))
                .unwrap_or_default()
        );
    }
    for distractor in &expected.distractors {
        let typed = found
            .iter()
            .any(|item| same_pair(item, &distractor.pair) && item.suggested.is_some());
        scores.distractors_rejected.add(!typed);
        if typed {
            println!(
                "  distractor proposed: {} ~ {}",
                distractor.pair[0], distractor.pair[1]
            );
        }
    }

    // Suspect audit against planted bad and known good declarations.
    let bad = expected
        .declared_bad
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for declaration in &expected.declared_bad {
        let caught = flagged.contains(declaration);
        scores.suspect_recall.add(caught);
        if !caught {
            println!("  suspect  missed planted bad: {}", declaration.join(" "));
        }
    }
    for declaration in &flagged {
        scores.suspect_precision.add(bad.contains(declaration));
    }
    for declaration in &expected.declared_bad {
        scores
            .suspect_recall_no_direction
            .add(flagged_no_direction.contains(declaration));
    }
    for declaration in &flagged_no_direction {
        scores
            .suspect_precision_no_direction
            .add(bad.contains(declaration));
    }
    for declaration in &expected.declared_good {
        let kept = !flagged.contains(declaration);
        scores.good_kept.add(kept);
        if !kept {
            println!(
                "  suspect  flagged a good declaration: {}",
                declaration.join(" ")
            );
        }
    }
    println!(
        "  suspect  {} flagged; {} of {} planted bad",
        flagged.len(),
        expected
            .declared_bad
            .iter()
            .filter(|d| flagged.contains(*d))
            .count(),
        expected.declared_bad.len()
    );

    // Pre-write check: one apply per item, so each doubt is attributable.
    for (n, check) in expected.precheck.iter().enumerate() {
        let Some(item) = found.iter().find(|item| same_pair(item, &check.pair)) else {
            scores.precheck_available.add(false);
            println!(
                "  precheck {} ~ {}: not reviewed",
                check.pair[0], check.pair[1]
            );
            continue;
        };
        scores.precheck_available.add(true);
        let owner = case
            .abouts
            .iter()
            .find(|about| check.from.starts_with(&format!("{about}:")))
            .ok_or("precheck `from` belongs to no selected about")?;
        let applied = call(
            &judged,
            100 + n as u64,
            "kmp_curate",
            json!({"mode": "apply", "about": owner, "actor": ACTOR, "review_token": token,
                   "accepted": [{"item_id": item.item_id, "rel": check.rel,
                                 "why": check.why, "evidence": check.evidence,
                                 "reverse": item.from != check.from}]}),
        )
        .await?;
        refuse_unrecorded(&applied["curate"])?;
        if let Some(usage) = applied["curate"]["jev"].as_object() {
            scores.jev_requests += usage["requests"].as_u64().unwrap_or(0);
            scores.jev_input_tokens += usage["input_tokens"].as_u64().unwrap_or(0);
        }
        let doubts = applied["curate"]["doubted"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let doubted = !doubts.is_empty();
        let doubted_no_direction = doubts.iter().any(|doubt| {
            doubt["reasons"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|reason| reason != "direction")
        });
        scores.precheck_right.add(doubted == check.doubt);
        scores
            .precheck_right_no_direction
            .add(doubted_no_direction == check.doubt);
        println!(
            "  precheck {:<14} expected {:<6} got {}",
            check.rel,
            if check.doubt { "doubt" } else { "pass" },
            if doubted { "doubt" } else { "pass" }
        );
    }

    // Ask, the same question against the plain, narrow and wide stores.
    for (n, ask) in expected.ask.iter().enumerate() {
        let arguments = json!({"about": case.about, "question": ask.question,
            "dimensions": {"scope": "abouts", "abouts": case.abouts},
            "budget": {"depth": 3, "tokens": 2048, "max_entries": 10}});
        let mut ranks = [None; 3];
        for (arm, server) in [&plain, &reranked, &wide].into_iter().enumerate() {
            let started = std::time::Instant::now();
            let answered = call(
                server,
                200 + (arm as u64) * 100 + n as u64,
                "kmp_ask",
                arguments.clone(),
            )
            .await?;
            scores.ask_ms[arm] += started.elapsed().as_millis();
            refuse_unrecorded(&answered)?;
            ranks[arm] = rank(&answered, &ask.gold);
        }
        let [without, with, wider] = ranks;
        scores.ask_mrr_plain.push(reciprocal(without));
        scores.ask_mrr_rerank.push(reciprocal(with));
        scores.ask_mrr_wide.push(reciprocal(wider));
        scores
            .ask_top_plain
            .add(without.is_some_and(|r| r <= ASK_TOP));
        scores
            .ask_top_rerank
            .add(with.is_some_and(|r| r <= ASK_TOP));
        scores.ask_top_wide.add(wider.is_some_and(|r| r <= ASK_TOP));
        println!(
            "  ask      {:<16} plain {:<5} narrow {:<5} wide {:<5} {}",
            ask.shape,
            show(without),
            show(with),
            show(wider),
            ask.question
        );
    }
    Ok(())
}

async fn seeded_server(
    case: &JudgedCase,
    arm: &str,
    files: &[(&str, &str)],
) -> Result<KernelMcpServer, Box<dyn Error>> {
    let data_dir = std::env::temp_dir().join(format!("kmp-jev-{}-{arm}", case.id));
    let _ = fs::remove_dir_all(&data_dir);
    fs::create_dir_all(&data_dir)?;
    for (name, body) in files {
        fs::write(data_dir.join(name), body)?;
    }
    let server = KernelMcpServer::embedded(&data_dir)?;
    for (index, seeded) in case.memories.iter().enumerate() {
        let receipt = call(
            &server,
            1 + index as u64,
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

/// A replayed run must never answer from a guess: a warning that the
/// cassette lacks a judgement, or that Jev was unavailable, stops it.
fn refuse_unrecorded(value: &Value) -> Result<(), Box<dyn Error>> {
    let warnings = value["warnings"].to_string();
    for marker in [
        "not in the cassette",
        "Jev unavailable",
        "Jev disabled",
        "rerank disabled",
        "rerank unavailable",
        "pre-write check skipped",
    ] {
        if warnings.contains(marker) {
            return Err(
                format!("a judgement was not answered from the cassette: {warnings}").into(),
            );
        }
    }
    Ok(())
}

fn same_pair(item: &Found, pair: &[String; 2]) -> bool {
    (item.from == pair[0] && item.to == pair[1]) || (item.from == pair[1] && item.to == pair[0])
}

/// 1-based rank of the first proof item that supports a gold ref.
fn rank(answer: &Value, gold: &[String]) -> Option<usize> {
    answer["proof"]["evidence"]
        .as_array()
        .into_iter()
        .flatten()
        .position(|item| {
            item["supports"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|support| gold.iter().any(|g| support == g))
                || gold.iter().any(|g| {
                    item["id"]
                        .as_str()
                        .is_some_and(|id| id.ends_with(g.as_str()))
                })
        })
        .map(|index| index + 1)
}

fn reciprocal(rank: Option<usize>) -> f64 {
    rank.map_or(0.0, |rank| 1.0 / rank as f64)
}

fn show(rank: Option<usize>) -> String {
    rank.map_or_else(|| "-".to_string(), |rank| format!("#{rank}"))
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        1.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn columns(scores: &Scores) -> Vec<(&'static str, f64)> {
    vec![
        ("missing_found", scores.missing_found.rate()),
        ("missing_typed", scores.missing_typed.rate()),
        ("distractors_rejected", scores.distractors_rejected.rate()),
        ("missing_found_by_jev", scores.missing_found_by_jev.rate()),
        (
            "missing_found_without_jev",
            scores.missing_found_without_jev.rate(),
        ),
        ("suspect_recall", scores.suspect_recall.rate()),
        ("suspect_precision", scores.suspect_precision.rate()),
        ("good_declarations_kept", scores.good_kept.rate()),
        ("precheck_available", scores.precheck_available.rate()),
        ("precheck_right", scores.precheck_right.rate()),
        (
            "precheck_right_no_direction",
            scores.precheck_right_no_direction.rate(),
        ),
        ("ask_mrr_plain", mean(&scores.ask_mrr_plain)),
        ("ask_mrr_rerank", mean(&scores.ask_mrr_rerank)),
        ("ask_mrr_wide", mean(&scores.ask_mrr_wide)),
        ("ask_top5_plain", scores.ask_top_plain.rate()),
        ("ask_top5_rerank", scores.ask_top_rerank.rate()),
        ("ask_top5_wide", scores.ask_top_wide.rate()),
    ]
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

fn write_baseline(
    path: &Path,
    cases: usize,
    columns: &[(&'static str, f64)],
) -> Result<(), Box<dyn Error>> {
    let mut out = String::from(
        "# Jev baseline: the floors `scripts/ci/jev-baseline.sh` holds TypeSafe Jev with KMP to,\n\
         # measured on the judged corpus against the recorded cassette. missing_* score kmp_curate's\n\
         # missing relations (found at all; typed as a reader accepts), distractors_rejected the pairs\n\
         # that share words without a relation, suspect_* the audit against planted bad and known good\n\
         # declarations, precheck_* the apply pre-write check, ask_* the rank of the answer in proof\n\
         # without and with re-ranking. A number may rise freely; lowering one is a reviewed change\n\
         # that says why, and a re-recorded cassette is one. `cases` is exact.\n\
         # Refresh with: JEV_BASELINE=write bash scripts/ci/jev-baseline.sh\n\
         metric\tfloor\n",
    );
    out.push_str(&format!("cases\t{cases}\n"));
    for (name, value) in columns {
        out.push_str(&format!(
            "{name}\t{:.4}\n",
            (value * 10_000.0).floor() / 10_000.0
        ));
    }
    fs::write(path, out)?;
    Ok(())
}

fn enforce_baseline(
    path: &Path,
    cases: usize,
    columns: &[(&'static str, f64)],
) -> Result<(), Box<dyn Error>> {
    let recorded = fs::read_to_string(path)
        .map_err(|error| format!("no Jev baseline at {}: {error}", path.display()))?;
    let mut floors = BTreeMap::new();
    for line in recorded.lines() {
        if line.starts_with('#') || line.starts_with("metric") || line.trim().is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once('\t')
            .ok_or_else(|| format!("malformed baseline row: {line}"))?;
        floors.insert(name.to_string(), value.trim().parse::<f64>()?);
    }
    let mut regressions = Vec::new();
    if let Some(recorded_cases) = floors.get("cases")
        && (*recorded_cases as usize) != cases
    {
        regressions.push(format!(
            "the corpus changed size: {recorded_cases} judged cases recorded, {cases} run"
        ));
    }
    for (name, measured) in columns {
        if let Some(floor) = floors.get(*name)
            && measured + 1e-9 < *floor
        {
            regressions.push(format!("{name} fell to {measured:.4}, below {floor:.4}"));
        }
    }
    if regressions.is_empty() {
        println!("\nJev baseline holds");
        Ok(())
    } else {
        Err(format!("Jev quality regressed:\n  {}", regressions.join("\n  ")).into())
    }
}
