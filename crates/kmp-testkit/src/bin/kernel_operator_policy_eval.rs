use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Value, json};

use kmp_testkit::{kernel_operator_action_contract_error, kernel_operator_primary_refs};

const EVALUATOR: &str = "kernel-operator-policy-eval-v1";
const ACTION_VALIDATOR: &str = "kernel-operator-action-contract-v1";
const SCHEMA_MODE: &str = "strict-no-additional-properties";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    trajectories: PathBuf,
    predictions: Option<PathBuf>,
    baseline: Baseline,
    output: Option<PathBuf>,
    details_output: Option<PathBuf>,
    limit: Option<usize>,
    offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Baseline {
    Deterministic,
    Oracle,
}

#[derive(Debug, Clone)]
struct Trajectory {
    step_id: String,
    about: String,
    mode: String,
    task_family: String,
    visible_state: Value,
    target_action: Value,
}

#[derive(Debug, Serialize)]
struct EvalSummary {
    evaluator: &'static str,
    action_validator: &'static str,
    schema_mode: &'static str,
    generated_at_unix_seconds: u64,
    trajectories: String,
    predictions: Option<String>,
    predictor: String,
    total: usize,
    by_mode: BTreeMap<String, usize>,
    by_mode_eval: BTreeMap<String, EvalBreakdown>,
    by_task_family: BTreeMap<String, usize>,
    target_actions: BTreeMap<String, usize>,
    predicted_actions: BTreeMap<String, usize>,
    invalid_prediction_reasons: BTreeMap<String, usize>,
    counts: EvalCounts,
    rates: EvalRates,
}

#[derive(Debug, Serialize)]
struct EvalResult {
    summary: EvalSummary,
    details: Vec<EvalDetail>,
}

#[derive(Debug, Serialize)]
struct EvalDetail {
    step_id: String,
    mode: String,
    task_family: String,
    target_capability_key: String,
    target_action_label: String,
    predicted_action_label: Option<String>,
    prediction_status: &'static str,
    invalid_reason: Option<String>,
    score: ActionScore,
}

#[derive(Debug, Serialize)]
struct EvalBreakdown {
    total: usize,
    counts: EvalCounts,
    rates: EvalRates,
}

#[derive(Debug, Default, Serialize)]
struct ActionScore {
    action_type_correct: bool,
    tool_correct: bool,
    primary_refs_correct: bool,
    scope_correct: bool,
    cursor_mode_correct: bool,
    window_shape_correct: bool,
    limit_policy_correct: bool,
    continue_page_correct: bool,
    stop_correct: bool,
    exact_action_correct: bool,
}

#[derive(Debug, Default, Serialize)]
struct EvalCounts {
    target_tool_calls: usize,
    target_stop_actions: usize,
    target_cursor_actions: usize,
    target_window_actions: usize,
    target_limit_actions: usize,
    target_page_continuations: usize,
    missing_predictions: usize,
    invalid_predictions: usize,
    unbounded_tool_calls: usize,
    action_type_correct: usize,
    tool_correct: usize,
    primary_refs_correct: usize,
    scope_correct: usize,
    cursor_mode_correct: usize,
    window_shape_correct: usize,
    limit_policy_correct: usize,
    continue_page_correct: usize,
    stop_correct: usize,
    exact_action_correct: usize,
}

#[derive(Debug, Default, Serialize)]
struct EvalRates {
    action_type_accuracy: f64,
    tool_accuracy: f64,
    primary_ref_accuracy: f64,
    scope_accuracy: f64,
    cursor_mode_accuracy: f64,
    window_shape_accuracy: f64,
    limit_policy_accuracy: f64,
    continue_page_accuracy: f64,
    stop_accuracy: f64,
    exact_action_accuracy: f64,
    invalid_prediction_rate: f64,
    unbounded_tool_call_rate: f64,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    let trajectories = select_trajectories(read_trajectories(&args.trajectories)?, &args);
    let predictions = match args.predictions.as_deref() {
        Some(path) => Some(read_predictions(path)?),
        None => None,
    };
    let result = evaluate_internal(&args, &trajectories, predictions.as_ref())?;
    let rendered = serde_json::to_string_pretty(&result.summary)?;
    if let Some(output) = args.output.as_deref() {
        let file = File::create(output)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(rendered.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }
    if let Some(output) = args.details_output.as_deref() {
        let file = File::create(output)?;
        let mut writer = BufWriter::new(file);
        for detail in &result.details {
            serde_json::to_writer(&mut writer, detail)?;
            writer.write_all(b"\n")?;
        }
        writer.flush()?;
    }
    println!("{rendered}");
    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut trajectories = None;
    let mut predictions = None;
    let mut baseline = Baseline::Deterministic;
    let mut output = None;
    let mut details_output = None;
    let mut limit = None;
    let mut offset = 0usize;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--trajectories" => {
                trajectories = Some(PathBuf::from(next_arg(&mut args, "--trajectories")?));
            }
            "--predictions" => {
                predictions = Some(PathBuf::from(next_arg(&mut args, "--predictions")?));
            }
            "--baseline" => {
                baseline = parse_baseline(&next_arg(&mut args, "--baseline")?)?;
            }
            "--output" => output = Some(PathBuf::from(next_arg(&mut args, "--output")?)),
            "--details-output" => {
                details_output = Some(PathBuf::from(next_arg(&mut args, "--details-output")?));
            }
            "--limit" => limit = Some(next_arg(&mut args, "--limit")?.parse()?),
            "--offset" => offset = next_arg(&mut args, "--offset")?.parse()?,
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
        predictions,
        baseline,
        output,
        details_output,
        limit,
        offset,
    })
}

