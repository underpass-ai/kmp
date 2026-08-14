use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    LongMemEvalCandidateTurn, LongMemEvalItem, longmemeval_answer_turn_refs,
    longmemeval_candidate_turns, parse_longmemeval_dataset,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
struct Args {
    input: PathBuf,
    output: PathBuf,
    endpoint: Option<String>,
    model: Option<String>,
    api_key_env: String,
    top_k: usize,
    batch_size: usize,
    limit: Option<usize>,
    per_question_type_limit: Option<usize>,
    question_type: Option<String>,
    run_id: Option<String>,
    include_abstention: bool,
    force: bool,
}

#[derive(Debug, Clone, Default)]
struct EmbeddingUsageTotals {
    prompt_tokens: u64,
    total_tokens: u64,
    requests: usize,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    #[serde(default)]
    usage: Option<EmbeddingUsage>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingUsage {
    #[serde(default)]
    prompt_tokens: Option<u64>,
    #[serde(default)]
    total_tokens: Option<u64>,
}

#[derive(Debug, Serialize)]
struct RankedCandidate {
    rank: usize,
    turn_ref: String,
    score: f64,
    role: String,
    session_id: String,
    session_date: String,
    one_based_turn_index: usize,
    has_answer: bool,
    text_preview: String,
}

#[derive(Debug, Serialize)]
struct EmbeddingCandidateItemResult {
    question_id: String,
    question_type: String,
    candidate_turns: usize,
    top_k: usize,
    gold_turn_refs: Vec<String>,
    selected_turn_refs: Vec<String>,
    hit_turn_refs: Vec<String>,
    missing_turn_refs: Vec<String>,
    evidence_hit: &'static str,
    ranked_candidates: Vec<RankedCandidate>,
    embedding_requests: usize,
    prompt_tokens: u64,
    total_tokens: u64,
    latency_ms: u128,
}

#[derive(Debug, Serialize)]
struct EmbeddingCandidateSummary {
    benchmark: &'static str,
    evaluator: &'static str,
    generated_at_unix_seconds: u64,
    source_path: String,
    endpoint: String,
    model: String,
    run_id: Option<String>,
    top_k: usize,
    batch_size: usize,
    total_items: usize,
    candidate_turns: usize,
    gold_turns: usize,
    selected_turns: usize,
    hit_turns: usize,
    full_hits: usize,
    partial_hits: usize,
    missing_hits: usize,
    not_applicable_hits: usize,
    recall: f64,
    embedding_requests: usize,
    prompt_tokens: u64,
    total_tokens: u64,
    elapsed_ms: u128,
    question_types: BTreeMap<String, EmbeddingCandidateQuestionTypeSummary>,
}

#[derive(Debug, Default, Serialize)]
struct EmbeddingCandidateQuestionTypeSummary {
    count: usize,
    candidate_turns: usize,
    gold_turns: usize,
    selected_turns: usize,
    hit_turns: usize,
    full_hits: usize,
    partial_hits: usize,
    missing_hits: usize,
    not_applicable_hits: usize,
    recall: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    validate_args(&args)?;
    let endpoint = resolve_required(args.endpoint.as_deref(), "EMBEDDING_ENDPOINT", "--endpoint")?;
    let model = resolve_required(args.model.as_deref(), "EMBEDDING_MODEL", "--model")?;
    let api_key = env::var(&args.api_key_env)
        .ok()
        .filter(|value| !value.trim().is_empty());
    ensure_output_dir(&args.output, args.force)?;

    let payload = fs::read_to_string(&args.input)?;
    let dataset = parse_longmemeval_dataset(&payload)?;
    let selected_items = select_items(&dataset, &args);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;
    let started = Instant::now();
    let mut results = Vec::new();

