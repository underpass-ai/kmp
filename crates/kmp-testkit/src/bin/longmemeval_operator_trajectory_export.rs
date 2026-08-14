use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Map, Value, json};

use kmp_testkit::{kernel_operator_allowed_read_tools, kernel_operator_is_bounded_tool_call};

const SCHEMA_VERSION: &str = "kernel-operator-trajectory-v1";
const EXPORTER: &str = "longmemeval-operator-trajectory-export-v1";
const DEFAULT_CONTEXT_CHARS: usize = 12_000;
const DEFAULT_TOOL_CALL_BUDGET: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    run: PathBuf,
    artifacts: Option<PathBuf>,
    output: PathBuf,
    expected_run_id: Option<String>,
    include_writer_reads: bool,
    force: bool,
}

#[derive(Debug, Clone)]
struct AskArtifact {
    about: String,
    question_type: String,
    arguments: Value,
}

#[derive(Debug, Serialize)]
struct TrajectoryItem {
    schema_version: &'static str,
    run_id: String,
    task_family: String,
    mode: String,
    source: String,
    about: String,
    step_id: String,
    step_index: usize,
    goal: String,
    visible_state: Value,
    allowed_tools: Vec<String>,
    target_action: Value,
    observed_outcome: Option<Value>,
    quality: Value,
}

#[derive(Debug, Serialize)]
struct ExportSummary {
    exporter: &'static str,
    schema_version: &'static str,
    generated_at_unix_seconds: u64,
    source_run: String,
    source_artifacts: String,
    output: String,
    run_id: String,
    task_families: BTreeMap<String, usize>,
    modes: BTreeMap<String, usize>,
    target_actions: BTreeMap<String, usize>,
    trajectories: usize,
    tool_call_trajectories: usize,
    stop_trajectories: usize,
    writer_read_trajectories: usize,
    failure_rows: usize,
    bounded_failures: usize,
    redaction_findings: usize,
}

#[derive(Debug, Serialize)]
struct FailureRow {
    source: String,
    location: String,
    reason: String,
    detail: Value,
}

#[derive(Debug, Serialize)]
struct RedactionReport {
    checked_values: usize,
    findings: Vec<RedactionFinding>,
}

#[derive(Debug, Clone, Serialize)]
struct RedactionFinding {
    path: String,
    reason: String,
}

#[derive(Debug, Default)]
struct ExportData {
    trajectories: Vec<TrajectoryItem>,
    failures: Vec<FailureRow>,
    redaction: RedactionReportBuilder,
    run_ids: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
struct CandidateDraft {
    reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LongMemEvalTurnRef {
    question_id: String,
    session_ref: String,
    sequence: i64,
}

#[derive(Debug, Default)]
struct RedactionReportBuilder {
    checked_values: usize,
    findings: Vec<RedactionFinding>,
}

impl RedactionReportBuilder {
    fn scan(&mut self, value: &Value) {
        scan_redaction(value, "$", self);
    }

    fn finish(self) -> RedactionReport {
        RedactionReport {
            checked_values: self.checked_values,
            findings: self.findings,
        }
    }
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    ensure_output_dir(&args.output, args.force)?;

    let artifacts = resolve_artifacts_dir(&args)?;
    let ask_artifacts = load_ask_artifacts(&artifacts.join("ask.jsonl"))?;

    let mut data = ExportData::default();
    export_results(&args.run.join("results.jsonl"), &ask_artifacts, &mut data)?;
    if args.include_writer_reads {
        export_writer_reads(&args.run.join("writer_results.jsonl"), &mut data)?;
    }

    let run_id = resolve_run_id(&data.run_ids, args.expected_run_id.as_deref())?;
    for item in &mut data.trajectories {
        item.run_id = run_id.clone();
        item.step_id = namespaced_step_id(&run_id, &item.step_id);
    }
    ensure_unique_step_ids(&data.trajectories)?;

    let bounded_failures = data
        .failures
        .iter()
        .filter(|failure| failure.reason == "unbounded_tool_call")
        .count();
    if bounded_failures > 0 {
        write_outputs(&args, &artifacts, &run_id, data, bounded_failures)?;
        return Err(format!("refusing to export {bounded_failures} unbounded tool calls").into());
    }

    let redaction_findings = data.redaction.findings.len();
    if redaction_findings > 0 {
        write_outputs(&args, &artifacts, &run_id, data, bounded_failures)?;
        return Err(format!("refusing to export {redaction_findings} redaction findings").into());
    }

