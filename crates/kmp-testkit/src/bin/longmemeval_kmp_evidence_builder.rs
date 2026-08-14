use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    LlmProvider, LongMemEvalCandidateTurn, LongMemEvalEvidenceLabels, LongMemEvalEvidenceTurnLabel,
    LongMemEvalItem, call_llm, longmemeval_candidate_turns, normalize_llm_json_response,
    parse_longmemeval_dataset,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
struct Args {
    input: PathBuf,
    output: PathBuf,
    endpoint: Option<String>,
    model: Option<String>,
    provider: Option<LlmProvider>,
    api_key_env: String,
    max_tokens: u32,
    temperature: f64,
    limit: Option<usize>,
    per_question_type_limit: Option<usize>,
    question_type: Option<String>,
    run_id: Option<String>,
    include_abstention: bool,
    force: bool,
}

#[derive(Debug, Deserialize)]
struct BuilderResponse {
    evidence_turn_refs: Vec<BuilderEvidenceRef>,
}

#[derive(Debug, Deserialize)]
struct BuilderEvidenceRef {
    r#ref: String,
    reason: Option<String>,
    confidence: Option<String>,
}

#[derive(Debug, Serialize)]
struct BuilderItemResult {
    question_id: String,
    question_type: String,
    candidate_turns: usize,
    selected_turns: usize,
    labels: LongMemEvalEvidenceLabels,
    prompt_tokens: u32,
    completion_tokens: u32,
    latency_ms: u128,
}

#[derive(Debug, Serialize)]
struct BuilderSummary {
    benchmark: &'static str,
    builder: &'static str,
    generated_at_unix_seconds: u64,
    source_path: String,
    endpoint: String,
    model: String,
    provider: &'static str,
    run_id: Option<String>,
    total_items: usize,
    selected_turns: usize,
    prompt_tokens: u32,
    completion_tokens: u32,
    elapsed_ms: u128,
    question_types: BTreeMap<String, BuilderQuestionTypeSummary>,
}

#[derive(Debug, Default, Serialize)]
struct BuilderQuestionTypeSummary {
    count: usize,
    selected_turns: usize,
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

    let payload = fs::read_to_string(&args.input)?;
    let dataset = parse_longmemeval_dataset(&payload)?;
    let selected_items = select_items(&dataset, &args);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;
    let started = Instant::now();
    let mut results = Vec::new();
    let mut labels_writer =
        BufWriter::new(File::create(args.output.join("evidence_labels.jsonl"))?);
    let mut results_writer =
        BufWriter::new(File::create(args.output.join("builder_results.jsonl"))?);

