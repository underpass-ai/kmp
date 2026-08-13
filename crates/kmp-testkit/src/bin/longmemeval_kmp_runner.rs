use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use kmp_interpretation::{
    ComposedEvidenceReader, EvidenceFragment, EvidenceInterpretationInput, EvidenceReaderOutput,
    EvidenceReaderPluginConfiguration, EvidenceReaderRequest,
};
use kmp_mcp::{KernelMcpGrpcTlsConfig, KernelMcpServer};
use kmp_testkit::{
    LlmProvider, LongMemEvalExpected, LongMemEvalSmartWriter, LongMemEvalSmartWriterConfig,
    LongMemEvalSmartWriterItem, LongMemEvalSmartWriterResult, LongMemEvalSmartWriterSummary,
    summarize_longmemeval_smart_writer,
};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
struct Args {
    artifacts: PathBuf,
    endpoint: Option<String>,
    output: PathBuf,
    limit: Option<usize>,
    smart_writer: bool,
    log_mcp_navigation: bool,
    writer_llm_endpoint: Option<String>,
    writer_llm_model: Option<String>,
    writer_llm_provider: Option<LlmProvider>,
    writer_api_key_env: String,
    writer_max_tokens: u32,
    writer_temperature: f64,
    force: bool,
}

#[derive(Debug, Serialize)]
struct RunSummary {
    benchmark: &'static str,
    runner: &'static str,
    generated_at_unix_seconds: u64,
    artifacts: String,
    endpoint: String,
    total_items: usize,
    ingested_items: usize,
    asked_items: usize,
    abstention_items: usize,
    evidence_items: usize,
    full_evidence_hits: usize,
    partial_evidence_hits: usize,
    missing_evidence_hits: usize,
    lexical_answer_items: usize,
    lexical_answer_hits: usize,
    value_plugins: Vec<&'static str>,
    derivation_plugins: Vec<&'static str>,
    plugin_configuration: EvidenceReaderPluginConfiguration,
    plugin_value_mentions: usize,
    plugin_derivation_results: usize,
    plugin_diagnostics: usize,
    smart_writer: LongMemEvalSmartWriterSummary,
    elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
struct ItemResult {
    question_id: String,
    question_type: String,
    about: String,
    abstention: bool,
    expected_answer: Value,
    expected_answer_turn_refs: Vec<String>,
    observed_refs: Vec<String>,
    missing_refs: Vec<String>,
    evidence_hit: EvidenceHit,
    ask_answer: Option<String>,
    ask_content: Value,
    plugin_reader: Value,
    plugin_value_mentions: usize,
    plugin_derivation_results: usize,
    plugin_diagnostics: usize,
    lexical_answer_hit: bool,
    ask_summary: String,
    ingest_elapsed_ms: u128,
    ask_elapsed_ms: u128,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum EvidenceHit {
    NotApplicable,
    Full,
    Partial,
    Missing,
}

#[derive(Debug, Clone)]
struct ToolCall {
    question_id: String,
    question_type: String,
    about: String,
    arguments: Value,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    ensure_output_dir(&args.output, args.force)?;

    let ingests = read_tool_calls(&args.artifacts.join("ingest.jsonl"), args.limit)?;
    let asks = read_tool_calls(&args.artifacts.join("ask.jsonl"), args.limit)?;
    let expected = read_expected(&args.artifacts.join("expected.jsonl"), args.limit)?;

    if ingests.len() != asks.len() || ingests.len() != expected.len() {
        return Err(format!(
            "artifact count mismatch: ingest={} ask={} expected={}",
            ingests.len(),
            asks.len(),
            expected.len()
        )
        .into());
    }

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
    let smart_writer = if args.smart_writer {
        Some(LongMemEvalSmartWriter::new(writer_config(&args)?)?)
    } else {
        None
    };

    let started = Instant::now();
    let mut item_results = Vec::new();
    let mut writer_results = Vec::<LongMemEvalSmartWriterResult>::new();
    let mut request_id = 1u64;
    let plugin_reader = ComposedEvidenceReader::kernel_default();

