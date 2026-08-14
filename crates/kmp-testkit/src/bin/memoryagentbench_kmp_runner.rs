use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use kmp_mcp::{KernelMcpGrpcTlsConfig, KernelMcpServer};
use kmp_testkit::MemoryAgentBenchExpected;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    artifacts: PathBuf,
    endpoint: Option<String>,
    output: PathBuf,
    limit_items: Option<usize>,
    force: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct MemoryAgentBenchEvent {
    event: String,
    item_id: String,
    split: String,
    source: String,
    event_index: usize,
    phase: String,
    #[serde(default)]
    query_index: Option<usize>,
    about: String,
    tool: String,
    arguments: Value,
}

#[derive(Debug, Serialize)]
struct EventResult {
    event: String,
    item_id: String,
    split: String,
    source: String,
    event_index: usize,
    phase: String,
    query_index: Option<usize>,
    about: String,
    tool: String,
    elapsed_ms: u128,
    success: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct AskResult {
    item_id: String,
    split: String,
    source: String,
    query_index: usize,
    about: String,
    question: String,
    expected_answer: Value,
    question_id: Option<String>,
    qa_pair_id: Option<String>,
    question_type: Option<String>,
    question_date: Option<String>,
    allowed_known_at_refs: Vec<String>,
    observed_refs: Vec<String>,
    observed_allowed_refs: Vec<String>,
    unexpected_refs: Vec<String>,
    missing_allowed_refs: Vec<String>,
    known_at_clean: bool,
    lexical_answer_hit: bool,
    ask_answer: Option<String>,
    ask_content: Value,
    ask_elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
struct RunSummary {
    benchmark: &'static str,
    runner: &'static str,
    generated_at_unix_seconds: u64,
    artifacts: String,
    endpoint: String,
    total_events: usize,
    ingest_events: usize,
    ask_events: usize,
    successful_events: usize,
    failed_events: usize,
    known_at_clean_asks: usize,
    lexical_answer_hits: usize,
    unexpected_ref_asks: usize,
    missing_allowed_ref_asks: usize,
    elapsed_ms: u128,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    ensure_output_dir(&args.output, args.force)?;

    let events = read_events(&args.artifacts.join("events.jsonl"), args.limit_items)?;
    let expected = read_expected(&args.artifacts.join("expected.jsonl"), args.limit_items)?;
    let expected_by_ask = expected_by_ask_key(expected)?;
    validate_events_expected_alignment(&events, &expected_by_ask)?;

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
    let mut event_results = Vec::new();
    let mut ask_results = Vec::new();

    for event in &events {
        validate_event(event)?;
        let event_started = Instant::now();
        let response = call_mcp_tool(&server, request_id, &event.tool, &event.arguments).await;
        request_id = request_id.checked_add(1).ok_or("request id overflow")?;
        let elapsed_ms = event_started.elapsed().as_millis();

        match response {
            Ok(response) => {
                let content_result = assert_tool_success(&response, &event.tool, event);
                match content_result {
                    Ok(content) => {
                        if event.event == "ask" {
                            let expected = expected_by_ask
                                .get(&(event.item_id.clone(), event.query_index.unwrap_or(0)))
                                .ok_or_else(|| {
                                    format!(
                                        "missing expected row for item {} query {:?}",
                                        event.item_id, event.query_index
                                    )
                                })?;
                            ask_results
                                .push(build_ask_result(event, expected, content, elapsed_ms));
                        }
                        event_results.push(success_event_result(event, elapsed_ms));
                    }
                    Err(error) => {
                        event_results.push(failed_event_result(event, elapsed_ms, error));
                    }
                }
            }
            Err(error) => {
                event_results.push(failed_event_result(event, elapsed_ms, error.to_string()));
            }
        }
    }

    let summary = summarize_run(
        &args,
        endpoint_label,
        started.elapsed().as_millis(),
        &event_results,
        &ask_results,
    )?;
    write_jsonl(
        &args.output.join("event_results.jsonl"),
        event_results.iter().map(serde_json::to_value),
    )?;
    write_jsonl(
        &args.output.join("results.jsonl"),
        ask_results.iter().map(serde_json::to_value),
    )?;
    write_jsonl(
        &args.output.join("hypotheses.jsonl"),
        ask_results.iter().map(|item| {
            Ok(json!({
                "item_id": item.item_id,
                "query_index": item.query_index,
                "question_id": item.question_id,
                "qa_pair_id": item.qa_pair_id,
                "hypothesis": item.ask_answer.as_deref().unwrap_or_default()
            }))
        }),
    )?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
    if summary.failed_events > 0 {
        return Err(format!(
            "MemoryAgentBench KMP runner failed: {} event(s) failed",
            summary.failed_events
        )
        .into());
    }
    Ok(())
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
    Ok(value)
}

fn assert_tool_success<'a>(
    response: &'a Value,
    tool: &str,
    event: &MemoryAgentBenchEvent,
) -> Result<&'a Value, String> {
    let result = response
        .get("result")
        .ok_or_else(|| format!("{tool} event {} returned no result", event.event_index))?;
    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!(
            "{tool} event {} for item {} failed: {result}",
            event.event_index, event.item_id
        ));
    }
    result.get("structuredContent").ok_or_else(|| {
        format!(
            "{tool} event {} for item {} returned no structuredContent",
            event.event_index, event.item_id
        )
    })
}