    write_outputs(&args, &artifacts, &run_id, data, bounded_failures)?;
    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut run = None;
    let mut artifacts = None;
    let mut output = None;
    let mut expected_run_id = None;
    let mut include_writer_reads = false;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--run" => run = Some(PathBuf::from(next_arg(&mut args, "--run")?)),
            "--artifacts" => artifacts = Some(PathBuf::from(next_arg(&mut args, "--artifacts")?)),
            "--output" => output = Some(PathBuf::from(next_arg(&mut args, "--output")?)),
            "--expected-run-id" => {
                expected_run_id = Some(next_arg(&mut args, "--expected-run-id")?);
            }
            "--include-writer-reads" => include_writer_reads = true,
            "--force" => force = true,
            "--help" | "-h" => return Err(usage().into()),
            value if value.starts_with('-') => {
                return Err(format!("unknown argument: {value}\n{}", usage()).into());
            }
            value => {
                if run.is_some() {
                    return Err(format!("unexpected positional argument: {value}").into());
                }
                run = Some(PathBuf::from(value));
            }
        }
    }

    Ok(Args {
        run: run.ok_or_else(usage)?,
        artifacts,
        output: output.ok_or("--output is required")?,
        expected_run_id,
        include_writer_reads,
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
    "usage: longmemeval_operator_trajectory_export --run <longmemeval-run-dir> --artifacts <longmemeval-artifacts-dir> --output <dir> [--expected-run-id id] [--include-writer-reads] [--force]".to_string()
}

fn namespaced_step_id(run_id: &str, step_id: &str) -> String {
    if step_id.starts_with("longmemeval:run:") {
        return step_id.to_string();
    }
    let suffix = step_id.strip_prefix("longmemeval:").unwrap_or(step_id);
    format!("longmemeval:run:{run_id}:{suffix}")
}

fn ensure_unique_step_ids(
    trajectories: &[TrajectoryItem],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut seen = BTreeSet::<&str>::new();
    for item in trajectories {
        if !seen.insert(item.step_id.as_str()) {
            return Err(format!(
                "duplicate LongMemEval operator trajectory step_id `{}`; step ids must be unique",
                item.step_id
            )
            .into());
        }
    }
    Ok(())
}

fn ensure_output_dir(path: &Path, force: bool) -> Result<(), Box<dyn Error + Send + Sync>> {
    if path.exists() {
        if !force {
            return Err(format!(
                "output directory already exists: {}; pass --force to replace generated files",
                path.display()
            )
            .into());
        }
    } else {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn resolve_artifacts_dir(args: &Args) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    if let Some(path) = args.artifacts.as_ref() {
        return Ok(path.clone());
    }

    let summary_path = args.run.join("summary.json");
    if summary_path.exists() {
        let summary: Value = serde_json::from_reader(File::open(&summary_path)?)?;
        if let Some(path) = summary.get("artifacts").and_then(Value::as_str) {
            let candidate = PathBuf::from(path);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    let run_name = args
        .run
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("run path must have a final directory name")?;
    let artifacts_name = run_name.replace("-run", "-artifacts");
    let sibling = args.run.with_file_name(artifacts_name);
    if sibling.exists() {
        return Ok(sibling);
    }

    Err(format!(
        "could not resolve LongMemEval artifacts directory for {}; pass --artifacts explicitly",
        args.run.display()
    )
    .into())
}

fn load_ask_artifacts(
    path: &Path,
) -> Result<BTreeMap<String, AskArtifact>, Box<dyn Error + Send + Sync>> {
    let mut artifacts = BTreeMap::new();
    for (line_index, value) in read_jsonl(path)?.into_iter().enumerate() {
        let location = format!("{}:{}", path.display(), line_index + 1);
        let question_id = required_string(&value, "question_id", &location)?.to_string();
        let about = required_string(&value, "about", &location)?.to_string();
        let question_type = required_string(&value, "question_type", &location)?.to_string();
        let arguments = value
            .get("arguments")
            .cloned()
            .ok_or_else(|| format!("{location} missing required field `arguments`"))?;
        artifacts.insert(
            question_id,
            AskArtifact {
                about,
                question_type,
                arguments,
            },
        );
    }
    Ok(artifacts)
}

fn export_results(
    path: &Path,
    ask_artifacts: &BTreeMap<String, AskArtifact>,
    data: &mut ExportData,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (line_index, value) in read_jsonl(path)?.into_iter().enumerate() {
        collect_run_ids(&value, &mut data.run_ids);
        let location = format!("{}:{}", path.display(), line_index + 1);
        let question_id = required_string(&value, "question_id", &location)?;
        let question_type = required_string(&value, "question_type", &location)?;
        let about = required_string(&value, "about", &location)?;
        let Some(ask) = ask_artifacts.get(question_id) else {
            data.failures.push(FailureRow {
                source: "longmemeval.results".to_string(),
                location,
                reason: "missing_ask_artifact".to_string(),
                detail: json!({ "question_id": question_id }),
            });
            continue;
        };
        if ask.about != about {
            data.failures.push(FailureRow {
                source: "longmemeval.results".to_string(),
                location: location.clone(),
                reason: "ask_artifact_about_mismatch".to_string(),
                detail: json!({
                    "question_id": question_id,
                    "result_about": about,
                    "ask_about": ask.about,
                }),
            });
            continue;
        }
        if ask.question_type != question_type {
            data.failures.push(FailureRow {
                source: "longmemeval.results".to_string(),
                location: location.clone(),
                reason: "ask_artifact_question_type_mismatch".to_string(),
                detail: json!({
                    "question_id": question_id,
                    "result_question_type": question_type,
                    "ask_question_type": ask.question_type,
                }),
            });
            continue;
        }

        let observed_refs = string_array(&value, "observed_refs");
        let evidence_hit = value
            .get("evidence_hit")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let missing_ref_count = value
            .get("missing_refs")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let lexical_answer_hit = bool_field(&value, "lexical_answer_hit");
        let abstention = bool_field(&value, "abstention").unwrap_or(false);
        let bounded = kernel_operator_is_bounded_tool_call("kernel_ask", &ask.arguments);
        if !bounded {
            data.failures.push(FailureRow {
                source: "longmemeval.results".to_string(),
                location: location.clone(),
                reason: "unbounded_tool_call".to_string(),
                detail: json!({
                    "tool": "kernel_ask",
                    "arguments": ask.arguments,
                }),
            });
        }

        let question_ref = longmemeval_question_ref(about, question_id);
        let question_text = ask
            .arguments
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let task_family = format!("longmemeval.{question_type}");
        let allowed_tools = kernel_operator_allowed_read_tools();

        let ask_item = TrajectoryItem {
            schema_version: SCHEMA_VERSION,
            run_id: String::new(),
            task_family: task_family.clone(),
            mode: "read".to_string(),
            source: "longmemeval.results.kernel_ask".to_string(),
            about: about.to_string(),
            step_id: format!("longmemeval:{question_type}:{question_id}:read:0"),
            step_index: 0,
            goal: "Choose the bounded KMP read move for the current LongMemEval memory question."
                .to_string(),
            visible_state: json!({
                "current_ref": question_ref,
                "known_refs": [],
                "last_tool": null,
                "last_observed_refs": [],
                "last_result_page": null,
                "last_result_partial": null,
                "remaining_budget": {
                    "tool_calls": 1,
                    "context_chars": DEFAULT_CONTEXT_CHARS,
                },
                "task": {
                    "benchmark": "LongMemEval",
                    "task_type": question_type,
                    "question_id": question_id,
                    "question": question_text,
                    "abstention": abstention,
                }
            }),
            allowed_tools: allowed_tools.clone(),
            target_action: json!({
                "type": "tool_call",
                "tool": "kernel_ask",
                "arguments": ask.arguments,
            }),
            observed_outcome: Some(json!({
                "success": evidence_hit != "missing",
                "observed_refs": observed_refs,
                "elapsed_ms": value.get("ask_elapsed_ms").and_then(Value::as_u64),
                "observed_ref_count": observed_refs.len(),
                "evidence_hit": evidence_hit,
            })),
            quality: json!({
                "evidence_hit": evidence_hit,
                "invalid_tool_call": false,
                "bounded": bounded,
                "stop_correct": null,
                "future_answer_leak": false,
                "missing_ref_count": missing_ref_count,
                "observed_ref_count": observed_refs.len(),
                "lexical_answer_hit": lexical_answer_hit,
            }),
        };
        data.redaction.scan(&serde_json::to_value(&ask_item)?);
        data.trajectories.push(ask_item);

        let stop_correct = evidence_hit != "missing";
        let stop_item = TrajectoryItem {
            schema_version: SCHEMA_VERSION,
            run_id: String::new(),
            task_family,
            mode: "read".to_string(),
            source: "longmemeval.results.kernel_ask".to_string(),
            about: about.to_string(),
            step_id: format!("longmemeval:{question_type}:{question_id}:stop"),
            step_index: 1,
            goal: "Stop when deterministic KMP evidence has been retrieved for the LongMemEval question."
                .to_string(),
            visible_state: json!({
                "current_ref": question_ref,
                "known_refs": observed_refs,
                "last_tool": "kernel_ask",
                "last_observed_refs": observed_refs,
                "last_result_page": null,
                "last_result_partial": null,
                "remaining_budget": {
                    "tool_calls": 0,
                    "context_chars": DEFAULT_CONTEXT_CHARS,
                },
                "task": {
                    "benchmark": "LongMemEval",
                    "task_type": question_type,
                    "question_id": question_id,
                    "question": question_text,
                    "abstention": abstention,
                }
            }),
            allowed_tools,
            target_action: json!({
                "type": "stop",
                "answer_policy": "evidence_or_unknown",
                "final_refs": observed_refs,
                "reason": if stop_correct { "sufficient_evidence" } else { "incomplete_evidence" },
            }),
            observed_outcome: Some(json!({
                "success": stop_correct,
                "observed_refs": observed_refs,
                "evidence_hit": evidence_hit,
            })),
            quality: json!({
                "evidence_hit": evidence_hit,
                "invalid_tool_call": false,
                "bounded": true,
                "stop_correct": stop_correct,
                "future_answer_leak": false,
                "missing_ref_count": missing_ref_count,
                "observed_ref_count": observed_refs.len(),
                "lexical_answer_hit": lexical_answer_hit,
            }),
        };
        data.redaction.scan(&serde_json::to_value(&stop_item)?);
        data.trajectories.push(stop_item);
    }
    Ok(())
}

fn export_writer_reads(
    path: &Path,
    data: &mut ExportData,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (line_index, value) in read_jsonl(path)?.into_iter().enumerate() {
        collect_run_ids(&value, &mut data.run_ids);
        let location = format!("{}:{}", path.display(), line_index + 1);
        let about = required_string(&value, "about", &location)?;
        let entry_ref = required_string(&value, "entry_ref", &location)?;
        let calls = value
            .get("pre_read_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let target_ref = value
            .get("target_ref")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut known_refs = BTreeSet::<String>::new();
        let mut last_tool: Option<String> = None;
        let mut last_observed_refs = Vec::<String>::new();
        let mut last_page: Option<Value> = None;
        let mut last_partial_result: Option<bool> = None;
        let allowed_tools = kernel_operator_allowed_read_tools();
        let candidate_ref_details =
            writer_candidate_ref_details(&value, &calls, entry_ref, target_ref);
        let candidate_refs = candidate_ref_details
            .iter()
            .filter_map(|detail| detail.get("ref").and_then(Value::as_str))
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        for (call_index, call) in calls.iter().enumerate() {
            let Some(tool) = call.get("tool").and_then(Value::as_str) else {
                data.failures.push(FailureRow {
                    source: "longmemeval.writer_results".to_string(),
                    location: format!("{location}:pre_read_calls[{call_index}]"),
                    reason: "missing_tool".to_string(),
                    detail: json!({}),
                });
                continue;
            };
            let arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let observed_refs = string_array(call, "observed_refs");
            let current_page = call_page(call);
            let current_partial_result = call_partial_result(call);
            let bounded = kernel_operator_is_bounded_tool_call(tool, &arguments);
            if !bounded {
                data.failures.push(FailureRow {
                    source: "longmemeval.writer_results".to_string(),
                    location: format!("{location}:pre_read_calls[{call_index}]"),
                    reason: "unbounded_tool_call".to_string(),
                    detail: json!({
                        "tool": tool,
                        "arguments": arguments,
                    }),
                });
            }

            let item = TrajectoryItem {
                schema_version: SCHEMA_VERSION,
                run_id: String::new(),
                task_family: "longmemeval.smart_writer".to_string(),
                mode: "write_context_read".to_string(),
                source: "longmemeval.writer_results.pre_read_calls".to_string(),
                about: about.to_string(),
                step_id: format!("longmemeval:writer:{entry_ref}:read:{call_index}"),
                step_index: call_index,
                goal:
                    "Choose the next bounded KMP read move needed before writing a LongMemEval memory relation."
                        .to_string(),
                visible_state: json!({
                    "current_ref": entry_ref,
                    "candidate_refs": candidate_refs,
                    "candidate_ref_details": candidate_ref_details,
                    "known_refs": known_refs.iter().cloned().collect::<Vec<_>>(),
                    "last_tool": last_tool,
                    "last_observed_refs": last_observed_refs,
                    "last_result_page": last_page.clone(),
                    "last_result_partial": last_partial_result,
                    "remaining_budget": {
                        "tool_calls": calls.len().saturating_sub(call_index).min(DEFAULT_TOOL_CALL_BUDGET),
                        "context_chars": DEFAULT_CONTEXT_CHARS,
                    }
                }),
                allowed_tools: allowed_tools.clone(),
                target_action: json!({
                    "type": "tool_call",
                    "tool": tool,
                    "arguments": arguments,
                }),
                observed_outcome: Some(json!({
                    "success": true,
                    "observed_refs": observed_refs,
                    "elapsed_ms": call.get("elapsed_ms").and_then(Value::as_u64),
                    "observed_ref_count": observed_refs.len(),
                    "partial_result": current_partial_result,
                    "page": current_page.clone(),
                })),
                quality: json!({
                    "known_at_clean": null,
                    "future_answer_leak": false,
                    "invalid_tool_call": false,
                    "bounded": bounded,
                    "stop_correct": null,
                    "candidate_ref_count": candidate_refs.len(),
                    "candidate_ref_detail_count": candidate_ref_details.len(),
                }),
            };
            data.redaction.scan(&serde_json::to_value(&item)?);
            data.trajectories.push(item);
            known_refs.extend(observed_refs.iter().cloned());
            last_tool = Some(tool.to_string());
            last_observed_refs = observed_refs;
            last_page = current_page;
            last_partial_result = current_partial_result;
        }
    }
    Ok(())
}

fn call_page(call: &Value) -> Option<Value> {
    call.get("page").filter(|page| page.is_object()).cloned()
}

fn call_partial_result(call: &Value) -> Option<bool> {
    call.get("partial_result")
        .and_then(Value::as_bool)
        .or_else(|| call_page(call).and_then(|page| page.get("has_more").and_then(Value::as_bool)))
}

fn writer_candidate_ref_details(
    value: &Value,
    calls: &[Value],
    entry_ref: &str,
    target_ref: &str,
) -> Vec<Value> {
    let mut candidates = BTreeMap::<String, CandidateDraft>::new();
    if !target_ref.is_empty() && target_ref != entry_ref {
        insert_candidate(&mut candidates, target_ref);
    }
    collect_candidate_array(
        value.pointer("/write_request/read_context/inspected_refs"),
        entry_ref,
        &mut candidates,
    );
    collect_candidate_array(
        value.pointer("/write_request/read_context/temporal_refs"),
        entry_ref,
        &mut candidates,
    );
    if let Some(connect_to) = value
        .pointer("/write_request/connect_to")
        .and_then(Value::as_array)
    {
        for relation in connect_to {
            if let Some(reference) = relation.get("ref").and_then(Value::as_str)
                && reference != entry_ref
            {
                insert_candidate(&mut candidates, reference);
            }
        }
    }
    if let Some(relation_quality) = value.get("relation_quality").and_then(Value::as_array) {
        for relation in relation_quality {
            if let Some(reference) = relation.get("to").and_then(Value::as_str)
                && reference != entry_ref
            {
                insert_candidate(&mut candidates, reference);
            }
        }
    }
    for call in calls {
        if let Some(arguments) = call.get("arguments") {
            collect_action_primary_candidates(arguments, entry_ref, &mut candidates);
        }
    }

    let mut details = candidates
        .values()
        .map(|candidate| writer_candidate_detail(candidate, entry_ref, target_ref))
        .collect::<Vec<_>>();
    details.sort_by(|left, right| {
        let left_priority = left
            .get("priority")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        let right_priority = right
            .get("priority")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        let left_ref = left.get("ref").and_then(Value::as_str).unwrap_or_default();
        let right_ref = right.get("ref").and_then(Value::as_str).unwrap_or_default();
        left_priority
            .cmp(&right_priority)
            .then_with(|| left_ref.cmp(right_ref))
    });
    details
}

fn collect_candidate_array(
    value: Option<&Value>,
    entry_ref: &str,
    candidates: &mut BTreeMap<String, CandidateDraft>,
) {
    if let Some(values) = value.and_then(Value::as_array) {
        for item in values {
            if let Some(reference) = item.as_str()
                && reference != entry_ref
            {
                insert_candidate(candidates, reference);
            }
        }
    }
}

fn collect_action_primary_candidates(
    arguments: &Value,
    entry_ref: &str,
    candidates: &mut BTreeMap<String, CandidateDraft>,
) {
    for key in ["ref", "from", "to"] {
        if let Some(reference) = arguments.get(key).and_then(Value::as_str)
            && reference != entry_ref
        {
            insert_candidate(candidates, reference);
        }
    }
    if let Some(reference) = arguments
        .get("around")
        .and_then(|around| around.get("ref"))
        .and_then(Value::as_str)
        && reference != entry_ref
    {
        insert_candidate(candidates, reference);
    }
}

fn insert_candidate(candidates: &mut BTreeMap<String, CandidateDraft>, reference: &str) {
    candidates
        .entry(reference.to_string())
        .or_insert_with(|| CandidateDraft {
            reference: reference.to_string(),
        });
}

fn writer_candidate_detail(candidate: &CandidateDraft, entry_ref: &str, target_ref: &str) -> Value {
    let role = writer_candidate_role(entry_ref, &candidate.reference, target_ref);
    let priority = writer_candidate_priority(&role);
    json!({
        "ref": candidate.reference,
        "role": role,
        "priority": priority,
        "relation_hint": writer_candidate_relation_hint(&role),
    })
}

fn writer_candidate_role(entry_ref: &str, candidate_ref: &str, target_ref: &str) -> String {
    if !target_ref.is_empty() && candidate_ref == target_ref {
        return "target_question".to_string();
    }
    if candidate_ref.contains(":dimension:") {
        return "dimension_scope".to_string();
    }
    if candidate_ref.starts_with("question:") {
        return "question".to_string();
    }
    let Some(entry) = parse_longmemeval_turn_ref(entry_ref) else {
        return "context_candidate".to_string();
    };
    let Some(candidate) = parse_longmemeval_turn_ref(candidate_ref) else {
        return "context_candidate".to_string();
    };
    if entry.question_id != candidate.question_id {
        return "other_question_turn".to_string();
    }
    if entry.session_ref == candidate.session_ref {
        return match entry.sequence - candidate.sequence {
            0 => "same_turn".to_string(),
            1 => "previous_turn_same_session".to_string(),
            distance if distance > 1 => "older_turn_same_session".to_string(),
            _ => "future_turn_same_session".to_string(),
        };
    }
    "other_session_turn".to_string()
}

fn writer_candidate_priority(role: &str) -> u64 {
    match role {
        "target_question" => 10,
        "previous_turn_same_session" => 20,
        "older_turn_same_session" => 30,
        "other_session_turn" => 40,
        "question" => 50,
        "dimension_scope" => 90,
        _ => 80,
    }
}

fn writer_candidate_relation_hint(role: &str) -> &'static str {
    match role {
        "target_question" => "entry_answers_or_supports_question",
        "previous_turn_same_session" | "older_turn_same_session" => {
            "entry_uses_prior_session_context"
        }
        "other_session_turn" => "entry_correlates_with_other_session_context",
        "dimension_scope" => "scope_anchor",
        _ => "context_candidate",
    }
}

fn parse_longmemeval_turn_ref(reference: &str) -> Option<LongMemEvalTurnRef> {
    let parts = reference.split(':').collect::<Vec<_>>();
    if parts.len() < 7 || parts[0] != "turn" || parts[1] != "run" || parts[3] != "question" {
        return None;
    }
    let question_id = parts[4].to_string();
    let sequence = parts.last()?.parse::<i64>().ok()?;
    let session_ref = parts[5..parts.len() - 1].join(":");
    Some(LongMemEvalTurnRef {
        question_id,
        session_ref,
        sequence,
    })
}

fn longmemeval_question_ref(about: &str, question_id: &str) -> String {
    match longmemeval_run_id_from_about(about) {
        Some(run_id) => format!("question:run:{run_id}:question:{question_id}"),
        None => format!("question:{question_id}"),
    }
}

fn longmemeval_run_id_from_about(about: &str) -> Option<&str> {
    let parts = about.split(':').collect::<Vec<_>>();
    if parts.len() >= 4 && parts[0] == "longmemeval" && parts[1] == "run" {
        return Some(parts[2]);
    }
    None
}

fn write_outputs(
    args: &Args,
    artifacts: &Path,
    run_id: &str,
    data: ExportData,
    bounded_failures: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let summary = summary(args, artifacts, run_id, &data, bounded_failures)?;
    let redaction = data.redaction.finish();
    write_jsonl(&args.output.join("trajectories.jsonl"), &data.trajectories)?;
    write_json(&args.output.join("summary.json"), &summary)?;
    write_jsonl(&args.output.join("failures.jsonl"), &data.failures)?;
    write_json(&args.output.join("redaction_report.json"), &redaction)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn summary(
    args: &Args,
    artifacts: &Path,
    run_id: &str,
    data: &ExportData,
    bounded_failures: usize,
) -> Result<ExportSummary, Box<dyn Error + Send + Sync>> {
    let mut task_families = BTreeMap::<String, usize>::new();
    let mut modes = BTreeMap::<String, usize>::new();
    let mut target_actions = BTreeMap::<String, usize>::new();
    let mut tool_call_trajectories = 0usize;
    let mut stop_trajectories = 0usize;
    let mut writer_read_trajectories = 0usize;

    for item in &data.trajectories {
        *task_families.entry(item.task_family.clone()).or_default() += 1;
        *modes.entry(item.mode.clone()).or_default() += 1;
        if item.mode == "write_context_read" {
            writer_read_trajectories += 1;
        }
        let action_type = item
            .target_action
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        match action_type {
            "tool_call" => {
                tool_call_trajectories += 1;
                let tool = item
                    .target_action
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                *target_actions
                    .entry(format!("tool_call:{tool}"))
                    .or_default() += 1;
            }
            "stop" => {
                stop_trajectories += 1;
                *target_actions.entry("stop".to_string()).or_default() += 1;
            }
            other => {
                *target_actions.entry(other.to_string()).or_default() += 1;
            }
        }
    }

    Ok(ExportSummary {
        exporter: EXPORTER,
        schema_version: SCHEMA_VERSION,
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        source_run: args.run.display().to_string(),
        source_artifacts: artifacts.display().to_string(),
        output: args.output.display().to_string(),
        run_id: run_id.to_string(),
        task_families,
        modes,
        target_actions,
        trajectories: data.trajectories.len(),
        tool_call_trajectories,
        stop_trajectories,
        writer_read_trajectories,
        failure_rows: data.failures.len(),
        bounded_failures,
        redaction_findings: data.redaction.findings.len(),
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn write_jsonl<T: Serialize>(
    path: &Path,
    values: &[T],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    for value in values {
        serde_json::to_writer(&mut writer, value)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
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

fn bool_field(value: &Value, field: &str) -> Option<bool> {
    value.get(field).and_then(Value::as_bool)
}

fn string_array(value: &Value, field: &str) -> Vec<String> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn collect_run_ids(value: &Value, out: &mut BTreeMap<String, usize>) {
    collect_strings(value, &mut |text| {
        if let Some(run_id) = run_id_from_ref(text) {
            *out.entry(run_id).or_default() += 1;
        }
    });
}

fn collect_strings(value: &Value, visit: &mut impl FnMut(&str)) {
    match value {
        Value::String(text) => visit(text),
        Value::Array(items) => {
            for item in items {
                collect_strings(item, visit);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                visit(key);
                collect_strings(value, visit);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn run_id_from_ref(value: &str) -> Option<String> {
    let parts = value.split(':').collect::<Vec<_>>();
    parts
        .windows(2)
        .find(|window| window[0] == "run")
        .map(|window| window[1].to_string())
}

fn resolve_run_id(
    run_ids: &BTreeMap<String, usize>,
    expected_run_id: Option<&str>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    if let Some(expected) = expected_run_id {
        let foreign = run_ids
            .keys()
            .filter(|run_id| run_id.as_str() != expected)
            .cloned()
            .collect::<Vec<_>>();
        if !foreign.is_empty() {
            return Err(format!(
                "mixed run ids detected; expected `{expected}`, found foreign ids: {}",
                foreign.join(", ")
            )
            .into());
        }
        return Ok(expected.to_string());
    }
    match run_ids.len() {
        0 => Err("no LongMemEval run id found in exported artifacts".into()),
        1 => run_ids
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| "no LongMemEval run id found in exported artifacts".into()),
        _ => Err(format!(
            "mixed run ids detected: {}",
            run_ids.keys().cloned().collect::<Vec<_>>().join(", ")
        )
        .into()),
    }
}

fn scan_redaction(value: &Value, path: &str, report: &mut RedactionReportBuilder) {
    report.checked_values += 1;
    match value {
        Value::String(text) => {
            if let Some(reason) = secret_reason(text) {
                report.findings.push(RedactionFinding {
                    path: path.to_string(),
                    reason,
                });
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                scan_redaction(item, &format!("{path}[{index}]"), report);
            }
        }
        Value::Object(map) => scan_redaction_map(map, path, report),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn scan_redaction_map(map: &Map<String, Value>, path: &str, report: &mut RedactionReportBuilder) {
    for (key, value) in map {
        if secret_key(key) {
            report.findings.push(RedactionFinding {
                path: format!("{path}.{key}"),
                reason: "secret-like field name".to_string(),
            });
        }
        scan_redaction(value, &format!("{path}.{key}"), report);
    }
}

fn secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "authorization"
        || key == "password"
        || key == "api_key"
        || key == "apikey"
        || key == "secret"
        || key.ends_with("_secret")
        || key.ends_with("_token")
}

fn secret_reason(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if text.contains("-----BEGIN") {
        return Some("pem material".to_string());
    }
    if text.contains("ghp_") || text.contains("github_pat_") {
        return Some("github token-like value".to_string());
    }
    if text.contains("sk-") {
        return Some("openai token-like value".to_string());
    }
    if text.contains("AKIA") {
        return Some("aws access key-like value".to_string());
    }
    if lower.contains("bearer ") {
        return Some("bearer token-like value".to_string());
    }
    if lower.contains("password=") || lower.contains("api_key=") || lower.contains("token=") {
        return Some("secret assignment-like value".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_ref_preserves_run_scope() {
        assert_eq!(
            longmemeval_question_ref("longmemeval:run:lme-1:item:q1", "q1"),
            "question:run:lme-1:question:q1"
        );
        assert_eq!(
            longmemeval_question_ref("longmemeval:item:q1", "q1"),
            "question:q1"
        );
    }

    #[test]
    fn parses_longmemeval_turn_refs() {
        let parsed = parse_longmemeval_turn_ref("turn:run:lme-a:question:q1:answer_session_1:7")
            .expect("turn ref should parse");
        assert_eq!(parsed.question_id, "q1");
        assert_eq!(parsed.session_ref, "answer_session_1");
        assert_eq!(parsed.sequence, 7);
    }

    #[test]
    fn writer_candidate_role_prefers_target_question() {
        assert_eq!(
            writer_candidate_role(
                "turn:run:lme-a:question:q1:answer_session_1:7",
                "question:run:lme-a:question:q1",
                "question:run:lme-a:question:q1",
            ),
            "target_question"
        );
    }

    #[test]
    fn writer_candidate_details_do_not_expose_exporter_sources() {
        let value = json!({
            "write_request": {
                "read_context": {
                    "inspected_refs": ["turn:run:lme-a:question:q1:answer_session_1:6"],
                    "temporal_refs": ["turn:run:lme-a:question:q1:answer_session_1:5"]
                },
                "connect_to": [
                    { "ref": "question:run:lme-a:question:q1" }
                ]
            },
            "relation_quality": [
                { "to": "turn:run:lme-a:question:q1:answer_session_1:4" }
            ]
        });
        let calls = vec![json!({
            "arguments": {
                "around": { "ref": "turn:run:lme-a:question:q1:answer_session_1:6" }
            }
        })];

        let details = writer_candidate_ref_details(
            &value,
            &calls,
            "turn:run:lme-a:question:q1:answer_session_1:7",
            "question:run:lme-a:question:q1",
        );

        assert!(!details.is_empty());
        assert!(details.iter().all(|detail| detail.get("sources").is_none()));
    }

    #[test]
    fn namespaced_step_id_adds_run_scope_once() {
        assert_eq!(
            namespaced_step_id("lme-a", "longmemeval:multi-session:q1:read:0"),
            "longmemeval:run:lme-a:multi-session:q1:read:0"
        );
        assert_eq!(
            namespaced_step_id("lme-a", "longmemeval:run:lme-a:multi-session:q1:read:0"),
            "longmemeval:run:lme-a:multi-session:q1:read:0"
        );
    }

    #[test]
    fn redaction_detects_secret_like_values() {
        let mut report = RedactionReportBuilder::default();
        report.scan(&json!({
            "safe": "node:1",
            "authorization": "Bearer sk-test",
        }));
        assert_eq!(report.findings.len(), 2);
    }
}