    let total_items = ingests.len();
    for (item_index, ((ingest, ask), expected)) in ingests
        .iter()
        .zip(asks.iter())
        .zip(expected.iter())
        .enumerate()
    {
        validate_matching_artifacts(ingest, ask, expected)?;
        eprintln!(
            "longmemeval runner item {}/{} question_id={} question_type={}",
            item_index + 1,
            total_items,
            ingest.question_id,
            ingest.question_type
        );

        let ingest_started = Instant::now();
        let ingest_response =
            call_mcp_tool(&server, request_id, "kernel_ingest", &ingest.arguments).await?;
        request_id = request_id.checked_add(1).ok_or("request id overflow")?;
        assert_tool_success(&ingest_response, "kernel_ingest", &ingest.question_id)?;
        let ingest_elapsed_ms = ingest_started.elapsed().as_millis();

        if let Some(writer) = smart_writer.as_ref() {
            writer_results.extend(
                writer
                    .write_ingest_item(
                        &server,
                        &mut request_id,
                        LongMemEvalSmartWriterItem {
                            item_index,
                            question_id: &ingest.question_id,
                            question_type: &ingest.question_type,
                            about: &ingest.about,
                            arguments: &ingest.arguments,
                        },
                    )
                    .await?,
            );
        }

        let ask_started = Instant::now();
        let ask_response = call_mcp_tool(&server, request_id, "kernel_ask", &ask.arguments).await?;
        request_id = request_id.checked_add(1).ok_or("request id overflow")?;
        let ask_content = assert_tool_success(&ask_response, "kernel_ask", &ask.question_id)?;
        let ask_elapsed_ms = ask_started.elapsed().as_millis();

        let observed_refs = collect_observed_evidence_refs(ask_content);
        let missing_refs = expected
            .answer_turn_refs
            .iter()
            .filter(|reference| !observed_refs.contains(*reference))
            .cloned()
            .collect::<Vec<_>>();
        let evidence_hit = classify_evidence_hit(expected, &observed_refs, &missing_refs);
        let ask_answer = ask_content
            .get("answer")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let lexical_answer_hit = answer_contains_expected(&expected.answer, ask_answer.as_deref());
        let ask_summary = ask_content
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let plugin_output = read_plugins_for_item(&plugin_reader, expected, ask_content)?;
        let plugin_reader_value = serde_json::to_value(&plugin_output)?;

        item_results.push(ItemResult {
            question_id: expected.question_id.clone(),
            question_type: expected.question_type.clone(),
            about: ingest.about.clone(),
            abstention: expected.abstention,
            expected_answer: expected.answer.clone(),
            expected_answer_turn_refs: expected.answer_turn_refs.clone(),
            observed_refs: observed_refs.into_iter().collect(),
            missing_refs,
            evidence_hit,
            ask_answer,
            ask_content: ask_content.clone(),
            plugin_reader: plugin_reader_value,
            plugin_value_mentions: plugin_output.values.len(),
            plugin_derivation_results: plugin_output.derivation_results.len(),
            plugin_diagnostics: plugin_output.diagnostics.len(),
            lexical_answer_hit,
            ask_summary,
            ingest_elapsed_ms,
            ask_elapsed_ms,
        });
        eprintln!(
            "longmemeval runner item {}/{} done question_id={} evidence_hit={:?} ingest_elapsed_ms={} ask_elapsed_ms={}",
            item_index + 1,
            total_items,
            expected.question_id,
            evidence_hit,
            ingest_elapsed_ms,
            ask_elapsed_ms
        );
    }

    let summary = summarize_run(
        &args,
        endpoint_label,
        started.elapsed().as_millis(),
        &item_results,
        &plugin_reader,
        &writer_results,
    )?;
    write_jsonl(
        &args.output.join("results.jsonl"),
        item_results.iter().map(serde_json::to_value),
    )?;
    write_jsonl(
        &args.output.join("hypotheses.jsonl"),
        item_results.iter().map(|item| {
            Ok(json!({
                "question_id": item.question_id,
                "hypothesis": item.ask_answer.as_deref().unwrap_or_default()
            }))
        }),
    )?;
    write_jsonl(
        &args.output.join("writer_results.jsonl"),
        writer_results.iter().map(serde_json::to_value),
    )?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
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
    question_id: &str,
) -> Result<&'a Value, Box<dyn Error + Send + Sync>> {
    let result = response
        .get("result")
        .ok_or_else(|| format!("{tool} for {question_id} returned no result"))?;
    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!("{tool} for {question_id} failed: {result}").into());
    }
    result
        .get("structuredContent")
        .ok_or_else(|| format!("{tool} for {question_id} returned no structuredContent").into())
}