    for item in selected_items {
        let candidates = longmemeval_candidate_turns(item, args.run_id.as_deref())?;
        let gold_turn_refs = longmemeval_answer_turn_refs(item, args.run_id.as_deref())?;
        let query_text = build_query_embedding_text(item);
        let mut embedding_inputs = Vec::with_capacity(candidates.len() + 1);
        embedding_inputs.push(query_text);
        embedding_inputs.extend(candidates.iter().map(build_candidate_embedding_text));

        let call_started = Instant::now();
        let (embeddings, usage) = embed_texts(
            &client,
            &endpoint,
            &model,
            api_key.as_deref(),
            &embedding_inputs,
            args.batch_size,
        )
        .await?;
        let latency_ms = call_started.elapsed().as_millis();
        let (query_embedding, candidate_embeddings) = embeddings
            .split_first()
            .ok_or("embedding response did not include the query embedding")?;
        let ranked_candidates = rank_candidates(
            query_embedding,
            &candidates,
            candidate_embeddings,
            args.top_k,
        )?;
        let selected_turn_refs = ranked_candidates
            .iter()
            .map(|candidate| candidate.turn_ref.clone())
            .collect::<Vec<_>>();
        let (hit_turn_refs, missing_turn_refs, evidence_hit) =
            classify_candidate_recall(&gold_turn_refs, &selected_turn_refs);

        results.push(EmbeddingCandidateItemResult {
            question_id: item.question_id.clone(),
            question_type: item.question_type.clone(),
            candidate_turns: candidates.len(),
            top_k: args.top_k,
            gold_turn_refs,
            selected_turn_refs,
            hit_turn_refs,
            missing_turn_refs,
            evidence_hit,
            ranked_candidates,
            embedding_requests: usage.requests,
            prompt_tokens: usage.prompt_tokens,
            total_tokens: usage.total_tokens,
            latency_ms,
        });
    }

    write_jsonl(
        &args.output.join("candidate_results.jsonl"),
        results.iter().map(serde_json::to_value),
    )?;
    let summary = summarize_run(
        &args,
        endpoint,
        model,
        started.elapsed().as_millis(),
        &results,
    )?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn select_items<'a>(items: &'a [LongMemEvalItem], args: &Args) -> Vec<&'a LongMemEvalItem> {
    let mut selected = Vec::new();
    let mut selected_by_question_type = BTreeMap::<String, usize>::new();
    for item in items {
        if args
            .question_type
            .as_deref()
            .is_some_and(|question_type| item.question_type != question_type)
        {
            continue;
        }
        if item.question_id.ends_with("_abs") && !args.include_abstention {
            continue;
        }
        if args.limit.is_some_and(|limit| selected.len() >= limit) {
            break;
        }
        if args.per_question_type_limit.is_some_and(|limit| {
            selected_by_question_type
                .get(&item.question_type)
                .copied()
                .unwrap_or_default()
                >= limit
        }) {
            continue;
        }
        selected.push(item);
        *selected_by_question_type
            .entry(item.question_type.clone())
            .or_insert(0) += 1;
    }
    selected
}

fn build_query_embedding_text(item: &LongMemEvalItem) -> String {
    format!(
        "Retrieve every conversation turn needed to answer this LongMemEval question.\n\
         Question type: {}\n\
         Question date: {}\n\
         Question: {}",
        item.question_type, item.question_date, item.question
    )
}

fn build_candidate_embedding_text(turn: &LongMemEvalCandidateTurn) -> String {
    format!(
        "Session id: {}\n\
         Session date: {}\n\
         Turn: {}\n\
         Role: {}\n\
         Text: {}",
        turn.session_id,
        turn.session_date,
        turn.one_based_turn_index,
        turn.role,
        turn.content.replace('\n', " ")
    )
}

async fn embed_texts(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
    texts: &[String],
    batch_size: usize,
) -> Result<(Vec<Vec<f32>>, EmbeddingUsageTotals), Box<dyn Error + Send + Sync>> {
    let mut embeddings = vec![None; texts.len()];
    let mut usage = EmbeddingUsageTotals::default();

    for (batch_start, batch) in texts.chunks(batch_size).enumerate() {
        let offset = batch_start * batch_size;
        let response = send_embedding_request(client, endpoint, model, api_key, batch).await?;
        usage.requests += 1;
        if let Some(response_usage) = response.usage {
            usage.prompt_tokens += response_usage.prompt_tokens.unwrap_or_default();
            usage.total_tokens += response_usage.total_tokens.unwrap_or_default();
        }
        let mut seen = vec![false; batch.len()];
        for data in response.data {
            if data.index >= batch.len() {
                return Err(format!(
                    "embedding response returned out-of-range index {} for batch size {}",
                    data.index,
                    batch.len()
                )
                .into());
            }
            if std::mem::replace(&mut seen[data.index], true) {
                return Err(
                    format!("embedding response returned duplicate index {}", data.index).into(),
                );
            }
            embeddings[offset + data.index] = Some(data.embedding);
        }
        if seen.iter().any(|value| !*value) {
            return Err("embedding response did not include every requested input".into());
        }
    }

    embeddings
        .into_iter()
        .enumerate()
        .map(|(index, embedding)| {
            embedding.ok_or_else(|| format!("missing embedding for input index {index}").into())
        })
        .collect::<Result<Vec<_>, Box<dyn Error + Send + Sync>>>()
        .map(|embeddings| (embeddings, usage))
}

