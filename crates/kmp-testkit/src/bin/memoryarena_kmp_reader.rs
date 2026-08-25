use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    LlmProvider, MemoryArenaExpected, call_llm, memoryarena_answer_candidates_from_text,
    score_memoryarena_answer,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

type AskKey = (String, String, usize);

const DEFAULT_MAX_EVIDENCE_CHARS: usize = 120_000;

#[derive(Debug, Clone)]
struct Args {
    artifacts: PathBuf,
    run: PathBuf,
    output: PathBuf,
    endpoint: Option<String>,
    model: Option<String>,
    provider: Option<LlmProvider>,
    api_key_env: String,
    max_tokens: u32,
    temperature: f64,
    limit: Option<usize>,
    task_type: Option<String>,
    max_evidence_chars: usize,
    force: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RunAskResult {
    task_id: String,
    task_type: String,
    #[serde(default)]
    category: Option<String>,
    subtask_index: usize,
    question: String,
    #[serde(default)]
    ask_content: Value,
}

#[derive(Debug, Clone)]
struct RunRow {
    raw: Value,
    ask: RunAskResult,
}

#[derive(Debug, Clone)]
struct EvidenceSnippet {
    ref_id: String,
    text: String,
    source: Option<String>,
    sequence: Option<u64>,
    relation: Option<String>,
    why: Option<String>,
}

#[derive(Debug, Serialize)]
struct ReaderItemResult {
    task_id: String,
    task_type: String,
    category: Option<String>,
    subtask_index: usize,
    question: String,
    expected_answer: Value,
    hypothesis: String,
    hard_success: bool,
    candidate_answers: Vec<String>,
    answer_source: &'static str,
    evidence_chars: usize,
    prompt_chars: usize,
    prompt_tokens: u32,
    completion_tokens: u32,
    reader_latency_ms: u128,
}

#[derive(Debug, Default, Serialize)]
struct ReaderTaskTypeSummary {
    asks: usize,
    hard_successes: usize,
    prompt_tokens: u32,
    completion_tokens: u32,
    evidence_chars: usize,
    reader_latency_ms: u128,
}

#[derive(Debug, Serialize)]
struct ReaderSummary {
    benchmark: &'static str,
    reader: &'static str,
    schema_version: &'static str,
    generated_at_unix_seconds: u64,
    artifacts: String,
    run: String,
    endpoint: String,
    model: String,
    provider: &'static str,
    task_type_filter: Option<String>,
    total_asks: usize,
    hard_successes: usize,
    hard_success_rate: f64,
    deterministic_answers: usize,
    llm_answers: usize,
    prompt_tokens: u32,
    completion_tokens: u32,
    max_evidence_chars: usize,
    elapsed_ms: u128,
    by_task_type: BTreeMap<String, ReaderTaskTypeSummary>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    let endpoint = resolve_required(args.endpoint.as_deref(), "LLM_ENDPOINT", "--endpoint")?;
    let model = resolve_required(args.model.as_deref(), "LLM_MODEL", "--model")?;
    let provider = match args.provider {
        Some(provider) => provider,
        None => match env::var("LLM_PROVIDER").ok() {
            Some(value) if !value.trim().is_empty() => parse_provider(&value)?,
            _ => detect_provider_from_model(&model),
        },
    };
    let api_key = env::var(&args.api_key_env)
        .ok()
        .filter(|value| !value.trim().is_empty());
    ensure_output_dir(&args.output, args.force)?;