fn collect_observed_evidence_refs(value: &Value) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    collect_supported_refs_from_proof_evidence(value, &mut refs);
    collect_support_relation_sources(value, &mut refs);
    collect_because_refs(value, &mut refs);
    refs
}

fn read_plugins_for_item(
    reader: &ComposedEvidenceReader,
    expected: &LongMemEvalExpected,
    ask_content: &Value,
) -> Result<EvidenceReaderOutput, Box<dyn Error + Send + Sync>> {
    let fragments = plugin_evidence_fragments(expected, ask_content);
    let request = EvidenceReaderRequest::new(EvidenceInterpretationInput::new(fragments));
    reader.read(&request).map_err(Into::into)
}

fn plugin_evidence_fragments(
    expected: &LongMemEvalExpected,
    ask_content: &Value,
) -> Vec<EvidenceFragment> {
    let mut fragments = Vec::new();
    fragments.push(EvidenceFragment::new(
        format!("question:{}", expected.question_id),
        expected.question.clone(),
    ));

    for item in ask_content
        .get("proof")
        .and_then(|proof| proof.get("evidence"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let Some(text) = item.get("text").and_then(Value::as_str) else {
            continue;
        };
        let source = item
            .get("source")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let supports = item
            .get("supports")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut pushed = false;
        for support in supports {
            let Some(ref_id) = support.as_str() else {
                continue;
            };
            if !looks_like_turn_ref(ref_id) {
                continue;
            }
            push_plugin_fragment(&mut fragments, ref_id, text, source.clone());
            pushed = true;
        }
        if !pushed {
            let ref_id = item
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| item.get("source").and_then(Value::as_str))
                .unwrap_or("proof:evidence");
            push_plugin_fragment(&mut fragments, ref_id, text, source);
        }
    }

    for relation in ask_content
        .get("proof")
        .and_then(|proof| proof.get("path"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let is_support = relation
            .get("rel")
            .and_then(Value::as_str)
            .is_some_and(|rel| rel == "supports_answer" || rel == "supports");
        if !is_support {
            continue;
        }
        let Some(ref_id) = relation.get("from").and_then(Value::as_str) else {
            continue;
        };
        if !looks_like_turn_ref(ref_id) {
            continue;
        }
        let Some(text) = relation.get("evidence").and_then(Value::as_str) else {
            continue;
        };
        push_plugin_fragment(&mut fragments, ref_id, text, None);
    }

    for reason in ask_content
        .get("because")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let Some(ref_id) = reason.get("ref").and_then(Value::as_str) else {
            continue;
        };
        if !looks_like_turn_ref(ref_id) {
            continue;
        }
        let Some(text) = reason.get("evidence").and_then(Value::as_str) else {
            continue;
        };
        push_plugin_fragment(&mut fragments, ref_id, text, None);
    }

    dedupe_plugin_fragments(fragments)
}

fn push_plugin_fragment(
    fragments: &mut Vec<EvidenceFragment>,
    ref_id: &str,
    text: &str,
    source: Option<String>,
) {
    if text.trim().is_empty() {
        return;
    }
    let mut fragment = EvidenceFragment::new(ref_id.to_string(), text.trim().to_string());
    fragment.source = source;
    fragments.push(fragment);
}

fn dedupe_plugin_fragments(fragments: Vec<EvidenceFragment>) -> Vec<EvidenceFragment> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for fragment in fragments {
        if seen.insert((fragment.ref_id.clone(), fragment.text.clone())) {
            deduped.push(fragment);
        }
    }
    deduped
}

