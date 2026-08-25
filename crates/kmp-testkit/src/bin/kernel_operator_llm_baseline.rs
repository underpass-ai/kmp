use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    LlmProvider, call_llm, detect_provider_from_model, kernel_operator_action_shape_error,
    kernel_operator_is_bounded_tool_call, kernel_operator_is_valid_action_shape,
    kernel_operator_primary_refs, normalize_llm_json_response, parse_provider,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const BASELINE: &str = "kernel-operator-llm-baseline-v1";

#[derive(Debug, Clone)]
struct Args {
    trajectories: PathBuf,
    output: PathBuf,
    endpoint: Option<String>,
    model: Option<String>,
    provider: Option<LlmProvider>,
    api_key_env: String,
    max_tokens: u32,
    temperature: f64,
    limit: Option<usize>,
    offset: usize,
    max_refs: usize,
    force: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct TrajectoryInput {
    step_id: String,
    about: String,
    task_family: String,
    mode: String,
    visible_state: Value,
    allowed_tools: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PredictionRow {
    step_id: String,
    action: Value,
}

#[derive(Debug, Serialize)]
struct LlmResultRow {
    step_id: String,
    action: Option<Value>,
    valid_action: bool,
    bounded_action: bool,
    prompt_tokens: u32,
    completion_tokens: u32,
    latency_ms: u128,
    raw_response: String,
}

#[derive(Debug, Serialize)]
struct FailureRow {
    step_id: String,
    reason: String,
    raw_response: Option<String>,
    detail: Value,
}

#[derive(Debug, Serialize)]
struct BaselineSummary {
    baseline: &'static str,
    generated_at_unix_seconds: u64,
    trajectories: String,
    output: String,
    endpoint: String,
    model: String,
    provider: &'static str,
    total_selected: usize,
    predictions: usize,
    llm_results: usize,
    failures: usize,
    invalid_actions: usize,
    unbounded_actions: usize,
    prompt_tokens: u32,
    completion_tokens: u32,
    elapsed_ms: u128,
    by_action: BTreeMap<String, usize>,
}

struct SummaryInput<'a> {
    args: &'a Args,
    endpoint: String,
    model: String,
    provider: LlmProvider,
    trajectories: &'a [TrajectoryInput],
    predictions: &'a [PredictionRow],
    llm_results: &'a [LlmResultRow],
    failures: &'a [FailureRow],
    elapsed_ms: u128,
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

    let trajectories = select_trajectories(read_trajectories(&args.trajectories)?, &args);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;
    let started = Instant::now();
    let mut predictions = Vec::<PredictionRow>::new();
    let mut llm_results = Vec::<LlmResultRow>::new();
    let mut failures = Vec::<FailureRow>::new();

    for trajectory in &trajectories {
        let prompt = build_prompt(trajectory, args.max_refs);
        let call_started = Instant::now();
        let response = call_llm(
            &client,
            &endpoint,
            &model,
            provider,
            api_key.as_deref(),
            &prompt,
            args.max_tokens,
            args.temperature,
        )
        .await;
        let latency_ms = call_started.elapsed().as_millis();
        let (raw_response, prompt_tokens, completion_tokens) = match response {
            Ok(response) => response,
            Err(error) => {
                failures.push(FailureRow {
                    step_id: trajectory.step_id.clone(),
                    reason: "llm_call_failed".to_string(),
                    raw_response: None,
                    detail: json!({ "error": error.to_string() }),
                });
                continue;
            }
        };
        match parse_action(&raw_response)
            .and_then(|action| validate_action_for_trajectory(action, trajectory))
        {
            Ok(action) => {
                predictions.push(PredictionRow {
                    step_id: trajectory.step_id.clone(),
                    action: action.clone(),
                });
                llm_results.push(LlmResultRow {
                    step_id: trajectory.step_id.clone(),
                    action: Some(action),
                    valid_action: true,
                    bounded_action: true,
                    prompt_tokens,
                    completion_tokens,
                    latency_ms,
                    raw_response,
                });
            }
            Err(error) => {
                llm_results.push(LlmResultRow {
                    step_id: trajectory.step_id.clone(),
                    action: None,
                    valid_action: false,
                    bounded_action: false,
                    prompt_tokens,
                    completion_tokens,
                    latency_ms,
                    raw_response: raw_response.clone(),
                });
                failures.push(FailureRow {
                    step_id: trajectory.step_id.clone(),
                    reason: "invalid_llm_action".to_string(),
                    raw_response: Some(raw_response),
                    detail: json!({ "error": error.to_string() }),
                });
            }
        }
    }

    write_jsonl(
        &args.output.join("predictions.jsonl"),
        predictions.iter().map(serde_json::to_value),
    )?;
    write_jsonl(
        &args.output.join("llm_results.jsonl"),
        llm_results.iter().map(serde_json::to_value),
    )?;
    write_jsonl(
        &args.output.join("failures.jsonl"),
        failures.iter().map(serde_json::to_value),
    )?;
    let summary = summarize(SummaryInput {
        args: &args,
        endpoint,
        model,
        provider,
        trajectories: &trajectories,
        predictions: &predictions,
        llm_results: &llm_results,
        failures: &failures,
        elapsed_ms: started.elapsed().as_millis(),
    })?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut trajectories = None;
    let mut output = None;
    let mut endpoint = None;
    let mut model = None;
    let mut provider = None;
    let mut api_key_env = "LLM_API_KEY".to_string();
    let mut max_tokens = 350u32;
    let mut temperature = 0.0f64;
    let mut limit = None;
    let mut offset = 0usize;
    let mut max_refs = 32usize;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--trajectories" => {
                trajectories = Some(PathBuf::from(next_arg(&mut args, "--trajectories")?));
            }
            "--output" => output = Some(PathBuf::from(next_arg(&mut args, "--output")?)),
            "--endpoint" => endpoint = Some(next_arg(&mut args, "--endpoint")?),
            "--model" => model = Some(next_arg(&mut args, "--model")?),
            "--provider" => provider = Some(parse_provider(&next_arg(&mut args, "--provider")?)?),
            "--api-key-env" => api_key_env = next_arg(&mut args, "--api-key-env")?,
            "--max-tokens" => max_tokens = next_arg(&mut args, "--max-tokens")?.parse()?,
            "--temperature" => temperature = next_arg(&mut args, "--temperature")?.parse()?,
            "--limit" => limit = Some(next_arg(&mut args, "--limit")?.parse()?),
            "--offset" => offset = next_arg(&mut args, "--offset")?.parse()?,
            "--max-refs" => max_refs = next_arg(&mut args, "--max-refs")?.parse()?,
            "--force" => force = true,
            "--help" | "-h" => return Err(usage().into()),
            value if value.starts_with('-') => {
                return Err(format!("unknown argument: {value}\n{}", usage()).into());
            }
            value => {
                if trajectories.is_some() {
                    return Err(format!("unexpected positional argument: {value}").into());
                }
                trajectories = Some(PathBuf::from(value));
            }
        }
    }

    Ok(Args {
        trajectories: trajectories.ok_or_else(usage)?,
        output: output.ok_or("--output is required")?,
        endpoint,
        model,
        provider,
        api_key_env,
        max_tokens,
        temperature,
        limit,
        offset,
        max_refs,
        force,
    })
}