fn parse_baseline(value: &str) -> Result<Baseline, Box<dyn Error + Send + Sync>> {
    match value {
        "deterministic" => Ok(Baseline::Deterministic),
        "oracle" => Ok(Baseline::Oracle),
        other => Err(format!("unknown baseline `{other}`; expected deterministic|oracle").into()),
    }
}

fn next_arg(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    args.next()
        .ok_or_else(|| format!("{name} requires a value").into())
}

fn usage() -> String {
    "usage: kernel_operator_policy_eval --trajectories <trajectories.jsonl> [--predictions predictions.jsonl] [--baseline deterministic|oracle] [--output summary.json] [--details-output details.jsonl] [--limit n] [--offset n]".to_string()
}

fn read_trajectories(path: &Path) -> Result<Vec<Trajectory>, Box<dyn Error + Send + Sync>> {
    let mut seen_step_ids = BTreeSet::<String>::new();
    read_jsonl(path)?
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let location = format!("{}:{}", path.display(), index + 1);
            let step_id = required_string(&value, "step_id", &location)?.to_string();
            if !seen_step_ids.insert(step_id.clone()) {
                return Err(format!(
                    "{location} duplicate step_id `{step_id}`; policy evaluation requires unique trajectory step ids"
                )
                .into());
            }
            Ok(Trajectory {
                step_id,
                about: required_string(&value, "about", &location)?.to_string(),
                mode: required_string(&value, "mode", &location)?.to_string(),
                task_family: required_string(&value, "task_family", &location)?.to_string(),
                visible_state: value
                    .get("visible_state")
                    .cloned()
                    .ok_or_else(|| format!("{location} missing required field `visible_state`"))?,
                target_action: value
                    .get("target_action")
                    .cloned()
                    .ok_or_else(|| format!("{location} missing required field `target_action`"))?,
            })
        })
        .collect()
}

fn select_trajectories(values: Vec<Trajectory>, args: &Args) -> Vec<Trajectory> {
    values
        .into_iter()
        .skip(args.offset)
        .take(args.limit.unwrap_or(usize::MAX))
        .collect()
}

fn read_predictions(path: &Path) -> Result<BTreeMap<String, Value>, Box<dyn Error + Send + Sync>> {
    let mut predictions = BTreeMap::new();
    for (index, value) in read_jsonl(path)?.into_iter().enumerate() {
        let location = format!("{}:{}", path.display(), index + 1);
        let step_id = required_string(&value, "step_id", &location)?;
        let action = value
            .get("action")
            .or_else(|| value.get("target_action"))
            .cloned()
            .ok_or_else(|| format!("{location} missing `action` or `target_action`"))?;
        if predictions.insert(step_id.to_string(), action).is_some() {
            return Err(format!(
                "{location} duplicate prediction step_id `{step_id}`; policy evaluation requires unique prediction step ids"
            )
            .into());
        }
    }
    Ok(predictions)
}

fn read_jsonl(path: &Path) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
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

fn required_string<'a>(
    value: &'a Value,
    field: &str,
    location: &str,
) -> Result<&'a str, Box<dyn Error + Send + Sync>> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{location} missing required string field `{field}`").into())
}

#[cfg(test)]
fn evaluate(
    args: &Args,
    trajectories: &[Trajectory],
    predictions: Option<&BTreeMap<String, Value>>,
) -> Result<EvalSummary, Box<dyn Error + Send + Sync>> {
    Ok(evaluate_internal(args, trajectories, predictions)?.summary)
}

