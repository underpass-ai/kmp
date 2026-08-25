use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use kmp_mcp::{KernelMcpGrpcTlsConfig, KernelMcpServer};
use kmp_testkit::kernel_operator_is_bounded_tool_call;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const REPLAYER: &str = "kernel-operator-mcp-replay-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    trajectories: PathBuf,
    predictions: PathBuf,
    output: PathBuf,
    endpoint: Option<String>,
    limit: Option<usize>,
    offset: usize,
    log_progress_every: Option<usize>,
    force: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Trajectory {
    step_id: String,
    about: String,
    mode: String,
    task_family: String,
    #[serde(default)]
    observed_outcome: Option<Value>,
}

#[derive(Debug, Clone)]
struct Prediction {
    action: Value,
}

#[derive(Debug, Serialize)]
struct ReplayRow {
    step_id: String,
    about: String,
    mode: String,
    task_family: String,
    action_label: String,
    elapsed_ms: u128,
    success: bool,
    error: Option<String>,
    partial_result: bool,
    page: Option<ReplayPage>,
    expected_observed_refs: Vec<String>,
    observed_refs: Vec<String>,
    missing_expected_refs: Vec<String>,
    extra_observed_refs: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ReplayPage {
    returned: Option<u64>,
    total: Option<u64>,
    has_more: bool,
    next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
struct ReplaySummary {
    replayer: &'static str,
    generated_at_unix_seconds: u64,
    endpoint: String,
    trajectories: String,
    predictions: String,
    output: String,
    selected: usize,
    tool_calls: usize,
    stop_actions: usize,
    executed_tool_calls: usize,
    successful_tool_calls: usize,
    failed_tool_calls: usize,
    missing_predictions: usize,
    invalid_predictions: usize,
    unbounded_tool_calls: usize,
    missing_expected_ref_rows: usize,
    missing_expected_ref_total: usize,
    extra_observed_ref_rows: usize,
    extra_observed_ref_total: usize,
    partial_result_rows: usize,
    partial_result_by_action: BTreeMap<String, usize>,
    by_action: BTreeMap<String, usize>,
    latency_ms_by_action: BTreeMap<String, ActionLatencySummary>,
    elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
struct ActionLatencySummary {
    count: usize,
    avg_ms: f64,
    max_ms: u128,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    ensure_output_dir(&args.output, args.force)?;

    let trajectories = select_trajectories(read_trajectories(&args.trajectories)?, &args);
    let mut predictions = read_predictions(&args.predictions)?;
    let server = match args.endpoint.as_deref() {
        Some(endpoint) => KernelMcpServer::grpc_with_tls(
            endpoint,
            KernelMcpGrpcTlsConfig::from_env_for_endpoint(Some(endpoint)),
        ),
        None => KernelMcpServer::try_from_env()
            .map_err(|error| format!("failed to configure MCP gRPC backend from env: {error}"))?,
    };
    let endpoint_label = args
        .endpoint
        .clone()
        .or_else(|| env::var("KMP_KERNEL_GRPC_ENDPOINT").ok())
        .unwrap_or_else(|| "env".to_string());

    let started = Instant::now();
    let mut request_id = 1u64;
    let mut rows = Vec::new();
    let mut counters = Counters::default();

    for trajectory in &trajectories {
        let row_index = rows.len() + 1;
        let row = replay_one(
            &server,
            &mut request_id,
            trajectory,
            take_prediction(&mut predictions, &trajectory.step_id).as_ref(),
            &mut counters,
        )
        .await;
        log_progress(&args, row_index, trajectories.len(), &row, &started);
        rows.push(row);
    }

    let summary = ReplaySummary {
        replayer: REPLAYER,
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        endpoint: endpoint_label,
        trajectories: args.trajectories.display().to_string(),
        predictions: args.predictions.display().to_string(),
        output: args.output.display().to_string(),
        selected: trajectories.len(),
        tool_calls: counters.tool_calls,
        stop_actions: counters.stop_actions,
        executed_tool_calls: counters.executed_tool_calls,
        successful_tool_calls: counters.successful_tool_calls,
        failed_tool_calls: counters.failed_tool_calls,
        missing_predictions: counters.missing_predictions,
        invalid_predictions: counters.invalid_predictions,
        unbounded_tool_calls: counters.unbounded_tool_calls,
        missing_expected_ref_rows: counters.missing_expected_ref_rows,
        missing_expected_ref_total: counters.missing_expected_ref_total,
        extra_observed_ref_rows: rows
            .iter()
            .filter(|row| !row.extra_observed_refs.is_empty())
            .count(),
        extra_observed_ref_total: rows.iter().map(|row| row.extra_observed_refs.len()).sum(),
        partial_result_rows: rows.iter().filter(|row| row.partial_result).count(),
        partial_result_by_action: partial_result_by_action(&rows),
        by_action: by_action(&rows),
        latency_ms_by_action: latency_ms_by_action(&rows),
        elapsed_ms: started.elapsed().as_millis(),
    };

    write_jsonl(
        &args.output.join("results.jsonl"),
        rows.iter().map(serde_json::to_value),
    )?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);

    if summary.missing_predictions > 0
        || summary.invalid_predictions > 0
        || summary.unbounded_tool_calls > 0
        || summary.failed_tool_calls > 0
        || summary.missing_expected_ref_rows > 0
    {
        return Err(format!(
            "kernel operator MCP replay failed: missing_predictions={} invalid_predictions={} unbounded_tool_calls={} failed_tool_calls={} missing_expected_ref_rows={}",
            summary.missing_predictions,
            summary.invalid_predictions,
            summary.unbounded_tool_calls,
            summary.failed_tool_calls,
            summary.missing_expected_ref_rows
        )
        .into());
    }
    Ok(())
}

#[derive(Debug, Default)]
struct Counters {
    tool_calls: usize,
    stop_actions: usize,
    executed_tool_calls: usize,
    successful_tool_calls: usize,
    failed_tool_calls: usize,
    missing_predictions: usize,
    invalid_predictions: usize,
    unbounded_tool_calls: usize,
    missing_expected_ref_rows: usize,
    missing_expected_ref_total: usize,
}

async fn replay_one(
    server: &KernelMcpServer,
    request_id: &mut u64,
    trajectory: &Trajectory,
    prediction: Option<&Prediction>,
    counters: &mut Counters,
) -> ReplayRow {
    let expected_observed_refs = expected_observed_refs(trajectory);
    let Some(prediction) = prediction else {
        counters.missing_predictions += 1;
        return failed_row(
            trajectory,
            "invalid".to_string(),
            "missing_prediction".to_string(),
            expected_observed_refs,
        );
    };

    let action = &prediction.action;
    let label = action_label(action);
    match action_type(action) {
        Some("stop") => {
            counters.stop_actions += 1;
            ReplayRow {
                step_id: trajectory.step_id.clone(),
                about: trajectory.about.clone(),
                mode: trajectory.mode.clone(),
                task_family: trajectory.task_family.clone(),
                action_label: label,
                elapsed_ms: 0,
                success: true,
                error: None,
                partial_result: false,
                page: None,
                expected_observed_refs,
                observed_refs: Vec::new(),
                missing_expected_refs: Vec::new(),
                extra_observed_refs: Vec::new(),
            }
        }
        Some("tool_call") => {
            counters.tool_calls += 1;
            let Some(tool) = tool(action) else {
                counters.invalid_predictions += 1;
                return failed_row(
                    trajectory,
                    label,
                    "tool_call_missing_tool".to_string(),
                    expected_observed_refs,
                );
            };
            let Some(arguments) = action.get("arguments") else {
                counters.invalid_predictions += 1;
                return failed_row(
                    trajectory,
                    label,
                    "tool_call_missing_arguments".to_string(),
                    expected_observed_refs,
                );
            };
            if !kernel_operator_is_bounded_tool_call(tool, arguments) {
                counters.unbounded_tool_calls += 1;
                return failed_row(
                    trajectory,
                    label,
                    format!("unbounded_tool_call:{tool}"),
                    expected_observed_refs,
                );
            }

            counters.executed_tool_calls += 1;
            let id = *request_id;
            *request_id = request_id.checked_add(1).unwrap_or(u64::MAX);
            let started = Instant::now();
            match call_mcp_tool(server, id, tool, arguments).await {
                Ok(content) => {
                    let elapsed_ms = started.elapsed().as_millis();
                    let page = page_from_content(&content);
                    let partial_result = page.as_ref().is_some_and(|page| page.has_more);
                    let observed_refs = collect_memory_refs(&content)
                        .into_iter()
                        .collect::<Vec<_>>();
                    let (missing_expected_refs, extra_observed_refs) =
                        ref_differences(&expected_observed_refs, &observed_refs);
                    if missing_expected_refs.is_empty() {
                        counters.successful_tool_calls += 1;
                    } else {
                        counters.failed_tool_calls += 1;
                        counters.missing_expected_ref_rows += 1;
                        counters.missing_expected_ref_total += missing_expected_refs.len();
                    }
                    ReplayRow {
                        step_id: trajectory.step_id.clone(),
                        about: trajectory.about.clone(),
                        mode: trajectory.mode.clone(),
                        task_family: trajectory.task_family.clone(),
                        action_label: label,
                        elapsed_ms,
                        success: missing_expected_refs.is_empty(),
                        error: if missing_expected_refs.is_empty() {
                            None
                        } else {
                            Some("missing_expected_refs".to_string())
                        },
                        partial_result,
                        page,
                        expected_observed_refs,
                        observed_refs,
                        missing_expected_refs,
                        extra_observed_refs,
                    }
                }
                Err(error) => {
                    counters.failed_tool_calls += 1;
                    failed_row(trajectory, label, error.to_string(), expected_observed_refs)
                }
            }
        }
        Some(other) => {
            counters.invalid_predictions += 1;
            failed_row(
                trajectory,
                "invalid".to_string(),
                format!("unsupported_action_type:{other}"),
                expected_observed_refs,
            )
        }
        None => {
            counters.invalid_predictions += 1;
            failed_row(
                trajectory,
                "invalid".to_string(),
                "missing_action_type".to_string(),
                expected_observed_refs,
            )
        }
    }
}

async fn call_mcp_tool(
    server: &KernelMcpServer,
    id: u64,
    name: &str,
    arguments: &Value,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments
        }
    });
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .ok_or_else(|| format!("MCP tool `{name}` returned no JSON-RPC response"))?;
    let value = serde_json::from_str::<Value>(&response)?;
    if let Some(error) = value.get("error") {
        return Err(format!("MCP tool `{name}` returned JSON-RPC error: {error}").into());
    }
    let result = value
        .get("result")
        .ok_or_else(|| format!("MCP tool `{name}` returned no result"))?;
    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!("MCP tool `{name}` returned isError=true: {result}").into());
    }
    result
        .get("structuredContent")
        .cloned()
        .ok_or_else(|| format!("MCP tool `{name}` returned no structuredContent").into())
}