    let total_items = selected_items.len();
    for (item_index, item) in selected_items.into_iter().enumerate() {
        eprintln!(
            "longmemeval evidence builder item {}/{} question_id={} question_type={}",
            item_index + 1,
            total_items,
            item.question_id,
            item.question_type
        );
        let candidates = longmemeval_candidate_turns(item, args.run_id.as_deref())?;
        let prompt = build_builder_prompt(item, &candidates);
        let call_started = Instant::now();
        let (raw_response, prompt_tokens, completion_tokens) = call_llm(
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
        let latency_ms = call_started.elapsed().as_millis();
        let labels = parse_builder_labels(item, &candidates, &raw_response)?;

        let result = BuilderItemResult {
            question_id: item.question_id.clone(),
            question_type: item.question_type.clone(),
            candidate_turns: candidates.len(),
            selected_turns: labels.evidence_turns.len(),
            labels,
            prompt_tokens,
            completion_tokens,
            latency_ms,
        };
        serde_json::to_writer(&mut labels_writer, &result.labels)?;
        labels_writer.write_all(b"\n")?;
        labels_writer.flush()?;
        serde_json::to_writer(&mut results_writer, &result)?;
        results_writer.write_all(b"\n")?;
        results_writer.flush()?;
        eprintln!(
            "longmemeval evidence builder item {}/{} done question_id={} selected_turns={} latency_ms={}",
            item_index + 1,
            total_items,
            result.question_id,
            result.selected_turns,
            result.latency_ms
        );
        results.push(result);
    }
    labels_writer.flush()?;
    results_writer.flush()?;
    let summary = summarize_builder_run(
        &args,
        endpoint,
        model,
        provider,
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

fn build_builder_prompt(item: &LongMemEvalItem, candidates: &[LongMemEvalCandidateTurn]) -> String {
    let selection_instruction = match item.question_type.as_str() {
        "single-session-preference" => {
            "Select a compact but complete set of user turns that support a personalized response. For preference questions, missing a personal constraint is worse than including one extra related user turn."
        }
        _ => "Select the minimal set of turns that directly support answering the question.",
    };
    let type_specific_instruction = match item.question_type.as_str() {
        "single-session-preference" => {
            "For preference questions, select turns that reveal durable user preferences, transferable constraints, accepted/rejected options, and follow-up refinements. Prefer user-authored turns over generic assistant recommendations. Include all user turns needed to infer both what the user would prefer and what they would not prefer. If the question asks for future advice or a recommendation in a new context, include the turns that expose portable preference dimensions instead of only the final accepted item. Include follow-up user turns that narrow an ingredient, material, tool, technique, maintenance concern, language, skill, venue type, activity format, or desired workflow even when they are phrased as requests for advice. When the question names a broad object or workflow, include user turns with concrete examples, experiments, recipe types, product types, or subgoals inside that object or workflow; do not let a later related preference replace those earlier concrete interests. Do not stop after the first near-match; scan later user turns in the same session for refinements that specialize or combine the same topic. Do not select only the final choice if earlier turns explain the underlying preference."
        }
        "multi-session" => {
            "For multi-session questions, include every user or assistant turn that is a member of the requested count, sum, list, average, maximum, minimum, comparison, or aggregate."
        }
        "temporal-reasoning" => {
            "For temporal questions, include every dated event needed to determine ordering, duration, recency, before/after, or elapsed time."
        }
        "knowledge-update" => {
            "For knowledge-update questions, include both superseded and latest facts when an update happened."
        }
        _ => "",
    };
    let mut prompt = format!(
        "You are building evidence relations for LongMemEval.\n\
         {selection_instruction}\n\
         Do not use the gold answer. Do not use outside knowledge.\n\
         Prefer complete evidence over sparse evidence: for aggregation, include every counted item; for temporal questions, include all dated events needed for comparison or duration; for knowledge updates, include old and new facts when needed; for preferences, include the personal preference evidence.\n\
         The ref value must be copied exactly from the candidate bracket header. Do not abbreviate refs, do not write session/turn labels, and do not invent ids.\n\
         {type_specific_instruction}\n\
         Return strict JSON only, no markdown, with this shape:\n\
         {{\"evidence_turn_refs\":[{{\"ref\":\"turn:...\",\"reason\":\"why this turn is needed\",\"confidence\":\"high|medium|low|unknown\"}}]}}\n\n\
         Question id: {}\n\
         Question type: {}\n\
         Question date: {}\n\
         Question: {}\n\n\
         Candidate turns:\n",
        item.question_id, item.question_type, item.question_date, item.question
    );

    for turn in candidates {
        prompt.push_str(&format!(
            "\n[{}]\nSession: {} ({})\nTurn: {}\nRole: {}\nText: {}\n",
            turn.turn_ref,
            turn.session_id,
            turn.session_date,
            turn.one_based_turn_index,
            turn.role,
            turn.content.replace('\n', " ")
        ));
    }
    prompt
}

fn parse_builder_labels(
    item: &LongMemEvalItem,
    candidates: &[LongMemEvalCandidateTurn],
    raw_response: &str,
) -> Result<LongMemEvalEvidenceLabels, Box<dyn Error + Send + Sync>> {
    let candidate_refs = candidates
        .iter()
        .map(|turn| turn.turn_ref.as_str())
        .collect::<BTreeSet<_>>();
    let normalized = normalize_llm_json_response(raw_response);
    let response = serde_json::from_str::<BuilderResponse>(&normalized).map_err(|error| {
        format!(
            "LLM evidence builder returned invalid JSON for {}: {error}; raw={}",
            item.question_id, raw_response
        )
    })?;

    let mut seen = BTreeSet::new();
    let mut evidence_turns = Vec::new();
    for evidence in response.evidence_turn_refs {
        if !candidate_refs.contains(evidence.r#ref.as_str()) {
            return Err(format!(
                "LLM evidence builder selected unknown turn ref for {}: {}",
                item.question_id, evidence.r#ref
            )
            .into());
        }
        if !seen.insert(evidence.r#ref.clone()) {
            return Err(format!(
                "LLM evidence builder selected duplicate turn ref for {}: {}",
                item.question_id, evidence.r#ref
            )
            .into());
        }
        evidence_turns.push(LongMemEvalEvidenceTurnLabel {
            turn_ref: evidence.r#ref,
            reason: evidence
                .reason
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or_else(|| "LLM selected this turn as answer evidence.".to_string()),
            confidence: normalize_confidence(evidence.confidence.as_deref())?.to_string(),
        });
    }
    Ok(LongMemEvalEvidenceLabels {
        question_id: item.question_id.clone(),
        evidence_turns,
    })
}

fn normalize_confidence(value: Option<&str>) -> Result<&'static str, Box<dyn Error + Send + Sync>> {
    match value
        .unwrap_or("unknown")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "high" => Ok("high"),
        "medium" => Ok("medium"),
        "low" => Ok("low"),
        "unknown" | "" => Ok("unknown"),
        other => Err(format!("invalid LLM evidence confidence `{other}`").into()),
    }
}

fn summarize_builder_run(
    args: &Args,
    endpoint: String,
    model: String,
    provider: LlmProvider,
    elapsed_ms: u128,
    results: &[BuilderItemResult],
) -> Result<BuilderSummary, Box<dyn Error + Send + Sync>> {
    let mut question_types = BTreeMap::<String, BuilderQuestionTypeSummary>::new();
    for result in results {
        let summary = question_types
            .entry(result.question_type.clone())
            .or_default();
        summary.count += 1;
        summary.selected_turns += result.selected_turns;
    }
    Ok(BuilderSummary {
        benchmark: "LongMemEval",
        builder: "longmemeval-kmp-evidence-builder-v1",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        source_path: args.input.display().to_string(),
        endpoint,
        model,
        provider: provider_label(provider),
        run_id: args.run_id.clone(),
        total_items: results.len(),
        selected_turns: results.iter().map(|result| result.selected_turns).sum(),
        prompt_tokens: results.iter().map(|result| result.prompt_tokens).sum(),
        completion_tokens: results.iter().map(|result| result.completion_tokens).sum(),
        elapsed_ms,
        question_types,
    })
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
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" => Ok(LlmProvider::OpenAI),
        "openai-new" | "openai_new" => Ok(LlmProvider::OpenAINew),
        "anthropic" => Ok(LlmProvider::Anthropic),
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

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut input = None;
    let mut output = None;
    let mut endpoint = None;
    let mut model = None;
    let mut provider = None;
    let mut api_key_env = "LLM_API_KEY".to_string();
    let mut max_tokens = 512u32;
    let mut temperature = 0.0f64;
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
            "--provider" => {
                provider = Some(parse_provider(&required_flag_value(&mut args, &arg)?)?);
            }
            "--api-key-env" => api_key_env = required_flag_value(&mut args, &arg)?,
            "--max-tokens" => {
                let value = required_flag_value(&mut args, &arg)?;
                max_tokens = value
                    .parse::<u32>()
                    .map_err(|error| format!("invalid --max-tokens value `{value}`: {error}"))?;
            }
            "--temperature" => {
                let value = required_flag_value(&mut args, &arg)?;
                temperature = value
                    .parse::<f64>()
                    .map_err(|error| format!("invalid --temperature value `{value}`: {error}"))?;
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
        provider,
        api_key_env,
        max_tokens,
        temperature,
        limit,
        per_question_type_limit,
        question_type,
        run_id,
        include_abstention,
        force,
    })
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
        "Usage: longmemeval_kmp_evidence_builder --input <longmemeval.json> --output <labels-dir> --endpoint <chat-completions-url> --model <model> [--provider openai|openai-new|anthropic] [--api-key-env LLM_API_KEY] [--max-tokens N] [--temperature F] [--limit N] [--per-question-type-limit N] [--question-type TYPE] [--run-id RUN] [--exclude-abstention] [--force]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builder_labels_and_rejects_unknown_refs() {
        let item = LongMemEvalItem {
            question_id: "q1".to_string(),
            question_type: "single-session-user".to_string(),
            question: "Where?".to_string(),
            answer: serde_json::json!("Austin"),
            question_date: "2023/05/21 (Sun) 12:00".to_string(),
            haystack_session_ids: vec!["s1".to_string()],
            haystack_dates: vec!["2023/05/20 (Sat) 00:04".to_string()],
            haystack_sessions: vec![vec![kmp_testkit::LongMemEvalTurn {
                role: "user".to_string(),
                content: "Austin.".to_string(),
                has_answer: false,
            }]],
            answer_session_ids: Vec::new(),
        };
        let candidates =
            longmemeval_candidate_turns(&item, None).expect("candidate turns should build");
        let labels = parse_builder_labels(
            &item,
            &candidates,
            r#"{"evidence_turn_refs":[{"ref":"turn:q1:s1:1","reason":"direct","confidence":"high"}]}"#,
        )
        .expect("labels should parse");