fn evaluate_internal(
    args: &Args,
    trajectories: &[Trajectory],
    predictions: Option<&BTreeMap<String, Value>>,
) -> Result<EvalResult, Box<dyn Error + Send + Sync>> {
    let mut counts = EvalCounts::default();
    let mut by_mode = BTreeMap::<String, usize>::new();
    let mut by_mode_counts = BTreeMap::<String, EvalCounts>::new();
    let mut by_task_family = BTreeMap::<String, usize>::new();
    let mut target_actions = BTreeMap::<String, usize>::new();
    let mut predicted_actions = BTreeMap::<String, usize>::new();
    let mut invalid_prediction_reasons = BTreeMap::<String, usize>::new();
    let mut details = Vec::<EvalDetail>::new();

    for trajectory in trajectories {
        let mode_key = trajectory.mode.clone();
        *by_mode.entry(mode_key.clone()).or_default() += 1;
        *by_task_family
            .entry(trajectory.task_family.clone())
            .or_default() += 1;
        *target_actions
            .entry(action_label(&trajectory.target_action))
            .or_default() += 1;
        count_target_metrics(&trajectory.target_action, &mut counts);
        count_target_metrics(
            &trajectory.target_action,
            by_mode_counts.entry(mode_key.clone()).or_default(),
        );

        let predicted = predicted_action(args.baseline, trajectory, predictions);
        let Some(predicted) = predicted else {
            counts.missing_predictions += 1;
            by_mode_counts
                .entry(mode_key)
                .or_default()
                .missing_predictions += 1;
            details.push(eval_detail(
                trajectory,
                None,
                "missing",
                None,
                ActionScore::default(),
            ));
            continue;
        };
        let predicted_action_label = action_label(&predicted);
        *predicted_actions.entry(predicted_action_label).or_default() += 1;
        if let Some(error) = kernel_operator_action_contract_error(&predicted) {
            counts.invalid_predictions += 1;
            by_mode_counts
                .entry(mode_key)
                .or_default()
                .invalid_predictions += 1;
            if is_unbounded_contract_error(&error) {
                counts.unbounded_tool_calls += 1;
                by_mode_counts
                    .entry(trajectory.mode.clone())
                    .or_default()
                    .unbounded_tool_calls += 1;
            }
            *invalid_prediction_reasons.entry(error).or_default() += 1;
            let error = kernel_operator_action_contract_error(&predicted);
            details.push(eval_detail(
                trajectory,
                Some(&predicted),
                "invalid",
                error,
                ActionScore::default(),
            ));
            continue;
        }
        let score = action_score(&trajectory.target_action, &predicted);
        apply_score(&trajectory.target_action, &predicted, &score, &mut counts);
        apply_score(
            &trajectory.target_action,
            &predicted,
            &score,
            by_mode_counts.entry(mode_key).or_default(),
        );
        details.push(eval_detail(
            trajectory,
            Some(&predicted),
            "valid",
            None,
            score,
        ));
    }

    let summary_rates = rates(&counts, trajectories.len());
    let by_mode_eval = by_mode_counts
        .into_iter()
        .map(|(mode, counts)| {
            let total = by_mode.get(&mode).copied().unwrap_or_default();
            let rates = rates(&counts, total);
            (
                mode,
                EvalBreakdown {
                    total,
                    counts,
                    rates,
                },
            )
        })
        .collect();
    Ok(EvalResult {
        summary: EvalSummary {
            evaluator: EVALUATOR,
            action_validator: ACTION_VALIDATOR,
            schema_mode: SCHEMA_MODE,
            generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            trajectories: args.trajectories.display().to_string(),
            predictions: args
                .predictions
                .as_ref()
                .map(|path| path.display().to_string()),
            predictor: match args.predictions.as_ref() {
                Some(_) => "predictions".to_string(),
                None => match args.baseline {
                    Baseline::Deterministic => "baseline:deterministic".to_string(),
                    Baseline::Oracle => "baseline:oracle".to_string(),
                },
            },
            total: trajectories.len(),
            by_mode,
            by_mode_eval,
            by_task_family,
            target_actions,
            predicted_actions,
            invalid_prediction_reasons,
            counts,
            rates: summary_rates,
        },
        details,
    })
}

fn eval_detail(
    trajectory: &Trajectory,
    predicted: Option<&Value>,
    prediction_status: &'static str,
    invalid_reason: Option<String>,
    score: ActionScore,
) -> EvalDetail {
    EvalDetail {
        step_id: trajectory.step_id.clone(),
        mode: trajectory.mode.clone(),
        task_family: trajectory.task_family.clone(),
        target_capability_key: target_capability_key(trajectory),
        target_action_label: action_label(&trajectory.target_action),
        predicted_action_label: predicted.map(action_label),
        prediction_status,
        invalid_reason,
        score,
    }
}

fn predicted_action(
    baseline: Baseline,
    trajectory: &Trajectory,
    predictions: Option<&BTreeMap<String, Value>>,
) -> Option<Value> {
    if let Some(predictions) = predictions {
        return predictions.get(&trajectory.step_id).cloned();
    }
    match baseline {
        Baseline::Oracle => Some(trajectory.target_action.clone()),
        Baseline::Deterministic => Some(deterministic_action(trajectory)),
    }
}

fn deterministic_action(trajectory: &Trajectory) -> Value {
    let current_ref = trajectory
        .visible_state
        .get("current_ref")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let last_tool = trajectory
        .visible_state
        .get("last_tool")
        .and_then(Value::as_str);
    let trace_target_ref = trajectory
        .visible_state
        .get("trace_target_ref")
        .and_then(Value::as_str);

    match last_tool {
        None => json!({
            "type": "tool_call",
            "tool": "kernel_near",
            "arguments": {
                "about": trajectory.about,
                "around": { "ref": current_ref },
                "dimensions": { "mode": "all", "scope": "current_about" },
                "include": { "evidence": true, "raw_refs": false, "relations": true },
                "limit": { "entries": 12, "tokens": 2400 },
                "budget": { "depth": 3, "tokens": 2400 },
                "window": { "before_entries": 6, "after_entries": 0 }
            }
        }),
        Some("kernel_near") => json!({
            "type": "tool_call",
            "tool": "kernel_inspect",
            "arguments": {
                "ref": current_ref,
                "include": {
                    "details": true,
                    "incoming": true,
                    "outgoing": true,
                    "raw": false
                }
            }
        }),
        Some("kernel_inspect") if trace_target_ref.is_some() => json!({
            "type": "tool_call",
            "tool": "kernel_trace",
            "arguments": {
                "from": current_ref,
                "to": trace_target_ref,
                "goal": "Kernel operator deterministic trace probe",
                "budget": { "depth": 1, "tokens": 1600 }
            }
        }),
        Some("kernel_trace")
        | Some("kernel_inspect")
        | Some("kernel_goto")
        | Some("kernel_rewind")
        | Some("kernel_forward")
        | Some("kernel_ask")
        | Some(_) => {
            json!({
                "type": "stop",
                "answer_policy": "evidence_or_unknown",
                "final_refs": trajectory.visible_state.get("known_refs").cloned().unwrap_or_else(|| json!([])),
                "reason": "deterministic_baseline_stop"
            })
        }
    }
}