fn collect_supported_refs_from_proof_evidence(value: &Value, refs: &mut BTreeSet<String>) {
    let evidence = value
        .get("proof")
        .and_then(|proof| proof.get("evidence"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for item in evidence {
        let supports = item
            .get("supports")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for support in supports {
            if let Some(reference) = support.as_str()
                && looks_like_turn_ref(reference)
            {
                refs.insert(reference.to_string());
            }
        }
    }
}

fn collect_support_relation_sources(value: &Value, refs: &mut BTreeSet<String>) {
    let path = value
        .get("proof")
        .and_then(|proof| proof.get("path"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for relation in path {
        let is_support = relation
            .get("rel")
            .and_then(Value::as_str)
            .is_some_and(|rel| rel == "supports_answer" || rel == "supports");
        if !is_support {
            continue;
        }
        if let Some(reference) = relation.get("from").and_then(Value::as_str)
            && looks_like_turn_ref(reference)
        {
            refs.insert(reference.to_string());
        }
    }
}

fn collect_because_refs(value: &Value, refs: &mut BTreeSet<String>) {
    let reasons = value
        .get("because")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for reason in reasons {
        if let Some(reference) = reason.get("ref").and_then(Value::as_str)
            && looks_like_turn_ref(reference)
        {
            refs.insert(reference.to_string());
        }
    }
}

fn looks_like_turn_ref(value: &str) -> bool {
    value.starts_with("turn:") && !value.contains(' ') && value.len() <= 240
}

fn classify_evidence_hit(
    expected: &LongMemEvalExpected,
    observed_refs: &BTreeSet<String>,
    missing_refs: &[String],
) -> EvidenceHit {
    if expected.abstention || expected.answer_turn_refs.is_empty() {
        return EvidenceHit::NotApplicable;
    }
    if missing_refs.is_empty() {
        EvidenceHit::Full
    } else if expected
        .answer_turn_refs
        .iter()
        .any(|reference| observed_refs.contains(reference))
    {
        EvidenceHit::Partial
    } else {
        EvidenceHit::Missing
    }
}

fn answer_contains_expected(expected_answer: &Value, ask_answer: Option<&str>) -> bool {
    let Some(ask_answer) = ask_answer else {
        return false;
    };
    let expected = normalize_answer_text(expected_answer);
    let expected = normalize_for_lexical_match(&expected);
    let observed = normalize_for_lexical_match(ask_answer);
    !expected.is_empty() && observed.contains(&expected)
}

fn normalize_answer_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(normalize_answer_text)
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => value.to_string(),
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

fn validate_matching_artifacts(
    ingest: &ToolCall,
    ask: &ToolCall,
    expected: &LongMemEvalExpected,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if ingest.question_id != ask.question_id || ingest.question_id != expected.question_id {
        return Err(format!(
            "question_id mismatch: ingest={} ask={} expected={}",
            ingest.question_id, ask.question_id, expected.question_id
        )
        .into());
    }
    if ingest.question_type != ask.question_type || ingest.question_type != expected.question_type {
        return Err(format!(
            "question_type mismatch for {}: ingest={} ask={} expected={}",
            ingest.question_id, ingest.question_type, ask.question_type, expected.question_type
        )
        .into());
    }
    if ingest.about != ask.about {
        return Err(format!(
            "about mismatch for {}: ingest={} ask={}",
            ingest.question_id, ingest.about, ask.about
        )
        .into());
    }
    Ok(())
}

fn summarize_run(
    args: &Args,
    endpoint: String,
    elapsed_ms: u128,
    results: &[ItemResult],
    plugin_reader: &ComposedEvidenceReader,
    writer_results: &[LongMemEvalSmartWriterResult],
) -> Result<RunSummary, Box<dyn Error + Send + Sync>> {
    let mut counts = BTreeMap::new();
    for result in results {
        *counts.entry(result.evidence_hit).or_insert(0usize) += 1;
    }
    Ok(RunSummary {
        benchmark: "LongMemEval",
        runner: "longmemeval-kmp-runner-v1",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        artifacts: args.artifacts.display().to_string(),
        endpoint,
        total_items: results.len(),
        ingested_items: results.len(),
        asked_items: results.len(),
        abstention_items: results.iter().filter(|result| result.abstention).count(),
        evidence_items: results
            .iter()
            .filter(|result| result.evidence_hit != EvidenceHit::NotApplicable)
            .count(),
        full_evidence_hits: *counts.get(&EvidenceHit::Full).unwrap_or(&0),
        partial_evidence_hits: *counts.get(&EvidenceHit::Partial).unwrap_or(&0),
        missing_evidence_hits: *counts.get(&EvidenceHit::Missing).unwrap_or(&0),
        lexical_answer_items: results
            .iter()
            .filter(|result| {
                !result.abstention && !normalize_answer_text(&result.expected_answer).is_empty()
            })
            .count(),
        lexical_answer_hits: results
            .iter()
            .filter(|result| result.lexical_answer_hit)
            .count(),
        value_plugins: plugin_reader.value_plugin_ids(),
        derivation_plugins: plugin_reader.derivation_plugin_ids(),
        plugin_configuration: plugin_reader.configuration(),
        plugin_value_mentions: results
            .iter()
            .map(|result| result.plugin_value_mentions)
            .sum(),
        plugin_derivation_results: results
            .iter()
            .map(|result| result.plugin_derivation_results)
            .sum(),
        plugin_diagnostics: results.iter().map(|result| result.plugin_diagnostics).sum(),
        smart_writer: summarize_longmemeval_smart_writer(args.smart_writer, writer_results),
        elapsed_ms,
    })
}

fn read_tool_calls(
    path: &Path,
    limit: Option<usize>,
) -> Result<Vec<ToolCall>, Box<dyn Error + Send + Sync>> {
    read_jsonl(path, limit)?
        .into_iter()
        .map(|value| {
            let question_id = required_string(&value, "question_id")?;
            let question_type = required_string(&value, "question_type")?;
            let about = required_string(&value, "about")?;
            let arguments = value
                .get("arguments")
                .cloned()
                .ok_or_else(|| format!("{} missing arguments", path.display()))?;
            Ok(ToolCall {
                question_id,
                question_type,
                about,
                arguments,
            })
        })
        .collect()
}

fn read_expected(
    path: &Path,
    limit: Option<usize>,
) -> Result<Vec<LongMemEvalExpected>, Box<dyn Error + Send + Sync>> {
    read_jsonl(path, limit)?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
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
    let mut limit = None;
    let mut smart_writer = false;
    let mut log_mcp_navigation = env_flag("LONGMEMEVAL_LOG_MCP_NAVIGATION");
    let mut writer_llm_endpoint = None;
    let mut writer_llm_model = None;
    let mut writer_llm_provider = None;
    let mut writer_api_key_env = "LLM_API_KEY".to_string();
    let mut writer_max_tokens = 384u32;
    let mut writer_temperature = 0.0f64;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--artifacts" => artifacts = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--endpoint" => endpoint = Some(required_flag_value(&mut args, &arg)?),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--limit" => {
                let value = required_flag_value(&mut args, &arg)?;
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|error| format!("invalid --limit value `{value}`: {error}"))?,
                );
            }
            "--smart-writer" => smart_writer = true,
            "--log-mcp-navigation" => log_mcp_navigation = true,
            "--writer-llm-endpoint" => {
                writer_llm_endpoint = Some(required_flag_value(&mut args, &arg)?)
            }
            "--writer-llm-model" => writer_llm_model = Some(required_flag_value(&mut args, &arg)?),
            "--writer-llm-provider" => {
                writer_llm_provider = Some(parse_writer_provider(&required_flag_value(
                    &mut args, &arg,
                )?)?)
            }
            "--writer-api-key-env" => writer_api_key_env = required_flag_value(&mut args, &arg)?,
            "--writer-max-tokens" => {
                let value = required_flag_value(&mut args, &arg)?;
                writer_max_tokens = value.parse::<u32>().map_err(|error| {
                    format!("invalid --writer-max-tokens value `{value}`: {error}")
                })?;
                if writer_max_tokens == 0 {
                    return Err("--writer-max-tokens must be greater than zero".into());
                }
            }
            "--writer-temperature" => {
                let value = required_flag_value(&mut args, &arg)?;
                writer_temperature = value.parse::<f64>().map_err(|error| {
                    format!("invalid --writer-temperature value `{value}`: {error}")
                })?;
                if !writer_temperature.is_finite() || writer_temperature < 0.0 {
                    return Err("--writer-temperature must be a finite non-negative value".into());
                }
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
        limit,
        smart_writer,
        log_mcp_navigation,
        writer_llm_endpoint,
        writer_llm_model,
        writer_llm_provider,
        writer_api_key_env,
        writer_max_tokens,
        writer_temperature,
        force,
    })
}