fn build_ask_result(
    _event: &MemoryAgentBenchEvent,
    expected: &MemoryAgentBenchExpected,
    ask_content: &Value,
    ask_elapsed_ms: u128,
) -> AskResult {
    let observed_refs = collect_memoryagentbench_refs(ask_content);
    let observed_entry_refs = observed_refs
        .iter()
        .filter(|reference| is_memoryagentbench_entry_ref(reference))
        .cloned()
        .collect::<BTreeSet<_>>();
    let allowed = expected
        .available_ref_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let observed_allowed_refs = observed_entry_refs
        .intersection(&allowed)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_refs = observed_entry_refs
        .difference(&allowed)
        .cloned()
        .collect::<Vec<_>>();
    let missing_allowed_refs = allowed
        .difference(&observed_entry_refs)
        .cloned()
        .collect::<Vec<_>>();
    let known_at_clean = unexpected_refs.is_empty();
    let ask_answer = ask_content
        .get("answer")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let lexical_answer_hit = answer_contains_expected(&expected.answer, ask_answer.as_deref());

    AskResult {
        item_id: expected.item_id.clone(),
        split: expected.split.clone(),
        source: expected.source.clone(),
        query_index: expected.query_index,
        about: expected.about.clone(),
        question: expected.question.clone(),
        expected_answer: expected.answer.clone(),
        question_id: expected.question_id.clone(),
        qa_pair_id: expected.qa_pair_id.clone(),
        question_type: expected.question_type.clone(),
        question_date: expected.question_date.clone(),
        allowed_known_at_refs: expected.available_ref_ids.clone(),
        observed_refs: observed_refs.into_iter().collect(),
        observed_allowed_refs,
        unexpected_refs,
        missing_allowed_refs,
        known_at_clean,
        lexical_answer_hit,
        ask_answer,
        ask_content: ask_content.clone(),
        ask_elapsed_ms,
    }
}

fn collect_memoryagentbench_refs(value: &Value) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    collect_memoryagentbench_refs_from_field(value, None, &mut refs);
    refs
}