async fn send_embedding_request(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
    inputs: &[String],
) -> Result<EmbeddingResponse, Box<dyn Error + Send + Sync>> {
    let mut request = client.post(endpoint).json(&json!({
        "model": model,
        "input": inputs,
    }));
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(format!("embedding request failed with {status}: {body}").into());
    }
    serde_json::from_str::<EmbeddingResponse>(&body)
        .map_err(|error| format!("failed to parse embedding response: {error}; body={body}").into())
}

fn rank_candidates(
    query_embedding: &[f32],
    candidates: &[LongMemEvalCandidateTurn],
    candidate_embeddings: &[Vec<f32>],
    top_k: usize,
) -> Result<Vec<RankedCandidate>, Box<dyn Error + Send + Sync>> {
    if candidates.len() != candidate_embeddings.len() {
        return Err(format!(
            "candidate/embedding length mismatch: {} candidates, {} embeddings",
            candidates.len(),
            candidate_embeddings.len()
        )
        .into());
    }
    let mut scored = candidates
        .iter()
        .zip(candidate_embeddings.iter())
        .map(|(candidate, embedding)| {
            cosine_similarity(query_embedding, embedding).map(|score| (candidate, score))
        })
        .collect::<Result<Vec<_>, _>>()?;

    scored.sort_by(
        |(left_candidate, left_score), (right_candidate, right_score)| {
            right_score
                .total_cmp(left_score)
                .then_with(|| left_candidate.turn_ref.cmp(&right_candidate.turn_ref))
        },
    );

    Ok(scored
        .into_iter()
        .take(top_k)
        .enumerate()
        .map(|(index, (candidate, score))| RankedCandidate {
            rank: index + 1,
            turn_ref: candidate.turn_ref.clone(),
            score,
            role: candidate.role.clone(),
            session_id: candidate.session_id.clone(),
            session_date: candidate.session_date.clone(),
            one_based_turn_index: candidate.one_based_turn_index,
            has_answer: candidate.has_answer,
            text_preview: preview_text(&candidate.content, 180),
        })
        .collect())
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> Result<f64, Box<dyn Error + Send + Sync>> {
    if left.len() != right.len() {
        return Err(format!(
            "embedding dimension mismatch: {} vs {}",
            left.len(),
            right.len()
        )
        .into());
    }
    let mut dot = 0.0f64;
    let mut left_norm = 0.0f64;
    let mut right_norm = 0.0f64;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        let left_value = f64::from(*left_value);
        let right_value = f64::from(*right_value);
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return Err("embedding vector norm must not be zero".into());
    }
    Ok(dot / (left_norm.sqrt() * right_norm.sqrt()))
}

fn classify_candidate_recall(
    gold_turn_refs: &[String],
    selected_turn_refs: &[String],
) -> (Vec<String>, Vec<String>, &'static str) {
    if gold_turn_refs.is_empty() {
        return (Vec::new(), Vec::new(), "not_applicable");
    }
    let selected = selected_turn_refs.iter().collect::<BTreeSet<_>>();
    let hit_turn_refs = gold_turn_refs
        .iter()
        .filter(|turn_ref| selected.contains(turn_ref))
        .cloned()
        .collect::<Vec<_>>();
    let missing_turn_refs = gold_turn_refs
        .iter()
        .filter(|turn_ref| !selected.contains(turn_ref))
        .cloned()
        .collect::<Vec<_>>();

    let evidence_hit = if missing_turn_refs.is_empty() {
        "full"
    } else if !hit_turn_refs.is_empty() {
        "partial"
    } else {
        "missing"
    };

    (hit_turn_refs, missing_turn_refs, evidence_hit)
}