fn next_arg(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    args.next()
        .ok_or_else(|| format!("{name} requires a value").into())
}

fn usage() -> String {
    "usage: kernel_operator_llm_baseline --trajectories <trajectories.jsonl> --output <dir> --endpoint <url> --model <model> [--provider openai|openai-new|anthropic] [--api-key-env LLM_API_KEY] [--limit n] [--offset n] [--force]".to_string()
}

fn resolve_required(
    explicit: Option<&str>,
    env_name: &str,
    flag_name: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    explicit
        .map(ToString::to_string)
        .or_else(|| env::var(env_name).ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing {flag_name} or {env_name}").into())
}

fn ensure_output_dir(path: &Path, force: bool) -> Result<(), Box<dyn Error + Send + Sync>> {
    if path.exists() {
        if !force {
            return Err(format!(
                "output directory already exists: {}; pass --force to reuse it",
                path.display()
            )
            .into());
        }
    } else {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn read_trajectories(path: &Path) -> Result<Vec<TrajectoryInput>, Box<dyn Error + Send + Sync>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str(&line).map_err(|error| {
            format!(
                "failed to parse {} line {}: {error}",
                path.display(),
                index + 1
            )
        })?);
    }
    Ok(values)
}

fn select_trajectories(values: Vec<TrajectoryInput>, args: &Args) -> Vec<TrajectoryInput> {
    values
        .into_iter()
        .skip(args.offset)
        .take(args.limit.unwrap_or(usize::MAX))
        .collect()
}