fn writer_config(
    args: &Args,
) -> Result<LongMemEvalSmartWriterConfig, Box<dyn Error + Send + Sync>> {
    let llm_endpoint = args
        .writer_llm_endpoint
        .clone()
        .or_else(|| non_empty_env("WRITER_LLM_ENDPOINT"))
        .or_else(|| non_empty_env("LLM_ENDPOINT"));
    let llm_model = args
        .writer_llm_model
        .clone()
        .or_else(|| non_empty_env("WRITER_LLM_MODEL"))
        .or_else(|| non_empty_env("LLM_MODEL"));
    let llm_provider = args
        .writer_llm_provider
        .or_else(|| {
            non_empty_env("WRITER_LLM_PROVIDER")
                .or_else(|| non_empty_env("LLM_PROVIDER"))
                .and_then(|value| parse_writer_provider(&value).ok())
        })
        .or_else(|| llm_model.as_deref().map(detect_writer_provider_from_model));
    let api_key = env::var(&args.writer_api_key_env)
        .ok()
        .filter(|value| !value.trim().is_empty());

    Ok(LongMemEvalSmartWriterConfig {
        llm_endpoint,
        llm_model,
        llm_provider,
        api_key,
        max_tokens: args.writer_max_tokens,
        temperature: args.writer_temperature,
        log_mcp_navigation: args.log_mcp_navigation,
    })
}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}