fn summarize_run(
    args: &Args,
    endpoint: String,
    model: String,
    elapsed_ms: u128,
    results: &[EmbeddingCandidateItemResult],
) -> Result<EmbeddingCandidateSummary, Box<dyn Error + Send + Sync>> {
    let mut question_types = BTreeMap::<String, EmbeddingCandidateQuestionTypeSummary>::new();
    for result in results {
        let summary = question_types
            .entry(result.question_type.clone())
            .or_default();
        accumulate_summary(
            summary,
            result.candidate_turns,
            result.gold_turn_refs.len(),
            result.selected_turn_refs.len(),
            result.hit_turn_refs.len(),
            result.evidence_hit,
        );
    }
    for summary in question_types.values_mut() {
        summary.recall = ratio(summary.hit_turns, summary.gold_turns);
    }

    let candidate_turns = results.iter().map(|result| result.candidate_turns).sum();
    let gold_turns = results
        .iter()
        .map(|result| result.gold_turn_refs.len())
        .sum();
    let selected_turns = results
        .iter()
        .map(|result| result.selected_turn_refs.len())
        .sum();
    let hit_turns = results
        .iter()
        .map(|result| result.hit_turn_refs.len())
        .sum();

    Ok(EmbeddingCandidateSummary {
        benchmark: "LongMemEval",
        evaluator: "longmemeval-kmp-embedding-candidates-v1",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        source_path: args.input.display().to_string(),
        endpoint,
        model,
        run_id: args.run_id.clone(),
        top_k: args.top_k,
        batch_size: args.batch_size,
        total_items: results.len(),
        candidate_turns,
        gold_turns,
        selected_turns,
        hit_turns,
        full_hits: results
            .iter()
            .filter(|result| result.evidence_hit == "full")
            .count(),
        partial_hits: results
            .iter()
            .filter(|result| result.evidence_hit == "partial")
            .count(),
        missing_hits: results
            .iter()
            .filter(|result| result.evidence_hit == "missing")
            .count(),
        not_applicable_hits: results
            .iter()
            .filter(|result| result.evidence_hit == "not_applicable")
            .count(),
        recall: ratio(hit_turns, gold_turns),
        embedding_requests: results.iter().map(|result| result.embedding_requests).sum(),
        prompt_tokens: results.iter().map(|result| result.prompt_tokens).sum(),
        total_tokens: results.iter().map(|result| result.total_tokens).sum(),
        elapsed_ms,
        question_types,
    })
}

fn accumulate_summary(
    summary: &mut EmbeddingCandidateQuestionTypeSummary,
    candidate_turns: usize,
    gold_turns: usize,
    selected_turns: usize,
    hit_turns: usize,
    evidence_hit: &str,
) {
    summary.count += 1;
    summary.candidate_turns += candidate_turns;
    summary.gold_turns += gold_turns;
    summary.selected_turns += selected_turns;
    summary.hit_turns += hit_turns;
    match evidence_hit {
        "full" => summary.full_hits += 1,
        "partial" => summary.partial_hits += 1,
        "missing" => summary.missing_hits += 1,
        "not_applicable" => summary.not_applicable_hits += 1,
        _ => {}
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn preview_text(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    normalized.chars().take(max_chars).collect()
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
    I: Iterator<Item = Result<Value, serde_json::Error>>,
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
    cli_value: Option<&str>,
    env_key: &str,
    flag: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    cli_value
        .map(ToString::to_string)
        .or_else(|| env::var(env_key).ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{flag} or {env_key} is required").into())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut input = None;
    let mut output = None;
    let mut endpoint = None;
    let mut model = None;
    let mut api_key_env = "EMBEDDING_API_KEY".to_string();
    let mut top_k = 20usize;
    let mut batch_size = 128usize;
    let mut limit = None;
    let mut per_question_type_limit = None;
    let mut question_type = None;
    let mut run_id = None;
    let mut include_abstention = true;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--endpoint" => endpoint = Some(required_flag_value(&mut args, &arg)?),
            "--model" => model = Some(required_flag_value(&mut args, &arg)?),
            "--api-key-env" => api_key_env = required_flag_value(&mut args, &arg)?,
            "--top-k" => {
                let value = required_flag_value(&mut args, &arg)?;
                top_k = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --top-k value `{value}`: {error}"))?;
            }
            "--batch-size" => {
                let value = required_flag_value(&mut args, &arg)?;
                batch_size = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --batch-size value `{value}`: {error}"))?;
            }
            "--limit" => {
                let value = required_flag_value(&mut args, &arg)?;
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|error| format!("invalid --limit value `{value}`: {error}"))?,
                );
            }
            "--per-question-type-limit" => {
                let value = required_flag_value(&mut args, &arg)?;
                per_question_type_limit = Some(value.parse::<usize>().map_err(|error| {
                    format!("invalid --per-question-type-limit value `{value}`: {error}")
                })?);
            }
            "--question-type" => question_type = Some(required_flag_value(&mut args, &arg)?),
            "--run-id" => run_id = Some(required_flag_value(&mut args, &arg)?),
            "--exclude-abstention" => include_abstention = false,
            "--force" => force = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument `{other}`").into()),
        }
    }

    Ok(Args {
        input: input.ok_or("--input is required")?,
        output: output.ok_or("--output is required")?,
        endpoint,
        model,
        api_key_env,
        top_k,
        batch_size,
        limit,
        per_question_type_limit,
        question_type,
        run_id,
        include_abstention,
        force,
    })
}