fn action_score(target: &Value, predicted: &Value) -> ActionScore {
    let mut score = ActionScore::default();
    let target_type = action_type(target);
    let predicted_type = action_type(predicted);
    if target_type == predicted_type {
        score.action_type_correct = true;
    }
    if target_type == Some("stop") && predicted_type == Some("stop") {
        score.stop_correct = true;
    }
    if target == predicted {
        score.exact_action_correct = true;
    }
    if target_type == Some("tool_call") && predicted_type == Some("tool_call") {
        if tool(target) == tool(predicted) {
            score.tool_correct = true;
        }
        if kernel_operator_primary_refs(target) == kernel_operator_primary_refs(predicted) {
            score.primary_refs_correct = true;
        }
        if scope_about(target) == scope_about(predicted) {
            score.scope_correct = true;
        }
        if target_cursor_mode(target).is_some()
            && target_cursor_mode(target) == target_cursor_mode(predicted)
        {
            score.cursor_mode_correct = true;
        }
        if action_window(target).is_some() && action_window(target) == action_window(predicted) {
            score.window_shape_correct = true;
        }
        if action_limit(target).is_some() && action_limit(target) == action_limit(predicted) {
            score.limit_policy_correct = true;
        }
        if trace_page_cursor(target).is_some()
            && trace_page_cursor(target) == trace_page_cursor(predicted)
        {
            score.continue_page_correct = true;
        }
    }
    score
}

fn apply_score(target: &Value, _predicted: &Value, score: &ActionScore, counts: &mut EvalCounts) {
    if score.action_type_correct {
        counts.action_type_correct += 1;
    }
    if score.stop_correct {
        counts.stop_correct += 1;
    }
    if score.exact_action_correct {
        counts.exact_action_correct += 1;
    }
    if action_type(target) == Some("tool_call") {
        if score.tool_correct {
            counts.tool_correct += 1;
        }
        if score.primary_refs_correct {
            counts.primary_refs_correct += 1;
        }
        if score.scope_correct {
            counts.scope_correct += 1;
        }
        if score.cursor_mode_correct {
            counts.cursor_mode_correct += 1;
        }
        if score.window_shape_correct {
            counts.window_shape_correct += 1;
        }
        if score.limit_policy_correct {
            counts.limit_policy_correct += 1;
        }
        if score.continue_page_correct {
            counts.continue_page_correct += 1;
        }
    }
}

fn count_target_metrics(target: &Value, counts: &mut EvalCounts) {
    match action_type(target) {
        Some("tool_call") => counts.target_tool_calls += 1,
        Some("stop") => counts.target_stop_actions += 1,
        _ => {}
    }
    count_target_navigation_metrics(target, counts);
}

fn count_target_navigation_metrics(target: &Value, counts: &mut EvalCounts) {
    if target_cursor_mode(target).is_some() {
        counts.target_cursor_actions += 1;
    }
    if action_window(target).is_some() {
        counts.target_window_actions += 1;
    }
    if action_limit(target).is_some() {
        counts.target_limit_actions += 1;
    }
    if trace_page_cursor(target).is_some() {
        counts.target_page_continuations += 1;
    }
}

fn rates(counts: &EvalCounts, total: usize) -> EvalRates {
    EvalRates {
        action_type_accuracy: ratio(counts.action_type_correct, total),
        tool_accuracy: ratio(counts.tool_correct, counts.target_tool_calls),
        primary_ref_accuracy: ratio(counts.primary_refs_correct, counts.target_tool_calls),
        scope_accuracy: ratio(counts.scope_correct, counts.target_tool_calls),
        cursor_mode_accuracy: ratio(counts.cursor_mode_correct, counts.target_cursor_actions),
        window_shape_accuracy: ratio(counts.window_shape_correct, counts.target_window_actions),
        limit_policy_accuracy: ratio(counts.limit_policy_correct, counts.target_limit_actions),
        continue_page_accuracy: ratio(
            counts.continue_page_correct,
            counts.target_page_continuations,
        ),
        stop_accuracy: ratio(counts.stop_correct, counts.target_stop_actions),
        exact_action_accuracy: ratio(counts.exact_action_correct, total),
        invalid_prediction_rate: ratio(counts.invalid_predictions, total),
        unbounded_tool_call_rate: ratio(counts.unbounded_tool_calls, total),
    }
}