    let expected = read_expected(
        &args.artifacts.join("expected.jsonl"),
        args.task_type.as_deref(),
    )?;
    let expected_by_key = expected_by_ask_key(&expected)?;
    let run_rows = read_run_rows(
        &args.run.join("results.jsonl"),
        args.limit,
        args.task_type.as_deref(),
    )?;
    if run_rows.is_empty() {
        return Err("MemoryArena reader has no run rows after filtering".into());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;
    let started = Instant::now();
    let mut reader_results = Vec::new();
    let mut output_rows = Vec::new();

    for row in run_rows {
        let expected = expected_by_key
            .get(&ask_key(
                &row.ask.task_type,
                &row.ask.task_id,
                row.ask.subtask_index,
            ))
            .ok_or_else(|| {
                format!(
                    "run result has no expected row for task_type {} task {} subtask {}",
                    row.ask.task_type, row.ask.task_id, row.ask.subtask_index
                )
            })?;
        validate_matching_item(expected, &row.ask)?;

        let evidence = recovered_evidence_text(&row.ask.ask_content, args.max_evidence_chars);
        let reader_started = Instant::now();
        let deterministic_hypothesis = deterministic_reader_answer(&row.ask, &evidence);
        let (hypothesis, answer_source, prompt, prompt_tokens, completion_tokens) =
            if let Some(hypothesis) = deterministic_hypothesis {
                (
                    hypothesis,
                    "progressive_exact_answer_candidate",
                    String::new(),
                    0,
                    0,
                )
            } else {
                let prompt = build_reader_prompt(&row.ask, &evidence);
                let (raw_hypothesis, prompt_tokens, completion_tokens) = call_llm(
                    &client,
                    &endpoint,
                    &model,
                    provider,
                    api_key.as_deref(),
                    &prompt,
                    args.max_tokens,
                    args.temperature,
                )
                .await?;
                (
                    normalize_reader_output(&raw_hypothesis),
                    "llm",
                    prompt,
                    prompt_tokens,
                    completion_tokens,
                )
            };
        let reader_latency_ms = reader_started.elapsed().as_millis();
        let answer_score = score_memoryarena_answer(
            &expected.task_type,
            &expected.answer,
            Some(hypothesis.as_str()),
        );
        let candidate_answers = memoryarena_answer_candidates_from_text(&hypothesis);

        reader_results.push(ReaderItemResult {
            task_id: row.ask.task_id.clone(),
            task_type: row.ask.task_type.clone(),
            category: row.ask.category.clone(),
            subtask_index: row.ask.subtask_index,
            question: row.ask.question.clone(),
            expected_answer: expected.answer.clone(),
            hypothesis: hypothesis.clone(),
            hard_success: answer_score.hard_success,
            candidate_answers,
            answer_source,
            evidence_chars: evidence.len(),
            prompt_chars: prompt.len(),
            prompt_tokens,
            completion_tokens,
            reader_latency_ms,
        });
        output_rows.push(rewrite_run_row_with_reader_answer(
            row,
            &endpoint,
            &model,
            provider,
            &hypothesis,
            &evidence,
            &prompt,
            prompt_tokens,
            completion_tokens,
            reader_latency_ms,
            answer_source,
        )?);
    }

    write_jsonl(
        &args.output.join("results.jsonl"),
        output_rows.into_iter().map(Ok),
    )?;
    write_jsonl(
        &args.output.join("reader_results.jsonl"),
        reader_results.iter().map(serde_json::to_value),
    )?;
    write_jsonl(
        &args.output.join("hypotheses.jsonl"),
        reader_results.iter().map(|item| {
            Ok(json!({
                "task_id": item.task_id,
                "task_type": item.task_type,
                "subtask_index": item.subtask_index,
                "hypothesis": item.hypothesis
            }))
        }),
    )?;

    let summary = summarize_reader_run(
        &args,
        endpoint,
        model,
        provider,
        started.elapsed().as_millis(),
        &reader_results,
    )?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn build_reader_prompt(run: &RunAskResult, evidence: &str) -> String {
    let task_instruction = match run.task_type.as_str() {
        "progressive_search" => {
            "Track the accumulating final entity or final answer across the progressive search. Prior feedback may contain an `Exact Answer` candidate from earlier subtasks. When such a candidate is present and still consistent with the current subtask, return that final candidate instead of answering the intermediate subquery literally."
        }
        "formal_reasoning_math" | "formal_reasoning_phys" => {
            "Extract or derive the exact mathematical or physics answer requested by the current subtask. Preserve necessary LaTeX notation. Do not summarize the paper."
        }
        "bundled_shopping" => {
            "Choose the current product only if the current question and recovered memory contain enough product identifiers and constraints. If an ASIN or exact product identifier is not available, answer UNKNOWN."
        }
        "group_travel_planner" => {
            "Compose the current travel-plan answer only from explicit recovered plans, constraints, and current request text. If required catalog data or required slots are missing, answer UNKNOWN."
        }
        _ => {
            "Answer the current MemoryArena subtask directly from the current question and recovered kernel memory."
        }
    };

    format!(
        "You are a MemoryArena reader consuming KMP memory.\n\
         Use only the current question and recovered kernel memory below.\n\
         Do not use outside knowledge. Do not mention evidence, memory, the kernel, or this prompt.\n\
         Return only the final answer, with no explanation, no markdown, and no preamble.\n\
         If the answer is not determined by the current question plus recovered memory, return exactly UNKNOWN.\n\n\
         Task type: {}\n\
         Reader task: {}\n\n\
         Current question:\n{}\n\n\
         Recovered kernel memory:\n{}\n",
        run.task_type, task_instruction, run.question, evidence
    )
}

fn deterministic_reader_answer(run: &RunAskResult, evidence: &str) -> Option<String> {
    if run.task_type != "progressive_search" {
        return None;
    }
    exact_answer_candidates_from_text(evidence)
        .into_iter()
        .rev()
        .find(|candidate| !candidate.trim().eq_ignore_ascii_case("UNKNOWN"))
}

fn exact_answer_candidates_from_text(value: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut use_next_non_empty_line = false;

    for line in value.lines() {
        let normalized_line = normalize_exact_answer_line(line);
        if use_next_non_empty_line {
            if !normalized_line.trim().is_empty() {
                push_unique_candidate(&mut candidates, normalized_line.trim());
                use_next_non_empty_line = false;
            }
            continue;
        }

        let lower = normalized_line.to_ascii_lowercase();
        let Some(label_start) = lower.find("exact answer:") else {
            continue;
        };
        let answer_start = label_start + "exact answer:".len();
        let candidate = normalized_line[answer_start..].trim();
        if candidate.is_empty() {
            use_next_non_empty_line = true;
        } else {
            push_unique_candidate(&mut candidates, candidate);
        }
    }

    candidates
}

fn normalize_exact_answer_line(line: &str) -> String {
    line.trim()
        .trim_start_matches(['-', '*', ' '])
        .replace(['*', '`'], "")
        .trim()
        .to_string()
}

fn push_unique_candidate(candidates: &mut Vec<String>, value: &str) {
    let candidate = value.trim().trim_matches('"').trim();
    if candidate.is_empty() {
        return;
    }
    if !candidates.iter().any(|existing| existing == candidate) {
        candidates.push(candidate.to_string());
    }
}

fn recovered_evidence_text(value: &Value, max_chars: usize) -> String {
    let mut snippets = structured_evidence_snippets(value);
    snippets.sort_by(|left, right| {
        left.sequence
            .unwrap_or(u64::MAX)
            .cmp(&right.sequence.unwrap_or(u64::MAX))
            .then_with(|| left.ref_id.cmp(&right.ref_id))
    });

    if snippets.is_empty() {
        return "No prior kernel evidence was recovered for this ask.".to_string();
    }

    let mut rendered = String::from(
        "Evidence is ordered by kernel sequence when available; later sequence means later memory within this task.\n",
    );
    for (index, snippet) in snippets.iter().enumerate() {
        rendered.push_str(&format!("\n[{}] ref={}", index + 1, snippet.ref_id));
        if let Some(sequence) = snippet.sequence {
            rendered.push_str(&format!(" sequence={sequence}"));
        }
        if let Some(relation) = snippet
            .relation
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            rendered.push_str(&format!(" relation={relation}"));
        }
        rendered.push('\n');
        if let Some(source) = snippet
            .source
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            rendered.push_str(&format!("source: {source}\n"));
        }
        if let Some(why) = snippet
            .why
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            rendered.push_str(&format!("why: {why}\n"));
        }
        rendered.push_str("text: ");
        rendered.push_str(snippet.text.trim());
        rendered.push('\n');
    }