fn build_prompt(trajectory: &TrajectoryInput, max_refs: usize) -> String {
    let visible_state = compact_visible_state(&trajectory.visible_state, max_refs);
    format!(
        r#"You operate KMP memory tools.

Your task is to output exactly one JSON object with an `action` field.
Do not explain. Do not include markdown. Do not invent refs, scopes, or hidden memory.

Allowed action shapes:

{{"action":{{"type":"tool_call","tool":"kmp_near","arguments":{{"about":"...","around":{{"ref":"..."}},"dimensions":{{"mode":"all","scope":"current_about"}},"include":{{"evidence":true,"raw_refs":false,"relations":true}},"limit":{{"entries":12,"tokens":2400}},"budget":{{"depth":3,"tokens":2400}},"window":{{"before_entries":6,"after_entries":0}}}}}}}}

{{"action":{{"type":"tool_call","tool":"kmp_inspect","arguments":{{"ref":"...","include":{{"details":true,"incoming":true,"outgoing":true,"raw":false}}}}}}}}

{{"action":{{"type":"tool_call","tool":"kmp_trace","arguments":{{"from":"...","to":"...","goal":"Kernel operator trace probe","budget":{{"depth":1,"tokens":1600}}}}}}}}

{{"action":{{"type":"stop","answer_policy":"evidence_or_unknown","final_refs":["..."],"reason":"sufficient_evidence"}}}}

Policy:
- If there is no `last_tool`, call `kmp_near` around `current_ref`.
- If the last tool was `kmp_near`, call `kmp_inspect` on `current_ref`.
- If the last tool was `kmp_inspect` and `trace_target_ref` is present, call `kmp_trace` from `current_ref` to `trace_target_ref`.
- Otherwise stop.
- Every tool call must be bounded.
- For `kmp_near`, `arguments.about` must equal the top-level `about` value exactly.
- Do not use `current_ref` as `arguments.about`.
- `kmp_inspect.include.raw` must be false.
- Use only tools present in `allowed_tools`.
- Use only refs visible in `current_ref`, `trace_target_ref`, `known_refs`, or `last_observed_refs`.

Trajectory metadata:
task_family: {task_family}
mode: {mode}
about: {about}
allowed_tools: {allowed_tools}

Visible state:
{visible_state}
"#,
        task_family = trajectory.task_family,
        mode = trajectory.mode,
        about = trajectory.about,
        allowed_tools = serde_json::to_string(&trajectory.allowed_tools).unwrap_or_default(),
        visible_state = serde_json::to_string_pretty(&visible_state).unwrap_or_default(),
    )
}

fn compact_visible_state(value: &Value, max_refs: usize) -> Value {
    let mut compact = value.clone();
    truncate_array(&mut compact, "known_refs", max_refs);
    truncate_array(&mut compact, "last_observed_refs", max_refs);
    compact
}

fn truncate_array(value: &mut Value, key: &str, max_items: usize) {
    let Some(items) = value.get_mut(key).and_then(Value::as_array_mut) else {
        return;
    };
    if items.len() > max_items {
        items.truncate(max_items);
    }
}

fn parse_action(raw_response: &str) -> Result<Value, Box<dyn Error + Send + Sync>> {
    let normalized = normalize_llm_json_response(raw_response);
    let value: Value = serde_json::from_str(&normalized)?;
    let action = value
        .get("action")
        .or_else(|| value.get("target_action"))
        .cloned()
        .unwrap_or(value);
    validate_action_shape(&action)?;
    Ok(action)
}

fn validate_action_shape(action: &Value) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(error) = kernel_operator_action_shape_error(action) {
        return Err(error.into());
    }
    match action.get("type").and_then(Value::as_str) {
        Some("stop") => Ok(()),
        Some("tool_call") => {
            let tool = action
                .get("tool")
                .and_then(Value::as_str)
                .ok_or("tool_call action requires tool")?;
            let arguments = action
                .get("arguments")
                .ok_or("tool_call action requires arguments")?;
            if !kernel_operator_is_bounded_tool_call(tool, arguments) {
                return Err(format!("unbounded or invalid tool call for `{tool}`").into());
            }
            if kernel_operator_primary_refs(action).is_empty() && tool != "kmp_ask" {
                return Err(format!("tool call `{tool}` has no primary refs").into());
            }
            Ok(())
        }
        Some(other) => Err(format!("unsupported action type `{other}`").into()),
        None => Err("action requires type".into()),
    }
}