fn collect_memoryagentbench_refs_from_field(
    value: &Value,
    field: Option<&str>,
    refs: &mut BTreeSet<String>,
) {
    match value {
        Value::String(value)
            if field_allows_memory_ref(field) && looks_like_memoryagentbench_ref(value) =>
        {
            refs.insert(value.to_string());
        }
        Value::Array(values) => {
            for value in values {
                collect_memoryagentbench_refs_from_field(value, field, refs);
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                collect_memoryagentbench_refs_from_field(value, Some(key), refs);
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

fn looks_like_memoryagentbench_ref(value: &str) -> bool {
    value.starts_with("memoryagentbench:") && !value.contains(' ') && value.len() <= 420
}

fn is_memoryagentbench_entry_ref(value: &str) -> bool {
    value.contains(":context:")
}

fn answer_contains_expected(expected_answer: &Value, ask_answer: Option<&str>) -> bool {
    let Some(ask_answer) = ask_answer else {
        return false;
    };
    expected_answer_candidates(expected_answer)
        .into_iter()
        .map(|candidate| normalize_for_lexical_match(&candidate))
        .filter(|candidate| !candidate.is_empty())
        .any(|candidate| normalize_for_lexical_match(ask_answer).contains(&candidate))
}

fn expected_answer_candidates(value: &Value) -> Vec<String> {
    match value {
        Value::String(value) => vec![value.clone()],
        Value::Array(values) => values
            .iter()
            .flat_map(expected_answer_candidates)
            .collect::<Vec<_>>(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => {
            vec![value.to_string()]
        }
    }
}

fn normalize_for_lexical_match(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_was_space = true;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }
    normalized.trim().to_string()
}

fn validate_event(event: &MemoryAgentBenchEvent) -> Result<(), Box<dyn Error + Send + Sync>> {
    match event.event.as_str() {
        "ingest" if event.tool == "kernel_ingest" => Ok(()),
        "ask" if event.tool == "kernel_ask" && event.query_index.is_some() => Ok(()),
        "ask" => Err(format!(
            "MemoryAgentBench ask event {} for item {} must use kernel_ask and include query_index",
            event.event_index, event.item_id
        )
        .into()),
        other => Err(format!(
            "unsupported MemoryAgentBench event `{other}` at event_index {} for item {}",
            event.event_index, event.item_id
        )
        .into()),
    }
}

fn success_event_result(event: &MemoryAgentBenchEvent, elapsed_ms: u128) -> EventResult {
    EventResult {
        event: event.event.clone(),
        item_id: event.item_id.clone(),
        split: event.split.clone(),
        source: event.source.clone(),
        event_index: event.event_index,
        phase: event.phase.clone(),
        query_index: event.query_index,
        about: event.about.clone(),
        tool: event.tool.clone(),
        elapsed_ms,
        success: true,
        error: None,
    }
}

fn failed_event_result(
    event: &MemoryAgentBenchEvent,
    elapsed_ms: u128,
    error: String,
) -> EventResult {
    EventResult {
        event: event.event.clone(),
        item_id: event.item_id.clone(),
        split: event.split.clone(),
        source: event.source.clone(),
        event_index: event.event_index,
        phase: event.phase.clone(),
        query_index: event.query_index,
        about: event.about.clone(),
        tool: event.tool.clone(),
        elapsed_ms,
        success: false,
        error: Some(error),
    }
}

fn summarize_run(
    args: &Args,
    endpoint: String,
    elapsed_ms: u128,
    event_results: &[EventResult],
    ask_results: &[AskResult],
) -> Result<RunSummary, Box<dyn Error + Send + Sync>> {
    Ok(RunSummary {
        benchmark: "MemoryAgentBench",
        runner: "memoryagentbench-kmp-runner-v1",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        artifacts: args.artifacts.display().to_string(),
        endpoint,
        total_events: event_results.len(),
        ingest_events: event_results
            .iter()
            .filter(|event| event.event == "ingest")
            .count(),
        ask_events: event_results
            .iter()
            .filter(|event| event.event == "ask")
            .count(),
        successful_events: event_results.iter().filter(|event| event.success).count(),
        failed_events: event_results.iter().filter(|event| !event.success).count(),
        known_at_clean_asks: ask_results
            .iter()
            .filter(|result| result.known_at_clean)
            .count(),
        lexical_answer_hits: ask_results
            .iter()
            .filter(|result| result.lexical_answer_hit)
            .count(),
        unexpected_ref_asks: ask_results
            .iter()
            .filter(|result| !result.unexpected_refs.is_empty())
            .count(),
        missing_allowed_ref_asks: ask_results
            .iter()
            .filter(|result| !result.missing_allowed_refs.is_empty())
            .count(),
        elapsed_ms,
    })
}

fn expected_by_ask_key(
    expected: Vec<MemoryAgentBenchExpected>,
) -> Result<BTreeMap<(String, usize), MemoryAgentBenchExpected>, Box<dyn Error + Send + Sync>> {
    let mut by_key = BTreeMap::new();
    for item in expected {
        let key = (item.item_id.clone(), item.query_index);
        if by_key.insert(key.clone(), item).is_some() {
            return Err(
                format!("duplicate expected row for item {} query {}", key.0, key.1).into(),
            );
        }
    }
    Ok(by_key)
}

fn validate_events_expected_alignment(
    events: &[MemoryAgentBenchEvent],
    expected_by_ask: &BTreeMap<(String, usize), MemoryAgentBenchExpected>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut ask_keys = BTreeSet::new();
    for event in events.iter().filter(|event| event.event == "ask") {
        let query_index = event.query_index.ok_or_else(|| {
            format!(
                "ask event {} for item {} has no query_index",
                event.event_index, event.item_id
            )
        })?;
        let key = (event.item_id.clone(), query_index);
        if !ask_keys.insert(key.clone()) {
            return Err(format!("duplicate ask event for item {} query {}", key.0, key.1).into());
        }
        if !expected_by_ask.contains_key(&key) {
            return Err(format!("missing expected row for item {} query {}", key.0, key.1).into());
        }
    }

    for key in expected_by_ask.keys() {
        if !ask_keys.contains(key) {
            return Err(format!(
                "expected row has no ask event for item {} query {}",
                key.0, key.1
            )
            .into());
        }
    }

    Ok(())
}

fn read_events(
    path: &Path,
    limit_items: Option<usize>,
) -> Result<Vec<MemoryAgentBenchEvent>, Box<dyn Error + Send + Sync>> {
    let selected_item_ids = selected_item_ids(path, limit_items)?;
    let events = read_jsonl(path)?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|event: &MemoryAgentBenchEvent| {
            selected_item_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&event.item_id))
        })
        .collect::<Vec<_>>();
    Ok(events)
}

fn read_expected(
    path: &Path,
    limit_items: Option<usize>,
) -> Result<Vec<MemoryAgentBenchExpected>, Box<dyn Error + Send + Sync>> {
    let selected_item_ids = selected_item_ids_from_expected(path, limit_items)?;
    let expected = read_jsonl(path)?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|expected: &MemoryAgentBenchExpected| {
            selected_item_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&expected.item_id))
        })
        .collect::<Vec<_>>();
    Ok(expected)
}