    truncate_chars(&rendered, max_chars)
}

fn structured_evidence_snippets(value: &Value) -> Vec<EvidenceSnippet> {
    let mut snippets = Vec::new();
    snippets.extend(proof_evidence_snippets(value));
    snippets.extend(proof_path_evidence_snippets(value));
    snippets.extend(because_evidence_snippets(value));
    deduplicate_snippets(snippets)
}

fn proof_evidence_snippets(value: &Value) -> Vec<EvidenceSnippet> {
    proof_evidence(value)
        .iter()
        .filter_map(|evidence| {
            let text = evidence.get("text").and_then(Value::as_str)?.trim();
            if text.is_empty() {
                return None;
            }
            let ref_id = evidence
                .get("supports")
                .and_then(Value::as_array)
                .and_then(|supports| {
                    supports
                        .iter()
                        .filter_map(Value::as_str)
                        .find(|ref_id| looks_like_memoryarena_ref(ref_id))
                })
                .or_else(|| evidence.get("id").and_then(Value::as_str))
                .unwrap_or("proof:evidence");
            Some(EvidenceSnippet {
                ref_id: ref_id.to_string(),
                text: text.to_string(),
                source: evidence
                    .get("source")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                sequence: None,
                relation: Some("proof_evidence".to_string()),
                why: None,
            })
        })
        .collect()
}