fn validate_action_for_trajectory(
    action: Value,
    trajectory: &TrajectoryInput,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    validate_action_shape(&action)?;
    if action.get("type").and_then(Value::as_str) == Some("tool_call") {
        let tool = action
            .get("tool")
            .and_then(Value::as_str)
            .ok_or("tool_call action requires tool")?;
        if !trajectory
            .allowed_tools
            .iter()
            .any(|allowed| allowed == tool)
        {
            return Err(format!("tool `{tool}` is not allowed for this trajectory").into());
        }
        if let Some(about) = action
            .get("arguments")
            .and_then(|arguments| arguments.get("about"))
            .and_then(Value::as_str)
            && about != trajectory.about
        {
            return Err(format!(
                "tool `{tool}` used about `{about}`, expected `{}`",
                trajectory.about
            )
            .into());
        }
    }
    validate_refs_are_visible(&action, &trajectory.visible_state)?;
    Ok(action)
}

fn validate_refs_are_visible(
    action: &Value,
    visible_state: &Value,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let visible_refs = visible_refs(visible_state);
    let primary_refs = kernel_operator_primary_refs(action);
    for ref_id in primary_refs {
        if !visible_refs.contains(&ref_id) {
            return Err(format!("action references non-visible ref `{ref_id}`").into());
        }
    }
    if action.get("type").and_then(Value::as_str) == Some("stop") {
        for ref_id in stop_final_refs(action) {
            if !visible_refs.contains(&ref_id) {
                return Err(format!("stop action references non-visible ref `{ref_id}`").into());
            }
        }
    }
    Ok(())
}

fn visible_refs(visible_state: &Value) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    for key in ["current_ref", "trace_target_ref"] {
        if let Some(ref_id) = visible_state.get(key).and_then(Value::as_str) {
            refs.insert(ref_id.to_string());
        }
    }
    for key in ["known_refs", "last_observed_refs"] {
        if let Some(values) = visible_state.get(key).and_then(Value::as_array) {
            for value in values {
                if let Some(ref_id) = value.as_str() {
                    refs.insert(ref_id.to_string());
                }
            }
        }
    }
    refs
}