fn ratio(count: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    count as f64 / total as f64
}

fn action_label(action: &Value) -> String {
    match action_type(action) {
        Some("tool_call") => format!("tool_call:{}", tool(action).unwrap_or("unknown")),
        Some(kind) => kind.to_string(),
        None => "invalid".to_string(),
    }
}

fn target_capability_key(trajectory: &Trajectory) -> String {
    let action = &trajectory.target_action;
    let label = action_label(action);
    let cursor = target_cursor_mode(action).unwrap_or("none");
    let dimension_mode = dimension_mode(action).unwrap_or("none");
    let dimension_scope = dimension_scope(action).unwrap_or("none");
    let trace_page = trace_page_mode(action);
    format!(
        "{}|{}|cursor:{cursor}|dim_mode:{dimension_mode}|dim_scope:{dimension_scope}|trace_page:{trace_page}",
        trajectory.task_family, label
    )
}

fn is_unbounded_contract_error(error: &str) -> bool {
    error.starts_with("unbounded or invalid tool call")
}

fn action_type(action: &Value) -> Option<&str> {
    action.get("type").and_then(Value::as_str)
}

fn tool(action: &Value) -> Option<&str> {
    action.get("tool").and_then(Value::as_str)
}

fn scope_about(action: &Value) -> Option<String> {
    action
        .get("arguments")
        .and_then(|arguments| arguments.get("about"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn target_cursor_mode(action: &Value) -> Option<&'static str> {
    let arguments = action.get("arguments")?;
    let cursor_key = match tool(action)? {
        "kernel_near" => "around",
        "kernel_goto" => "at",
        "kernel_rewind" | "kernel_forward" => "from",
        _ => return None,
    };
    let cursor = arguments.get(cursor_key)?.as_object()?;
    if cursor.contains_key("ref") {
        Some("ref")
    } else if cursor.contains_key("time") {
        Some("time")
    } else if cursor.contains_key("sequence") {
        Some("sequence")
    } else {
        None
    }
}

fn action_window(action: &Value) -> Option<&Value> {
    action.get("arguments")?.get("window")
}

fn action_limit(action: &Value) -> Option<&Value> {
    action.get("arguments")?.get("limit")
}

fn dimension_mode(action: &Value) -> Option<&str> {
    action
        .get("arguments")?
        .get("dimensions")?
        .get("mode")?
        .as_str()
}

fn dimension_scope(action: &Value) -> Option<&str> {
    action
        .get("arguments")?
        .get("dimensions")?
        .get("scope")?
        .as_str()
}

fn trace_page_mode(action: &Value) -> &'static str {
    if tool(action) != Some("kernel_trace") {
        return "none";
    }
    let Some(page) = action
        .get("arguments")
        .and_then(|arguments| arguments.get("page"))
    else {
        return "none";
    };
    if page.get("cursor").is_some() {
        "continue"
    } else {
        "first"
    }
}

fn trace_page_cursor(action: &Value) -> Option<&str> {
    if tool(action) != Some("kernel_trace") {
        return None;
    }
    action
        .get("arguments")?
        .get("page")?
        .get("cursor")?
        .as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn deterministic_policy_starts_with_near() -> Result<(), Box<dyn Error + Send + Sync>> {
        let trajectory = Trajectory {
            step_id: "s1".to_string(),
            about: "memoryarena:run:r1:task_type:progressive_search:task:1".to_string(),
            mode: "read".to_string(),
            task_family: "memoryarena.progressive_search".to_string(),
            visible_state: json!({
                "current_ref": "memoryarena:run:r1:task_type:progressive_search:task:1:subtask:1:question",
                "trace_target_ref": null,
                "last_tool": null,
                "known_refs": [],
            }),
            target_action: json!({ "type": "stop" }),
        };
        let action = deterministic_action(&trajectory);
        assert_eq!(action_type(&action), Some("tool_call"));
        assert_eq!(tool(&action), Some("kernel_near"));
        let refs = kernel_operator_primary_refs(&action);
        if refs != ["memoryarena:run:r1:task_type:progressive_search:task:1:subtask:1:question"] {
            return Err(format!("unexpected refs: {refs:?}").into());
        }
        Ok(())
    }

    #[test]
    fn oracle_baseline_scores_exact_actions() -> Result<(), Box<dyn Error + Send + Sync>> {
        let trajectories = vec![Trajectory {
            step_id: "s1".to_string(),
            about: "memoryarena:run:r1:task_type:progressive_search:task:1".to_string(),
            mode: "read".to_string(),
            task_family: "memoryarena.progressive_search".to_string(),
            visible_state: json!({
                "current_ref": "node:1",
                "last_tool": null,
            }),
            target_action: json!({
                "type": "tool_call",
                "tool": "kernel_inspect",
                "arguments": {
                    "ref": "node:1",
                    "include": {
                        "details": true,
                        "incoming": true,
                        "outgoing": true,
                        "raw": false
                    }
                }
            }),
        }];
        let args = Args {
            trajectories: PathBuf::from("trajectories.jsonl"),
            predictions: None,
            baseline: Baseline::Oracle,
            output: None,
            details_output: None,
            limit: None,
            offset: 0,
        };
        let summary = evaluate(&args, &trajectories, None)?;
        assert_eq!(summary.counts.exact_action_correct, 1);
        assert_eq!(summary.counts.tool_correct, 1);
        Ok(())
    }

    #[test]
    fn summary_reports_mode_specific_gate_metrics() -> Result<(), Box<dyn Error + Send + Sync>> {
        let read_action = json!({
            "type": "tool_call",
            "tool": "kernel_inspect",
            "arguments": {
                "ref": "node:read",
                "include": {
                    "details": true,
                    "incoming": true,
                    "outgoing": true,
                    "raw": false
                }
            }
        });
        let writer_target = json!({
            "type": "tool_call",
            "tool": "kernel_near",
            "arguments": {
                "about": "about:1",
                "around": { "ref": "node:writer" },
                "dimensions": { "mode": "all", "scope": "current_about" },
                "include": { "evidence": true, "raw_refs": false, "relations": true },
                "limit": { "entries": 12, "tokens": 2400 },
                "budget": { "depth": 3, "tokens": 2400 },
                "window": { "before_entries": 6, "after_entries": 0 }
            }
        });
        let writer_prediction = json!({
            "type": "tool_call",
            "tool": "kernel_trace",
            "arguments": {
                "from": "node:writer",
                "to": "node:prior",
                "budget": { "depth": 2, "tokens": 1600 }
            }
        });
        let trajectories = vec![
            Trajectory {
                step_id: "read".to_string(),
                about: "about:1".to_string(),
                mode: "read".to_string(),
                task_family: "memoryarena.progressive_search".to_string(),
                visible_state: json!({ "current_ref": "node:read" }),
                target_action: read_action.clone(),
            },
            Trajectory {
                step_id: "writer".to_string(),
                about: "about:1".to_string(),
                mode: "write_context_read".to_string(),
                task_family: "memoryarena.smart_writer".to_string(),
                visible_state: json!({ "current_ref": "node:writer" }),
                target_action: writer_target,
            },
        ];
        let predictions = BTreeMap::from([
            ("read".to_string(), read_action),
            ("writer".to_string(), writer_prediction),
        ]);
        let args = Args {
            trajectories: PathBuf::from("trajectories.jsonl"),
            predictions: Some(PathBuf::from("predictions.jsonl")),
            baseline: Baseline::Deterministic,
            output: None,
            details_output: None,
            limit: None,
            offset: 0,
        };

        let summary = evaluate(&args, &trajectories, Some(&predictions))?;

        assert_eq!(summary.counts.exact_action_correct, 1);
        assert_eq!(summary.rates.exact_action_accuracy, 0.5);

        let read = summary.by_mode_eval.get("read").expect("read mode summary");
        assert_eq!(read.total, 1);
        assert_eq!(read.counts.exact_action_correct, 1);
        assert_eq!(read.rates.exact_action_accuracy, 1.0);

        let writer = summary
            .by_mode_eval
            .get("write_context_read")
            .expect("writer mode summary");
        assert_eq!(writer.total, 1);
        assert_eq!(writer.counts.exact_action_correct, 0);
        assert_eq!(writer.rates.exact_action_accuracy, 0.0);
        Ok(())
    }

    #[test]
    fn unbounded_prediction_is_counted() -> Result<(), Box<dyn Error + Send + Sync>> {
        let trajectories = vec![Trajectory {
            step_id: "s1".to_string(),
            about: "memoryarena:run:r1:task_type:progressive_search:task:1".to_string(),
            mode: "read".to_string(),
            task_family: "memoryarena.progressive_search".to_string(),
            visible_state: json!({
                "current_ref": "node:1",
                "last_tool": null,
            }),
            target_action: json!({
                "type": "tool_call",
                "tool": "kernel_near",
                "arguments": {
                    "about": "about:1",
                    "around": { "ref": "node:1" },
                    "dimensions": { "mode": "all", "scope": "current_about" },
                    "include": { "evidence": true, "raw_refs": false, "relations": true },
                    "limit": { "entries": 12, "tokens": 2400 },
                    "budget": { "depth": 3, "tokens": 2400 },
                    "window": { "before_entries": 6, "after_entries": 0 }
                }
            }),
        }];
        let mut predictions = BTreeMap::new();
        predictions.insert(
            "s1".to_string(),
            json!({
                "type": "tool_call",
                "tool": "kernel_near",
                "arguments": {
                    "about": "about:1",
                    "around": { "ref": "node:1" },
                    "dimensions": { "mode": "all", "scope": "current_about" },
                    "include": { "evidence": true, "raw_refs": false, "relations": true },
                    "limit": { "entries": 1000, "tokens": 2400 },
                    "budget": { "depth": 3, "tokens": 2400 },
                    "window": { "before_entries": 6, "after_entries": 0 }
                }
            }),
        );
        let args = Args {
            trajectories: PathBuf::from("trajectories.jsonl"),
            predictions: Some(PathBuf::from("predictions.jsonl")),
            baseline: Baseline::Deterministic,
            output: None,
            details_output: None,
            limit: None,
            offset: 0,
        };
        let summary = evaluate(&args, &trajectories, Some(&predictions))?;
        assert_eq!(summary.counts.invalid_predictions, 1);
        assert_eq!(summary.counts.unbounded_tool_calls, 1);
        assert_eq!(summary.counts.tool_correct, 0);
        assert_eq!(
            summary
                .invalid_prediction_reasons
                .get("unbounded or invalid tool call for `kernel_near`"),
            Some(&1)
        );
        Ok(())
    }

    #[test]
    fn invalid_prediction_reason_is_counted() -> Result<(), Box<dyn Error + Send + Sync>> {
        let trajectories = vec![Trajectory {
            step_id: "s1".to_string(),
            about: "about:1".to_string(),
            mode: "read".to_string(),
            task_family: "longmemeval.temporal-reasoning".to_string(),
            visible_state: json!({
                "current_ref": "node:1",
                "last_tool": null,
            }),
            target_action: json!({
                "type": "tool_call",
                "tool": "kernel_ask",
                "arguments": {
                    "about": "about:1",
                    "answer_policy": "evidence_or_unknown",
                    "dimensions": { "mode": "all", "scope": "current_about" },
                    "question": "What changed?"
                }
            }),
        }];
        let mut predictions = BTreeMap::new();
        predictions.insert(
            "s1".to_string(),
            json!({
                "type": "tool_call",
                "tool": "kernel_ask",
                "arguments": {
                    "about": "about:1",
                    "answer_policy": "evidence_or_unknown",
                    "dimensions": { "mode": "all", "scope": "current_about" },
                    "question": "What changed?",
                    "final_refs": ["node:1"]
                }
            }),
        );
        let args = Args {
            trajectories: PathBuf::from("trajectories.jsonl"),
            predictions: Some(PathBuf::from("predictions.jsonl")),
            baseline: Baseline::Deterministic,
            output: None,
            details_output: None,
            limit: None,
            offset: 0,
        };
        let summary = evaluate(&args, &trajectories, Some(&predictions))?;
        assert_eq!(summary.action_validator, ACTION_VALIDATOR);
        assert_eq!(summary.schema_mode, SCHEMA_MODE);
        assert_eq!(summary.counts.invalid_predictions, 1);
        assert_eq!(
            summary
                .invalid_prediction_reasons
                .get("action.arguments has unexpected field `final_refs`"),
            Some(&1)
        );
        Ok(())
    }

    #[test]
    fn navigation_policy_metrics_score_cursor_window_limit_and_page_continuation()
    -> Result<(), Box<dyn Error + Send + Sync>> {
        let near_action = json!({
            "type": "tool_call",
            "tool": "kernel_near",
            "arguments": {
                "about": "about:1",
                "around": { "sequence": 7 },
                "dimensions": { "mode": "all", "scope": "current_about" },
                "include": { "evidence": true, "raw_refs": false, "relations": true },
                "limit": { "entries": 12, "tokens": 2400 },
                "budget": { "depth": 3, "tokens": 2400 },
                "window": { "before_entries": 6, "after_entries": 0 }
            }
        });
        let trace_action = json!({
            "type": "tool_call",
            "tool": "kernel_trace",
            "arguments": {
                "from": "node:1",
                "to": "node:3",
                "budget": { "depth": 2, "tokens": 2400 },
                "page": { "entries": 16, "cursor": "page:next" }
            }
        });
        let trajectories = vec![
            Trajectory {
                step_id: "near".to_string(),
                about: "about:1".to_string(),
                mode: "read".to_string(),
                task_family: "conformance.temporal".to_string(),
                visible_state: json!({ "current_ref": "node:1" }),
                target_action: near_action.clone(),
            },
            Trajectory {
                step_id: "trace".to_string(),
                about: "about:1".to_string(),
                mode: "read".to_string(),
                task_family: "conformance.trace".to_string(),
                visible_state: json!({ "current_ref": "node:1" }),
                target_action: trace_action.clone(),
            },
        ];
        let predictions = BTreeMap::from([
            ("near".to_string(), near_action),
            ("trace".to_string(), trace_action),
        ]);
        let args = Args {
            trajectories: PathBuf::from("trajectories.jsonl"),
            predictions: Some(PathBuf::from("predictions.jsonl")),
            baseline: Baseline::Deterministic,
            output: None,
            details_output: None,
            limit: None,
            offset: 0,
        };

        let summary = evaluate(&args, &trajectories, Some(&predictions))?;

        assert_eq!(summary.counts.target_cursor_actions, 1);
        assert_eq!(summary.counts.cursor_mode_correct, 1);
        assert_eq!(summary.counts.target_window_actions, 1);
        assert_eq!(summary.counts.window_shape_correct, 1);
        assert_eq!(summary.counts.target_limit_actions, 1);
        assert_eq!(summary.counts.limit_policy_correct, 1);
        assert_eq!(summary.counts.target_page_continuations, 1);
        assert_eq!(summary.counts.continue_page_correct, 1);
        assert_eq!(summary.rates.cursor_mode_accuracy, 1.0);
        assert_eq!(summary.rates.window_shape_accuracy, 1.0);
        assert_eq!(summary.rates.limit_policy_accuracy, 1.0);
        assert_eq!(summary.rates.continue_page_accuracy, 1.0);
        Ok(())
    }

    #[test]
    fn navigation_policy_metrics_detect_mismatched_cursor_and_page()
    -> Result<(), Box<dyn Error + Send + Sync>> {
        let target_near = json!({
            "type": "tool_call",
            "tool": "kernel_near",
            "arguments": {
                "about": "about:1",
                "around": { "sequence": 7 },
                "dimensions": { "mode": "all", "scope": "current_about" },
                "include": { "evidence": true, "raw_refs": false, "relations": true },
                "limit": { "entries": 12, "tokens": 2400 },
                "budget": { "depth": 3, "tokens": 2400 },
                "window": { "before_entries": 6, "after_entries": 0 }
            }
        });
        let predicted_near = json!({
            "type": "tool_call",
            "tool": "kernel_near",
            "arguments": {
                "about": "about:1",
                "around": { "time": "2026-05-14T00:00:00Z" },
                "dimensions": { "mode": "all", "scope": "current_about" },
                "include": { "evidence": true, "raw_refs": false, "relations": true },
                "limit": { "entries": 24, "tokens": 2400 },
                "budget": { "depth": 3, "tokens": 2400 },
                "window": { "before_entries": 3, "after_entries": 3 }
            }
        });
        let target_trace = json!({
            "type": "tool_call",
            "tool": "kernel_trace",
            "arguments": {
                "from": "node:1",
                "to": "node:3",
                "budget": { "depth": 2, "tokens": 2400 },
                "page": { "entries": 16, "cursor": "page:next" }
            }
        });
        let predicted_trace = json!({
            "type": "tool_call",
            "tool": "kernel_trace",
            "arguments": {
                "from": "node:1",
                "to": "node:3",
                "budget": { "depth": 2, "tokens": 2400 },
                "page": { "entries": 16, "cursor": "page:other" }
            }
        });
        let trajectories = vec![
            Trajectory {
                step_id: "near".to_string(),
                about: "about:1".to_string(),
                mode: "read".to_string(),
                task_family: "conformance.temporal".to_string(),
                visible_state: json!({}),
                target_action: target_near,
            },
            Trajectory {
                step_id: "trace".to_string(),
                about: "about:1".to_string(),
                mode: "read".to_string(),
                task_family: "conformance.trace".to_string(),
                visible_state: json!({}),
                target_action: target_trace,
            },
        ];
        let predictions = BTreeMap::from([
            ("near".to_string(), predicted_near),
            ("trace".to_string(), predicted_trace),
        ]);
        let args = Args {
            trajectories: PathBuf::from("trajectories.jsonl"),
            predictions: Some(PathBuf::from("predictions.jsonl")),
            baseline: Baseline::Deterministic,
            output: None,
            details_output: None,
            limit: None,
            offset: 0,
        };

        let summary = evaluate(&args, &trajectories, Some(&predictions))?;

        assert_eq!(summary.counts.tool_correct, 2);
        assert_eq!(summary.counts.cursor_mode_correct, 0);
        assert_eq!(summary.counts.window_shape_correct, 0);
        assert_eq!(summary.counts.limit_policy_correct, 0);
        assert_eq!(summary.counts.continue_page_correct, 0);
        assert_eq!(summary.rates.cursor_mode_accuracy, 0.0);
        assert_eq!(summary.rates.window_shape_accuracy, 0.0);
        assert_eq!(summary.rates.limit_policy_accuracy, 0.0);
        assert_eq!(summary.rates.continue_page_accuracy, 0.0);
        Ok(())
    }

    #[test]
    fn read_predictions_rejects_duplicate_step_ids() -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = env::temp_dir().join(format!(
            "kernel-operator-policy-eval-duplicate-predictions-{}.jsonl",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"{"step_id":"s1","action":{"type":"stop"}}"#.to_string()
                + "\n"
                + r#"{"step_id":"s1","action":{"type":"stop"}}"#
                + "\n",
        )?;
        let result = read_predictions(&path);
        let _ = fs::remove_file(&path);
        let error = result.expect_err("duplicate prediction step ids should fail");
        assert!(error.to_string().contains("duplicate prediction step_id"));
        Ok(())
    }

    #[test]
    fn read_trajectories_rejects_duplicate_step_ids() -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = env::temp_dir().join(format!(
            "kernel-operator-policy-eval-duplicate-trajectories-{}.jsonl",
            std::process::id()
        ));
        let row = json!({
            "step_id": "s1",
            "about": "about:1",
            "mode": "read",
            "task_family": "test",
            "visible_state": {},
            "target_action": { "type": "stop" }
        });
        fs::write(&path, format!("{row}\n{row}\n"))?;
        let result = read_trajectories(&path);
        let _ = fs::remove_file(&path);
        let error = result.expect_err("duplicate trajectory step ids should fail");
        assert!(error.to_string().contains("duplicate step_id"));
        Ok(())
    }
}