fn proof_path_evidence_snippets(value: &Value) -> Vec<EvidenceSnippet> {
    proof_path(value)
        .iter()
        .filter_map(|relation| {
            let text = relation.get("evidence").and_then(Value::as_str)?.trim();
            if text.is_empty() {
                return None;
            }
            let ref_id = relation
                .get("from")
                .and_then(Value::as_str)
                .filter(|ref_id| looks_like_memoryarena_ref(ref_id))
                .or_else(|| {
                    relation
                        .get("to")
                        .and_then(Value::as_str)
                        .filter(|ref_id| looks_like_memoryarena_ref(ref_id))
                })
                .unwrap_or("proof:path");
            Some(EvidenceSnippet {
                ref_id: ref_id.to_string(),
                text: text.to_string(),
                source: None,
                sequence: relation.get("sequence").and_then(Value::as_u64),
                relation: relation
                    .get("rel")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                why: relation
                    .get("why")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            })
        })
        .collect()
}

fn because_evidence_snippets(value: &Value) -> Vec<EvidenceSnippet> {
    value
        .get("because")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|reason| {
            let text = reason.get("evidence").and_then(Value::as_str)?.trim();
            if text.is_empty() {
                return None;
            }
            let ref_id = reason
                .get("ref")
                .and_then(Value::as_str)
                .or_else(|| reason.get("claim").and_then(Value::as_str))
                .unwrap_or("because:evidence");
            Some(EvidenceSnippet {
                ref_id: ref_id.to_string(),
                text: text.to_string(),
                source: None,
                sequence: None,
                relation: Some("because".to_string()),
                why: reason
                    .get("claim")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            })
        })
        .collect()
}