fn stop_final_refs(action: &Value) -> Vec<String> {
    action
        .get("final_refs")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn summarize(input: SummaryInput<'_>) -> Result<BaselineSummary, Box<dyn Error + Send + Sync>> {
    let mut by_action = BTreeMap::<String, usize>::new();
    let mut invalid_actions = 0usize;
    let mut unbounded_actions = 0usize;
    let mut prompt_tokens = 0u32;
    let mut completion_tokens = 0u32;
    for result in input.llm_results {
        prompt_tokens = prompt_tokens.saturating_add(result.prompt_tokens);
        completion_tokens = completion_tokens.saturating_add(result.completion_tokens);
    }
    for prediction in input.predictions {
        if !valid_action_shape(&prediction.action) {
            invalid_actions += 1;
        }
        if unbounded_action(&prediction.action) {
            unbounded_actions += 1;
        }
        *by_action
            .entry(action_label(&prediction.action))
            .or_default() += 1;
    }
    Ok(BaselineSummary {
        baseline: BASELINE,
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        trajectories: input.args.trajectories.display().to_string(),
        output: input.args.output.display().to_string(),
        endpoint: input.endpoint,
        model: input.model,
        provider: provider_label(input.provider),
        total_selected: input.trajectories.len(),
        predictions: input.predictions.len(),
        llm_results: input.llm_results.len(),
        failures: input.failures.len(),
        invalid_actions,
        unbounded_actions,
        prompt_tokens,
        completion_tokens,
        elapsed_ms: input.elapsed_ms,
        by_action,
    })
}

fn valid_action_shape(action: &Value) -> bool {
    kernel_operator_is_valid_action_shape(action)
}

fn unbounded_action(action: &Value) -> bool {
    if action.get("type").and_then(Value::as_str) != Some("tool_call") {
        return false;
    }
    let Some(tool) = action.get("tool").and_then(Value::as_str) else {
        return true;
    };
    let arguments = action.get("arguments").unwrap_or(&Value::Null);
    !kernel_operator_is_bounded_tool_call(tool, arguments)
}

fn action_label(action: &Value) -> String {
    match action.get("type").and_then(Value::as_str) {
        Some("tool_call") => format!(
            "tool_call:{}",
            action
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ),
        Some(kind) => kind.to_string(),
        None => "invalid".to_string(),
    }
}

fn provider_label(provider: LlmProvider) -> &'static str {
    match provider {
        LlmProvider::OpenAI => "openai",
        LlmProvider::OpenAINew => "openai-new",
        LlmProvider::Anthropic => "anthropic",
    }
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

fn write_jsonl<T: Serialize>(
    path: &Path,
    values: impl Iterator<Item = Result<T, serde_json::Error>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    for value in values {
        serde_json::to_writer(&mut writer, &value?)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_does_not_include_target_action() {
        let trajectory = TrajectoryInput {
            step_id: "s1".to_string(),
            about: "about:1".to_string(),
            task_family: "memoryarena.progressive_search".to_string(),
            mode: "read".to_string(),
            visible_state: json!({
                "current_ref": "node:1",
                "last_tool": null,
                "known_refs": [],
            }),
            allowed_tools: vec!["kmp_near".to_string()],
        };
        let prompt = build_prompt(&trajectory, 32);
        assert!(!prompt.contains("target_action"));
        assert!(prompt.contains("kmp_near"));
    }

    #[test]
    fn parse_action_accepts_wrapped_tool_call() -> Result<(), Box<dyn Error + Send + Sync>> {
        let action = parse_action(
            r#"{"action":{"type":"tool_call","tool":"kmp_inspect","arguments":{"ref":"node:1","include":{"details":true,"incoming":true,"outgoing":true,"raw":false}}}}"#,
        )?;
        assert_eq!(
            action.get("tool").and_then(Value::as_str),
            Some("kmp_inspect")
        );
        Ok(())
    }

    #[test]
    fn parse_action_rejects_raw_inspect() {
        let error = parse_action(
            r#"{"action":{"type":"tool_call","tool":"kmp_inspect","arguments":{"ref":"node:1","include":{"details":true,"incoming":true,"outgoing":true,"raw":true}}}}"#,
        )
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
        assert!(error.contains("raw") || error.contains("unbounded"));
    }

    #[test]
    fn trajectory_validation_rejects_disallowed_tools() {
        let trajectory = test_trajectory(json!({
            "current_ref": "node:1",
            "known_refs": ["node:1"],
            "last_observed_refs": [],
            "trace_target_ref": null,
        }));
        let error = validate_action_for_trajectory(
            json!({
                "type": "tool_call",
                "tool": "kmp_ask",
                "arguments": {
                    "about": "about:1",
                    "answer_policy": "evidence_or_unknown",
                    "dimensions": { "mode": "all", "scope": "current_about" },
                    "question": "what now?",
                    "budget": { "tokens": 800 }
                }
            }),
            &trajectory,
        )
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
        assert!(error.contains("not allowed"));
    }

    #[test]
    fn trajectory_validation_rejects_non_visible_refs() {
        let trajectory = test_trajectory(json!({
            "current_ref": "node:1",
            "known_refs": ["node:1"],
            "last_observed_refs": [],
            "trace_target_ref": null,
        }));
        let error = validate_action_for_trajectory(
            json!({
                "type": "tool_call",
                "tool": "kmp_inspect",
                "arguments": {
                    "ref": "node:2",
                    "include": {
                        "details": true,
                        "incoming": true,
                        "outgoing": true,
                        "raw": false
                    }
                }
            }),
            &trajectory,
        )
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
        assert!(error.contains("non-visible ref"));
    }

    #[test]
    fn compact_visible_state_truncates_large_ref_lists() {
        let compact = compact_visible_state(
            &json!({
                "known_refs": ["a", "b", "c"],
                "last_observed_refs": ["d", "e", "f"],
            }),
            2,
        );
        assert_eq!(
            compact
                .get("known_refs")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            compact
                .get("last_observed_refs")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
    }

    fn test_trajectory(visible_state: Value) -> TrajectoryInput {
        TrajectoryInput {
            step_id: "s1".to_string(),
            about: "about:1".to_string(),
            task_family: "memoryarena.progressive_search".to_string(),
            mode: "read".to_string(),
            visible_state,
            allowed_tools: vec!["kmp_near".to_string(), "kmp_inspect".to_string()],
        }
    }
}