fn failed_row(
    trajectory: &Trajectory,
    action_label: String,
    error: String,
    expected_observed_refs: Vec<String>,
) -> ReplayRow {
    ReplayRow {
        step_id: trajectory.step_id.clone(),
        about: trajectory.about.clone(),
        mode: trajectory.mode.clone(),
        task_family: trajectory.task_family.clone(),
        action_label,
        elapsed_ms: 0,
        success: false,
        error: Some(error),
        partial_result: false,
        page: None,
        expected_observed_refs,
        observed_refs: Vec::new(),
        missing_expected_refs: Vec::new(),
        extra_observed_refs: Vec::new(),
    }
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut trajectories = None;
    let mut predictions = None;
    let mut output = None;
    let mut endpoint = None;
    let mut limit = None;
    let mut offset = 0usize;
    let mut log_progress_every = None;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--trajectories" => {
                trajectories = Some(PathBuf::from(next_arg(&mut args, "--trajectories")?));
            }
            "--predictions" => {
                predictions = Some(PathBuf::from(next_arg(&mut args, "--predictions")?));
            }
            "--output" => output = Some(PathBuf::from(next_arg(&mut args, "--output")?)),
            "--endpoint" => endpoint = Some(next_arg(&mut args, "--endpoint")?),
            "--limit" => limit = Some(next_arg(&mut args, "--limit")?.parse()?),
            "--offset" => offset = next_arg(&mut args, "--offset")?.parse()?,
            "--log-progress-every" => {
                log_progress_every = Some(next_arg(&mut args, "--log-progress-every")?.parse()?);
            }
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
        predictions: predictions.ok_or("--predictions is required")?,
        output: output.ok_or("--output is required")?,
        endpoint,
        limit,
        offset,
        log_progress_every,
        force,
    })
}