fn proof_path(value: &Value) -> &[Value] {
    value
        .get("proof")
        .and_then(|proof| proof.get("path"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn proof_evidence(value: &Value) -> &[Value] {
    value
        .get("proof")
        .and_then(|proof| proof.get("evidence"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn deduplicate_snippets(snippets: Vec<EvidenceSnippet>) -> Vec<EvidenceSnippet> {
    let mut deduplicated = BTreeMap::<(String, String), EvidenceSnippet>::new();
    for snippet in snippets {
        let key = (snippet.ref_id.clone(), snippet.text.clone());
        deduplicated
            .entry(key)
            .and_modify(|existing| {
                if existing.sequence.is_none() {
                    existing.sequence = snippet.sequence;
                }
                if existing.relation.is_none() {
                    existing.relation = snippet.relation.clone();
                }
                if existing.why.is_none() {
                    existing.why = snippet.why.clone();
                }
                if existing.source.is_none() {
                    existing.source = snippet.source.clone();
                }
            })
            .or_insert(snippet);
    }
    deduplicated.into_values().collect()
}

fn looks_like_memoryarena_ref(value: &str) -> bool {
    value.starts_with("memoryarena:") && !value.contains(' ') && value.len() <= 400
}

#[allow(clippy::too_many_arguments)]
fn rewrite_run_row_with_reader_answer(
    row: RunRow,
    endpoint: &str,
    model: &str,
    provider: LlmProvider,
    hypothesis: &str,
    evidence: &str,
    prompt: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
    reader_latency_ms: u128,
    answer_source: &'static str,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    let mut output = row
        .raw
        .as_object()
        .cloned()
        .ok_or("run result row must be a JSON object")?;
    output.insert(
        "kmp_ask_answer".to_string(),
        output.get("ask_answer").cloned().unwrap_or(Value::Null),
    );
    output.insert(
        "ask_answer".to_string(),
        Value::String(hypothesis.to_string()),
    );
    output.insert(
        "memoryarena_reader".to_string(),
        json!({
            "reader": "memoryarena-kmp-reader-v1",
            "endpoint": endpoint,
            "model": model,
            "provider": provider_label(provider),
            "evidence_chars": evidence.len(),
            "prompt_chars": prompt.len(),
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "reader_latency_ms": reader_latency_ms,
            "answer_source": answer_source
        }),
    );
    Ok(Value::Object(output))
}

fn normalize_reader_output(value: &str) -> String {
    let mut normalized = value
        .trim()
        .trim_matches('"')
        .trim()
        .trim_start_matches("Answer:")
        .trim()
        .to_string();
    if normalized.starts_with("```") && normalized.ends_with("```") {
        normalized = normalized
            .trim_start_matches("```")
            .trim_start_matches(|ch: char| ch.is_ascii_alphabetic())
            .trim()
            .trim_end_matches("```")
            .trim()
            .to_string();
    }
    normalized
}

fn summarize_reader_run(
    args: &Args,
    endpoint: String,
    model: String,
    provider: LlmProvider,
    elapsed_ms: u128,
    results: &[ReaderItemResult],
) -> Result<ReaderSummary, Box<dyn Error + Send + Sync>> {
    let mut by_task_type = BTreeMap::<String, ReaderTaskTypeSummary>::new();
    for result in results {
        let summary = by_task_type.entry(result.task_type.clone()).or_default();
        summary.asks += 1;
        summary.hard_successes += usize::from(result.hard_success);
        summary.prompt_tokens += result.prompt_tokens;
        summary.completion_tokens += result.completion_tokens;
        summary.evidence_chars += result.evidence_chars;
        summary.reader_latency_ms += result.reader_latency_ms;
    }
    let hard_successes = results.iter().filter(|result| result.hard_success).count();
    let deterministic_answers = results
        .iter()
        .filter(|result| result.answer_source != "llm")
        .count();
    let llm_answers = results
        .iter()
        .filter(|result| result.answer_source == "llm")
        .count();

    Ok(ReaderSummary {
        benchmark: "MemoryArena",
        reader: "memoryarena-kmp-reader-v1",
        schema_version: "memoryarena-kmp-reader-summary-v1",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        artifacts: args.artifacts.display().to_string(),
        run: args.run.display().to_string(),
        endpoint,
        model,
        provider: provider_label(provider),
        task_type_filter: args.task_type.clone(),
        total_asks: results.len(),
        hard_successes,
        hard_success_rate: ratio(hard_successes, results.len()),
        deterministic_answers,
        llm_answers,
        prompt_tokens: results.iter().map(|result| result.prompt_tokens).sum(),
        completion_tokens: results.iter().map(|result| result.completion_tokens).sum(),
        max_evidence_chars: args.max_evidence_chars,
        elapsed_ms,
        by_task_type,
    })
}

fn read_expected(
    path: &Path,
    task_type: Option<&str>,
) -> Result<Vec<MemoryArenaExpected>, Box<dyn Error + Send + Sync>> {
    let expected = read_jsonl(path, None)?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<MemoryArenaExpected>, _>>()?
        .into_iter()
        .filter(|expected| task_type.is_none_or(|task_type| expected.task_type == task_type))
        .collect::<Vec<_>>();
    Ok(expected)
}

fn read_run_rows(
    path: &Path,
    limit: Option<usize>,
    task_type: Option<&str>,
) -> Result<Vec<RunRow>, Box<dyn Error + Send + Sync>> {
    let mut rows = Vec::new();
    for raw in read_jsonl(path, None)? {
        let ask = serde_json::from_value::<RunAskResult>(raw.clone())?;
        if task_type.is_some_and(|task_type| ask.task_type != task_type) {
            continue;
        }
        rows.push(RunRow { raw, ask });
        if limit.is_some_and(|limit| rows.len() >= limit) {
            break;
        }
    }
    Ok(rows)
}

fn expected_by_ask_key(
    expected: &[MemoryArenaExpected],
) -> Result<BTreeMap<AskKey, &MemoryArenaExpected>, Box<dyn Error + Send + Sync>> {
    let mut by_key = BTreeMap::new();
    for item in expected {
        let key = ask_key(&item.task_type, &item.task_id, item.subtask_index);
        if by_key.insert(key.clone(), item).is_some() {
            return Err(format!(
                "duplicate expected row for task_type {} task {} subtask {}",
                key.0, key.1, key.2
            )
            .into());
        }
    }
    Ok(by_key)
}

fn validate_matching_item(
    expected: &MemoryArenaExpected,
    run: &RunAskResult,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if expected.task_id != run.task_id {
        return Err(format!(
            "task_id mismatch: expected={} run={}",
            expected.task_id, run.task_id
        )
        .into());
    }
    if expected.task_type != run.task_type {
        return Err(format!(
            "task_type mismatch for {}: expected={} run={}",
            expected.task_id, expected.task_type, run.task_type
        )
        .into());
    }
    if expected.subtask_index != run.subtask_index {
        return Err(format!(
            "subtask_index mismatch for {}: expected={} run={}",
            expected.task_id, expected.subtask_index, run.subtask_index
        )
        .into());
    }
    Ok(())
}

fn read_jsonl(
    path: &Path,
    limit: Option<usize>,
) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        if limit.is_some_and(|limit| values.len() >= limit) {
            break;
        }
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str::<Value>(&line).map_err(|error| {
            format!(
                "invalid JSONL at {}:{}: {error}",
                path.display(),
                line_index + 1
            )
        })?);
    }
    Ok(values)
}

fn ensure_output_dir(output: &Path, force: bool) -> Result<(), Box<dyn Error + Send + Sync>> {
    if output.exists() {
        if !output.is_dir() {
            return Err(format!("output path is not a directory: {}", output.display()).into());
        }
        if !force && output.read_dir()?.next().is_some() {
            return Err(format!(
                "output directory is not empty: {} (use --force to overwrite known artifact files)",
                output.display()
            )
            .into());
        }
    }
    fs::create_dir_all(output)?;
    Ok(())
}

fn write_jsonl<I>(path: &Path, values: I) -> Result<(), Box<dyn Error + Send + Sync>>
where
    I: IntoIterator<Item = Result<Value, serde_json::Error>>,
{
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    for value in values {
        serde_json::to_writer(&mut writer, &value?)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
}

fn write_json_pretty<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn resolve_required(
    explicit: Option<&str>,
    env_key: &str,
    flag: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    explicit
        .map(ToString::to_string)
        .or_else(|| env::var(env_key).ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing {flag} or {env_key}").into())
}

fn detect_provider_from_model(model: &str) -> LlmProvider {
    if model.starts_with("claude") {
        LlmProvider::Anthropic
    } else if model.starts_with("gpt-5") || model.starts_with("o3") || model.starts_with("o4") {
        LlmProvider::OpenAINew
    } else {
        LlmProvider::OpenAI
    }
}

fn parse_provider(value: &str) -> Result<LlmProvider, Box<dyn Error + Send + Sync>> {
    match value.trim() {
        "anthropic" => Ok(LlmProvider::Anthropic),
        "openai" => Ok(LlmProvider::OpenAI),
        "openai-new" | "openai_new" => Ok(LlmProvider::OpenAINew),
        other => Err(format!(
            "unsupported provider `{other}`; use openai, openai-new, or anthropic"
        )
        .into()),
    }
}

fn provider_label(provider: LlmProvider) -> &'static str {
    match provider {
        LlmProvider::OpenAI => "openai",
        LlmProvider::OpenAINew => "openai-new",
        LlmProvider::Anthropic => "anthropic",
    }
}

fn parse_args<I>(mut args: I) -> Result<Args, Box<dyn Error + Send + Sync>>
where
    I: Iterator<Item = String>,
{
    let mut artifacts = None;
    let mut run = None;
    let mut output = None;
    let mut endpoint = None;
    let mut model = None;
    let mut provider = None;
    let mut api_key_env = "LLM_API_KEY".to_string();
    let mut max_tokens = 512u32;
    let mut temperature = 0.0f64;
    let mut limit = None;
    let mut task_type = None;
    let mut max_evidence_chars = DEFAULT_MAX_EVIDENCE_CHARS;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--artifacts" => artifacts = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--run" => run = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--endpoint" => endpoint = Some(required_flag_value(&mut args, &arg)?),
            "--model" => model = Some(required_flag_value(&mut args, &arg)?),
            "--provider" => {
                provider = Some(parse_provider(&required_flag_value(&mut args, &arg)?)?)
            }
            "--api-key-env" => api_key_env = required_flag_value(&mut args, &arg)?,
            "--max-tokens" => {
                max_tokens = required_flag_value(&mut args, &arg)?
                    .parse()
                    .map_err(|error| format!("invalid --max-tokens value: {error}"))?
            }
            "--temperature" => {
                temperature = required_flag_value(&mut args, &arg)?
                    .parse()
                    .map_err(|error| format!("invalid --temperature value: {error}"))?
            }
            "--limit" => {
                limit = Some(
                    required_flag_value(&mut args, &arg)?
                        .parse()
                        .map_err(|error| format!("invalid --limit value: {error}"))?,
                )
            }
            "--task-type" => task_type = Some(required_flag_value(&mut args, &arg)?),
            "--max-evidence-chars" => {
                max_evidence_chars = required_flag_value(&mut args, &arg)?
                    .parse()
                    .map_err(|error| format!("invalid --max-evidence-chars value: {error}"))?
            }
            "--force" => force = true,
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument `{other}`").into()),
        }
    }

    Ok(Args {
        artifacts: artifacts.ok_or("missing --artifacts")?,
        run: run.ok_or("missing --run")?,
        output: output.ok_or("missing --output")?,
        endpoint,
        model,
        provider,
        api_key_env,
        max_tokens,
        temperature,
        limit,
        task_type,
        max_evidence_chars,
        force,
    })
}

fn required_flag_value<I>(args: &mut I, flag: &str) -> Result<String, Box<dyn Error + Send + Sync>>
where
    I: Iterator<Item = String>,
{
    args.next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing value for {flag}").into())
}

fn print_usage() {
    eprintln!(
        "Usage: memoryarena_kmp_reader --artifacts <adapter-output-dir> --run <runner-output-dir> --output <reader-output-dir> --endpoint <chat-completions-url> --model <model> [--provider openai|openai-new|anthropic] [--api-key-env LLM_API_KEY] [--max-tokens N] [--temperature F] [--limit N] [--task-type TYPE] [--max-evidence-chars N] [--force]"
    );
}

fn ask_key(task_type: &str, task_id: &str, subtask_index: usize) -> AskKey {
    (task_type.to_string(), task_id.to_string(), subtask_index)
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    if max_chars < 64 {
        return value.chars().take(max_chars).collect();
    }
    let head_chars = max_chars / 2;
    let tail_chars = max_chars - head_chars;
    let head = value.chars().take(head_chars).collect::<String>();
    let tail = value
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}\n\n[... evidence truncated ...]\n\n{tail}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn structured_evidence_reads_memoryarena_proof_evidence() {
        let value = json!({
            "proof": {
                "evidence": [{
                    "id": "detail:evidence:1",
                    "source": "source:1",
                    "supports": [
                        "memoryarena:run:r:task_type:progressive_search:task:1:subtask:1:answer"
                    ],
                    "text": "Exact Answer: Ada Lovelace"
                }]
            }
        });

        let evidence = recovered_evidence_text(&value, 10_000);

        assert!(evidence.contains("Ada Lovelace"));
        assert!(evidence.contains("memoryarena:run:r"));
    }

    #[test]
    fn prompt_keeps_current_question_separate_from_prior_memory() {
        let run = RunAskResult {
            task_id: "1".to_string(),
            task_type: "progressive_search".to_string(),
            category: None,
            subtask_index: 2,
            question: "Who is the person?".to_string(),
            ask_content: Value::Null,
        };

        let prompt = build_reader_prompt(&run, "Exact Answer: Ada Lovelace");

        assert!(prompt.contains("Current question:\nWho is the person?"));
        assert!(prompt.contains("Recovered kernel memory:\nExact Answer: Ada Lovelace"));
    }

    #[test]
    fn normalize_reader_output_strips_answer_label_and_fences() {
        assert_eq!(
            normalize_reader_output(" Answer: Ada Lovelace "),
            "Ada Lovelace"
        );
        assert_eq!(
            normalize_reader_output("```text\nAda Lovelace\n```"),
            "Ada Lovelace"
        );
    }

    #[test]
    fn deterministic_progressive_policy_requires_explicit_exact_answer() {
        let run = RunAskResult {
            task_id: "1".to_string(),
            task_type: "progressive_search".to_string(),
            category: None,
            subtask_index: 1,
            question: "Who?".to_string(),
            ask_content: Value::Null,
        };

        assert_eq!(
            deterministic_reader_answer(
                &run,
                "No prior kernel evidence was recovered for this ask."
            ),
            None
        );
        assert_eq!(
            deterministic_reader_answer(&run, "Evidence\nExact Answer: Ada Lovelace"),
            Some("Ada Lovelace".to_string())
        );
    }
}