        assert_eq!(labels.evidence_turns[0].turn_ref, "turn:q1:s1:1");
        assert!(
            parse_builder_labels(
                &item,
                &candidates,
                r#"{"evidence_turn_refs":[{"ref":"turn:q1:s1:2"}]}"#
            )
            .is_err()
        );
        assert!(
            parse_builder_labels(
                &item,
                &candidates,
                r#"{"evidence_turn_refs":[{"ref":"turn:q1:s1:1","confidence":"made_up_confidence"}]}"#
            )
            .is_err()
        );
    }

    #[test]
    fn preference_prompt_requests_transferable_constraints_and_refinements() {
        let item = LongMemEvalItem {
            question_id: "q-pref".to_string(),
            question_type: "single-session-preference".to_string(),
            question: "Can you suggest a hotel for my upcoming trip?".to_string(),
            answer: serde_json::json!("hotel preference"),
            question_date: "2023/05/21 (Sun) 12:00".to_string(),
            haystack_session_ids: vec!["s1".to_string()],
            haystack_dates: vec!["2023/05/20 (Sat) 00:04".to_string()],
            haystack_sessions: vec![vec![kmp_testkit::LongMemEvalTurn {
                role: "user".to_string(),
                content: "I like great views and unique amenities.".to_string(),
                has_answer: false,
            }]],
            answer_session_ids: Vec::new(),
        };
        let prompt = build_builder_prompt(&item, &[]);

        assert!(prompt.contains("transferable constraints"));
        assert!(prompt.contains("portable preference dimensions"));
        assert!(prompt.contains("missing a personal constraint is worse"));
        assert!(prompt.contains("follow-up user turns"));
        assert!(prompt.contains("concrete examples, experiments"));
        assert!(prompt.contains("Do not stop after the first near-match"));
        assert!(prompt.contains("maintenance concern"));
    }
}