fn usage() -> String {
    "usage: kernel_operator_mcp_replay --trajectories <trajectories.jsonl> --predictions <predictions.jsonl> --output <dir> [--endpoint URL] [--limit n] [--offset n] [--log-progress-every n] [--force]".to_string()
}

fn next_arg(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    args.next()
        .ok_or_else(|| format!("{name} requires a value").into())
}

fn read_trajectories(path: &Path) -> Result<Vec<Trajectory>, Box<dyn Error + Send + Sync>> {
    read_jsonl(path)?
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            serde_json::from_value(value).map_err(|error| {
                format!(
                    "failed to parse trajectory {} line {}: {error}",
                    path.display(),
                    index + 1
                )
                .into()
            })
        })
        .collect()
}

fn read_predictions(
    path: &Path,
) -> Result<BTreeMap<String, VecDeque<Prediction>>, Box<dyn Error + Send + Sync>> {
    let mut predictions = BTreeMap::new();
    for (index, value) in read_jsonl(path)?.into_iter().enumerate() {
        let location = format!("{}:{}", path.display(), index + 1);
        let step_id = value
            .get("step_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{location} missing required string field `step_id`"))?;
        let action = value
            .get("action")
            .or_else(|| value.get("target_action"))
            .cloned()
            .ok_or_else(|| format!("{location} missing `action` or `target_action`"))?;
        predictions
            .entry(step_id.to_string())
            .or_insert_with(VecDeque::new)
            .push_back(Prediction { action });
    }
    Ok(predictions)
}