fn validate_args(args: &Args) -> Result<(), Box<dyn Error + Send + Sync>> {
    if args.top_k == 0 {
        return Err("--top-k must be greater than zero".into());
    }
    if args.batch_size == 0 {
        return Err("--batch-size must be greater than zero".into());
    }
    Ok(())
}

fn required_flag_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn print_usage() {
    eprintln!(
        "Usage: longmemeval_kmp_embedding_candidates --input <longmemeval.json> --output <out-dir> --endpoint <embeddings-url> --model <model> [--api-key-env EMBEDDING_API_KEY] [--top-k N] [--batch-size N] [--limit N] [--per-question-type-limit N] [--question-type TYPE] [--run-id RUN] [--exclude-abstention] [--force]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_orders_related_embeddings() {
        let query = vec![1.0, 0.0];
        let exact = vec![1.0, 0.0];
        let opposite = vec![-1.0, 0.0];
        let orthogonal = vec![0.0, 1.0];

        assert!(
            cosine_similarity(&query, &exact).expect("exact cosine")
                > cosine_similarity(&query, &orthogonal).expect("orthogonal cosine")
        );
        assert!(
            cosine_similarity(&query, &orthogonal).expect("orthogonal cosine")
                > cosine_similarity(&query, &opposite).expect("opposite cosine")
        );
        assert!(cosine_similarity(&query, &[0.0, 0.0]).is_err());
    }

    #[test]
    fn rank_candidates_uses_score_then_stable_ref_tiebreaker() {
        let candidates = vec![
            candidate("turn:q:s:2", "A"),
            candidate("turn:q:s:1", "B"),
            candidate("turn:q:s:3", "C"),
        ];
        let ranked = rank_candidates(
            &[1.0, 0.0],
            &candidates,
            &[vec![0.8, 0.2], vec![0.8, 0.2], vec![0.0, 1.0]],
            2,
        )
        .expect("ranking should succeed");

        assert_eq!(ranked[0].turn_ref, "turn:q:s:1");
        assert_eq!(ranked[1].turn_ref, "turn:q:s:2");
    }

    #[test]
    fn classify_candidate_recall_reports_full_partial_missing_and_not_applicable() {
        let gold = vec!["a".to_string(), "b".to_string()];
        let selected = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(classify_candidate_recall(&gold, &selected).2, "full");

        let selected = vec!["a".to_string()];
        assert_eq!(classify_candidate_recall(&gold, &selected).2, "partial");

        let selected = vec!["c".to_string()];
        assert_eq!(classify_candidate_recall(&gold, &selected).2, "missing");

        assert_eq!(
            classify_candidate_recall(&[], &["c".to_string()]).2,
            "not_applicable"
        );
    }

    fn candidate(turn_ref: &str, content: &str) -> LongMemEvalCandidateTurn {
        LongMemEvalCandidateTurn {
            turn_ref: turn_ref.to_string(),
            role: "user".to_string(),
            content: content.to_string(),
            session_id: "s".to_string(),
            session_date: "2023/05/20 (Sat) 00:04".to_string(),
            one_based_turn_index: 1,
            has_answer: false,
        }
    }
}