fn selected_item_ids(
    path: &Path,
    limit_items: Option<usize>,
) -> Result<Option<BTreeSet<String>>, Box<dyn Error + Send + Sync>> {
    let Some(limit) = limit_items else {
        return Ok(None);
    };
    let mut selected = BTreeSet::new();
    for value in read_jsonl(path)? {
        let item_id = required_string(&value, "item_id")?;
        selected.insert(item_id);
        if selected.len() >= limit {
            break;
        }
    }
    Ok(Some(selected))
}

fn selected_item_ids_from_expected(
    path: &Path,
    limit_items: Option<usize>,
) -> Result<Option<BTreeSet<String>>, Box<dyn Error + Send + Sync>> {
    let Some(limit) = limit_items else {
        return Ok(None);
    };
    let mut selected = BTreeSet::new();
    for value in read_jsonl(path)? {
        let item_id = required_string(&value, "item_id")?;
        selected.insert(item_id);
        if selected.len() >= limit {
            break;
        }
    }
    Ok(Some(selected))
}

fn read_jsonl(path: &Path) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
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

fn required_string(value: &Value, field: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing string field `{field}`").into())
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

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut artifacts = None;
    let mut endpoint = None;
    let mut output = None;
    let mut limit_items = None;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--artifacts" => artifacts = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--endpoint" => endpoint = Some(required_flag_value(&mut args, &arg)?),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--limit-items" => {
                let value = required_flag_value(&mut args, &arg)?;
                let parsed = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --limit-items value `{value}`: {error}"))?;
                if parsed == 0 {
                    return Err("--limit-items must be greater than zero".into());
                }
                limit_items = Some(parsed);
            }
            "--force" => force = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument `{other}`").into()),
        }
    }

    Ok(Args {
        artifacts: artifacts.ok_or("--artifacts is required")?,
        endpoint,
        output: output.ok_or("--output is required")?,
        limit_items,
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
        "Usage: memoryagentbench_kmp_runner --artifacts <adapter-output-dir> --output <run-dir> [--endpoint http://host] [--limit-items N] [--force]"
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn collects_memoryagentbench_refs_recursively() {
        let refs = collect_memoryagentbench_refs(&json!({
            "answer": "text memoryagentbench:not-a-ref with spaces",
            "because": [
                {"ref": "memoryagentbench:split:x:source:y:item:1:context:fact:2"}
            ],
            "proof": {
                "path": [
                    {
                        "from": "memoryagentbench:split:x:source:y:item:1:context:fact:3",
                        "to": "memoryagentbench:split:x:source:y:item:1:context:fact:2"
                    }
                ]
            }
        }));

        assert!(refs.contains("memoryagentbench:split:x:source:y:item:1:context:fact:2"));
        assert!(refs.contains("memoryagentbench:split:x:source:y:item:1:context:fact:3"));
        assert!(!refs.contains("memoryagentbench:not-a-ref with spaces"));
    }

    #[test]
    fn memoryagentbench_ask_result_reports_known_at_gaps() {
        let event = MemoryAgentBenchEvent {
            event: "ask".to_string(),
            item_id: "item-1".to_string(),
            split: "conflict_resolution".to_string(),
            source: "factconsolidation_mh_32k".to_string(),
            event_index: 2,
            phase: "query".to_string(),
            query_index: Some(1),
            about: "memoryagentbench:split:conflict_resolution:source:fact:item:item-1".to_string(),
            tool: "kernel_ask".to_string(),
            arguments: json!({}),
        };
        let expected = MemoryAgentBenchExpected {
            item_id: "item-1".to_string(),
            split: "conflict_resolution".to_string(),
            source: "factconsolidation_mh_32k".to_string(),
            query_index: 1,
            question: "Q?".to_string(),
            answer: json!(["A"]),
            about: "memoryagentbench:split:conflict_resolution:source:fact:item:item-1".to_string(),
            question_id: Some("q1".to_string()),
            qa_pair_id: Some("qa1".to_string()),
            question_type: None,
            question_date: None,
            available_ref_ids: vec![
                "memoryagentbench:split:conflict_resolution:source:fact:item:item-1:context:fact:1"
                    .to_string(),
                "memoryagentbench:split:conflict_resolution:source:fact:item:item-1:context:fact:2"
                    .to_string(),
            ],
        };

        let result = build_ask_result(
            &event,
            &expected,
            &json!({
                "answer": "A",
                "because": [
                    {
                        "ref": "memoryagentbench:split:conflict_resolution:source:fact:item:item-1:context:fact:2"
                    }
                ]
            }),
            12,
        );

        assert!(result.known_at_clean);
        assert!(result.lexical_answer_hit);
        assert_eq!(result.missing_allowed_refs.len(), 1);
    }
}