fn env_flag(key: &str) -> bool {
    env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn detect_writer_provider_from_model(model: &str) -> LlmProvider {
    if model.starts_with("gpt-5")
        || model.starts_with("o3")
        || model.starts_with("o4")
        || model.starts_with("gpt-4.1")
    {
        LlmProvider::OpenAINew
    } else {
        LlmProvider::OpenAI
    }
}

fn parse_writer_provider(value: &str) -> Result<LlmProvider, Box<dyn Error + Send + Sync>> {
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

fn required_flag_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn print_usage() {
    eprintln!(
        "Usage: longmemeval_kmp_runner --artifacts <adapter-output-dir> --output <run-dir> [--endpoint http://host] [--limit N] [--smart-writer] [--log-mcp-navigation] [--writer-llm-endpoint URL] [--writer-llm-model MODEL] [--writer-llm-provider openai|openai-new|anthropic] [--writer-api-key-env LLM_API_KEY] [--writer-max-tokens N] [--writer-temperature F] [--force]"
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn observed_refs_only_use_supported_evidence_not_incidental_context_refs() {
        let observed = collect_observed_evidence_refs(&json!({
            "answer": "context mentions turn:q:s1:1 and turn:q:s1:2",
            "because": [
                {
                    "claim": "direct",
                    "evidence": "selected",
                    "ref": "turn:q:s1:1"
                }
            ],
            "proof": {
                "path": [
                    {
                        "from": "turn:q:s1:2",
                        "to": "question:q",
                        "rel": "mentions",
                        "class": "semantic"
                    },
                    {
                        "from": "turn:q:s1:3",
                        "to": "question:q",
                        "rel": "supports_answer",
                        "class": "evidential"
                    }
                ],
                "evidence": [
                    {
                        "id": "evidence:q:s1:4",
                        "supports": ["turn:q:s1:4"],
                        "text": "selected evidence"
                    }
                ]
            }
        }));

        assert_eq!(
            observed,
            BTreeSet::from([
                "turn:q:s1:1".to_string(),
                "turn:q:s1:3".to_string(),
                "turn:q:s1:4".to_string()
            ])
        );
    }

    #[test]
    fn plugin_reader_interprets_recovered_evidence() {
        let expected = LongMemEvalExpected {
            question_id: "q-money".to_string(),
            question_type: "multi-session".to_string(),
            answer: json!("12"),
            question: "How much did I spend?".to_string(),
            question_date: "2026-05-08".to_string(),
            answer_session_ids: vec!["s1".to_string()],
            answer_turn_refs: vec!["turn:q-money:s1:1".to_string()],
            answer_session_refs: vec!["session:q-money:s1".to_string()],
            abstention: false,
        };
        let ask_content = json!({
            "proof": {
                "evidence": [
                    {
                        "supports": ["turn:q-money:s1:1"],
                        "text": "I paid $12 for lunch.",
                        "source": "session:s1"
                    }
                ]
            }
        });
        let reader = ComposedEvidenceReader::kernel_default();

        let output = read_plugins_for_item(&reader, &expected, &ask_content)
            .expect("plugin reader should parse recovered evidence");

        assert!(
            output
                .values
                .iter()
                .any(|mention| mention.plugin == "money-value-v1")
        );
        assert_eq!(output.value_plugin_ids, reader.value_plugin_ids());
    }
}