fn take_prediction(
    predictions: &mut BTreeMap<String, VecDeque<Prediction>>,
    step_id: &str,
) -> Option<Prediction> {
    predictions.get_mut(step_id).and_then(VecDeque::pop_front)
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

fn select_trajectories(values: Vec<Trajectory>, args: &Args) -> Vec<Trajectory> {
    values
        .into_iter()
        .skip(args.offset)
        .take(args.limit.unwrap_or(usize::MAX))
        .collect()
}

fn ensure_output_dir(output: &Path, force: bool) -> Result<(), Box<dyn Error + Send + Sync>> {
    if output.exists() {
        if !force {
            return Err(format!(
                "output directory already exists: {}; pass --force to replace generated files",
                output.display()
            )
            .into());
        }
        if !output.is_dir() {
            return Err(format!(
                "output path exists and is not a directory: {}",
                output.display()
            )
            .into());
        }
    } else {
        fs::create_dir_all(output)?;
    }
    Ok(())
}

fn write_json_pretty(
    path: &Path,
    value: &impl Serialize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
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

fn log_progress(args: &Args, processed: usize, total: usize, row: &ReplayRow, started: &Instant) {
    let Some(every) = args.log_progress_every else {
        return;
    };
    if every == 0 {
        return;
    }
    if !processed.is_multiple_of(every) && processed != total {
        return;
    }
    eprintln!(
        "{}",
        json!({
            "event": "kernel_operator_mcp_replay.progress",
            "processed": processed,
            "total": total,
            "step_id": row.step_id,
            "action": row.action_label,
            "success": row.success,
            "partial_result": row.partial_result,
            "elapsed_ms": started.elapsed().as_millis(),
        })
    );
}

fn expected_observed_refs(trajectory: &Trajectory) -> Vec<String> {
    trajectory
        .observed_outcome
        .as_ref()
        .and_then(|value| value.get("observed_refs"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn ref_differences(expected: &[String], observed: &[String]) -> (Vec<String>, Vec<String>) {
    let expected_set = expected.iter().cloned().collect::<BTreeSet<_>>();
    let observed_set = observed.iter().cloned().collect::<BTreeSet<_>>();
    let missing = expected_set.difference(&observed_set).cloned().collect();
    let extra = observed_set.difference(&expected_set).cloned().collect();
    (missing, extra)
}

fn by_action(rows: &[ReplayRow]) -> BTreeMap<String, usize> {
    let mut by_action = BTreeMap::new();
    for row in rows {
        *by_action.entry(row.action_label.clone()).or_default() += 1;
    }
    by_action
}

fn latency_ms_by_action(rows: &[ReplayRow]) -> BTreeMap<String, ActionLatencySummary> {
    let mut by_action = BTreeMap::<String, LatencyDraft>::new();
    for row in rows {
        let draft = by_action.entry(row.action_label.clone()).or_default();
        draft.count += 1;
        draft.total_ms += row.elapsed_ms;
        draft.max_ms = draft.max_ms.max(row.elapsed_ms);
    }
    by_action
        .into_iter()
        .map(|(action, draft)| {
            let avg_ms = if draft.count == 0 {
                0.0
            } else {
                draft.total_ms as f64 / draft.count as f64
            };
            (
                action,
                ActionLatencySummary {
                    count: draft.count,
                    avg_ms,
                    max_ms: draft.max_ms,
                },
            )
        })
        .collect()
}

fn partial_result_by_action(rows: &[ReplayRow]) -> BTreeMap<String, usize> {
    let mut by_action = BTreeMap::new();
    for row in rows.iter().filter(|row| row.partial_result) {
        *by_action.entry(row.action_label.clone()).or_default() += 1;
    }
    by_action
}

#[derive(Debug, Default)]
struct LatencyDraft {
    count: usize,
    total_ms: u128,
    max_ms: u128,
}

fn page_from_content(content: &Value) -> Option<ReplayPage> {
    let page = content.get("page")?;
    let object = page.as_object()?;
    let next_cursor = object
        .get("next_cursor")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string);

    Some(ReplayPage {
        returned: object.get("returned").and_then(Value::as_u64),
        total: object.get("total").and_then(Value::as_u64),
        has_more: object
            .get("has_more")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        next_cursor,
    })
}

fn collect_memory_refs(value: &Value) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    collect_memory_refs_from_field(value, None, &mut refs);
    refs
}

fn collect_memory_refs_from_field(value: &Value, field: Option<&str>, refs: &mut BTreeSet<String>) {
    match value {
        Value::String(value) if field_allows_memory_ref(field) && looks_like_memory_ref(value) => {
            refs.insert(value.to_string());
        }
        Value::Array(values) => {
            for value in values {
                collect_memory_refs_from_field(value, field, refs);
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                collect_memory_refs_from_field(value, Some(key), refs);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn field_allows_memory_ref(field: Option<&str>) -> bool {
    let Some(field) = field else {
        return false;
    };
    matches!(
        field,
        "anchor"
            | "because"
            | "cursor"
            | "evidence"
            | "from"
            | "node"
            | "path"
            | "proof"
            | "ref"
            | "reference"
            | "references"
            | "refs"
            | "related"
            | "source"
            | "sources"
            | "supports"
            | "target"
            | "to"
            | "trace"
    ) || field.ends_with("_ref")
        || field.ends_with("_refs")
        || field.ends_with("_ref_id")
        || field.ends_with("_ref_ids")
}

fn looks_like_memory_ref(value: &str) -> bool {
    !value.contains(' ')
        && value.len() <= 500
        && (value.starts_with("memoryarena:")
            || value.starts_with("longmemeval:")
            || value.starts_with("turn:")
            || value.starts_with("question:")
            || value.starts_with("evidence:")
            || value.starts_with("about:"))
}

fn action_label(action: &Value) -> String {
    match action_type(action) {
        Some("tool_call") => format!("tool_call:{}", tool(action).unwrap_or("unknown")),
        Some(kind) => kind.to_string(),
        None => "invalid".to_string(),
    }
}

fn action_type(action: &Value) -> Option<&str> {
    action.get("type").and_then(Value::as_str)
}

fn tool(action: &Value) -> Option<&str> {
    action.get("tool").and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_memory_refs_accepts_memoryarena_and_longmemeval_refs() {
        let refs = collect_memory_refs(&json!({
            "proof": {
                "evidence": [
                    {
                        "supports": [
                            "memoryarena:run:demo:task:1",
                            "turn:run:lme:question:q:answer:a:1",
                            "evidence:run:lme:question:q:answer:a:1"
                        ],
                        "source": "question:run:lme:question:q"
                    }
                ],
                "path": [
                    {
                        "from": "about:longmemeval:run:lme:item:q:dimension:longmemeval:session:s1",
                        "to": "longmemeval:run:lme:item:q"
                    }
                ]
            },
            "answer": "mentions turn:run:lme:question:q:answer:a:2 but answer text is not proof"
        }));

        assert!(refs.contains("memoryarena:run:demo:task:1"));
        assert!(refs.contains("turn:run:lme:question:q:answer:a:1"));
        assert!(refs.contains("evidence:run:lme:question:q:answer:a:1"));
        assert!(refs.contains("question:run:lme:question:q"));
        assert!(refs.contains("about:longmemeval:run:lme:item:q:dimension:longmemeval:session:s1"));
        assert!(refs.contains("longmemeval:run:lme:item:q"));
        assert!(!refs.contains("turn:run:lme:question:q:answer:a:2"));
    }

    #[test]
    fn take_prediction_preserves_duplicate_step_id_order() {
        let mut predictions = BTreeMap::from([(
            "same-step".to_string(),
            VecDeque::from([
                Prediction {
                    action: json!({"type": "tool_call", "tool": "kmp_ask"}),
                },
                Prediction {
                    action: json!({"type": "stop"}),
                },
            ]),
        )]);

        let first = take_prediction(&mut predictions, "same-step").expect("first prediction");
        let second = take_prediction(&mut predictions, "same-step").expect("second prediction");

        assert_eq!(first.action["tool"], "kmp_ask");
        assert_eq!(second.action["type"], "stop");
        assert!(take_prediction(&mut predictions, "same-step").is_none());
    }
}
