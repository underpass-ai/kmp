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
const EXPORTER: &str = "kernel-operator-trajectory-export-v1";
const DEFAULT_TOOL_CALL_BUDGET: usize = 6;
const DEFAULT_CONTEXT_CHARS: usize = 12_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    run: PathBuf,
    output: PathBuf,
    expected_run_id: Option<String>,
    include_writer_reads: bool,
    force: bool,
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
struct MemoryArenaTurnRef {
    subtask_index: i64,
    turn_kind: String,
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

    let mut data = ExportData::default();
    export_results(&args.run.join("results.jsonl"), &mut data)?;
    if args.include_writer_reads {
        export_writer_reads(&args.run.join("writer_results.jsonl"), &mut data)?;
    }

    let run_id = resolve_run_id(&data.run_ids, args.expected_run_id.as_deref())?;
    for item in &mut data.trajectories {
        item.run_id = run_id.clone();
    }

    let bounded_failures = data
        .failures
        .iter()
        .filter(|failure| failure.reason == "unbounded_tool_call")
        .count();
    if bounded_failures > 0 {
        write_outputs(&args, &run_id, data, bounded_failures)?;
        return Err(format!("refusing to export {bounded_failures} unbounded tool calls").into());
    }

    let redaction_findings = data.redaction.findings.len();
    if redaction_findings > 0 {
        write_outputs(&args, &run_id, data, bounded_failures)?;
        return Err(format!("refusing to export {redaction_findings} redaction findings").into());
    }

    write_outputs(&args, &run_id, data, bounded_failures)?;
    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut run = None;
    let mut output = None;
    let mut expected_run_id = None;
    let mut include_writer_reads = false;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--run" => run = Some(PathBuf::from(next_arg(&mut args, "--run")?)),
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
    "usage: kernel_operator_trajectory_export --run <memoryarena-run-dir> --output <dir> [--expected-run-id id] [--include-writer-reads] [--force]".to_string()
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

fn export_results(path: &Path, data: &mut ExportData) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (line_index, value) in read_jsonl(path)?.into_iter().enumerate() {
        collect_run_ids(&value, &mut data.run_ids);
        let location = format!("{}:{}", path.display(), line_index + 1);
        let Some(nav) = value.get("mcp_navigation") else {
            data.failures.push(FailureRow {
                source: "memoryarena.results".to_string(),
                location,
                reason: "missing_mcp_navigation".to_string(),
                detail: json!({}),
            });
            continue;
        };
        let Some(calls) = nav.get("calls").and_then(Value::as_array) else {
            data.failures.push(FailureRow {
                source: "memoryarena.results".to_string(),
                location,
                reason: "missing_mcp_navigation_calls".to_string(),
                detail: json!({}),
            });
            continue;
        };

        let about = required_string(&value, "about", &location)?;
        let task_type = required_string(&value, "task_type", &location)?;
        let task_id = required_string(&value, "task_id", &location)?;
        let subtask_index = value.get("subtask_index").and_then(Value::as_u64);
        let current_ref = required_string(&value, "current_question_ref", &location)?;
        let trace_target_ref = nav
            .get("trace_target_ref")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let allowed_known_at_refs = string_array(&value, "allowed_known_at_refs");
        let missing_allowed_refs = string_array(&value, "missing_allowed_refs");
        let final_observed_refs = string_array(nav, "observed_refs");
        let final_observed_allowed_refs = string_array(nav, "observed_allowed_refs");
        let final_unexpected_refs = string_array(nav, "unexpected_refs");
        let known_at_clean = bool_field(nav, "known_at_clean").unwrap_or(false);
        let future_answer_leak = bool_field(nav, "future_answer_leaked").unwrap_or(false);
        let current_question_observed =
            bool_field(nav, "current_question_observed").unwrap_or(false);
        let task_family = format!("memoryarena.{task_type}");
        let allowed_tools = kernel_operator_allowed_read_tools();
        let mut known_refs = BTreeSet::<String>::new();
        let mut last_tool: Option<String> = None;
        let mut last_observed_refs = Vec::<String>::new();
        let mut last_page: Option<Value> = None;
        let mut last_partial_result: Option<bool> = None;

        for (call_index, call) in calls.iter().enumerate() {
            let Some(tool) = call.get("tool").and_then(Value::as_str) else {
                data.failures.push(FailureRow {
                    source: "memoryarena.results".to_string(),
                    location: format!("{location}:calls[{call_index}]"),
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
                    source: "memoryarena.results".to_string(),
                    location: format!("{location}:calls[{call_index}]"),
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
                task_family: task_family.clone(),
                mode: "read".to_string(),
                source: "memoryarena.results.mcp_navigation".to_string(),
                about: about.to_string(),
                step_id: format!(
                    "memoryarena:{task_type}:{task_id}:subtask:{}:read:{call_index}",
                    subtask_index
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ),
                step_index: call_index,
                goal: "Choose the next bounded KMP read/navigation move for the current memory question.".to_string(),
                visible_state: json!({
                    "current_ref": current_ref,
                    "trace_target_ref": trace_target_ref,
                    "known_refs": known_refs.iter().cloned().collect::<Vec<_>>(),
                    "last_tool": last_tool,
                    "last_observed_refs": last_observed_refs,
                    "last_result_page": last_page.clone(),
                    "last_result_partial": last_partial_result,
                    "remaining_budget": {
                        "tool_calls": calls.len().saturating_sub(call_index),
                        "context_chars": DEFAULT_CONTEXT_CHARS,
                    },
                    "task": {
                        "benchmark": "MemoryArena",
                        "task_type": task_type,
                        "task_id": task_id,
                        "subtask_index": subtask_index,
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
                    "known_at_clean": known_at_clean,
                    "future_answer_leak": future_answer_leak,
                    "invalid_tool_call": false,
                    "bounded": bounded,
                    "stop_correct": null,
                    "current_question_observed": current_question_observed,
                    "allowed_known_at_ref_count": allowed_known_at_refs.len(),
                    "missing_allowed_ref_count": missing_allowed_refs.len(),
                    "unexpected_ref_count": final_unexpected_refs.len(),
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

        let stop_correct = known_at_clean && !future_answer_leak && missing_allowed_refs.is_empty();
        let stop_item = TrajectoryItem {
            schema_version: SCHEMA_VERSION,
            run_id: String::new(),
            task_family,
            mode: "read".to_string(),
            source: "memoryarena.results.mcp_navigation".to_string(),
            about: about.to_string(),
            step_id: format!(
                "memoryarena:{task_type}:{task_id}:subtask:{}:stop",
                subtask_index
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            step_index: calls.len(),
            goal: "Stop when bounded KMP evidence is sufficient, clean, and available without future leakage.".to_string(),
            visible_state: json!({
                "current_ref": current_ref,
                "trace_target_ref": trace_target_ref,
                "known_refs": known_refs.iter().cloned().collect::<Vec<_>>(),
                "last_tool": last_tool,
                "last_observed_refs": last_observed_refs,
                "last_result_page": last_page.clone(),
                "last_result_partial": last_partial_result,
                "remaining_budget": {
                    "tool_calls": 0,
                    "context_chars": DEFAULT_CONTEXT_CHARS,
                },
                "task": {
                    "benchmark": "MemoryArena",
                    "task_type": task_type,
                    "task_id": task_id,
                    "subtask_index": subtask_index,
                }
            }),
            allowed_tools,
            target_action: json!({
                "type": "stop",
                "answer_policy": "evidence_or_unknown",
                "final_refs": final_observed_allowed_refs,
                "reason": if stop_correct { "sufficient_evidence" } else { "incomplete_or_contaminated_evidence" },
            }),
            observed_outcome: Some(json!({
                "success": stop_correct,
                "observed_refs": final_observed_refs,
                "observed_allowed_refs": final_observed_allowed_refs,
                "unexpected_refs": final_unexpected_refs,
            })),
            quality: json!({
                "known_at_clean": known_at_clean,
                "future_answer_leak": future_answer_leak,
                "invalid_tool_call": false,
                "bounded": true,
                "stop_correct": stop_correct,
                "current_question_observed": current_question_observed,
                "allowed_known_at_ref_count": allowed_known_at_refs.len(),
                "missing_allowed_ref_count": missing_allowed_refs.len(),
                "unexpected_ref_count": final_unexpected_refs.len(),
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
        let task_family = "memoryarena.smart_writer".to_string();
        let entry_kind = value
            .get("entry_kind")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let calls = value
            .get("pre_read_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut known_refs = BTreeSet::<String>::new();
        let mut last_tool: Option<String> = None;
        let mut last_observed_refs = Vec::<String>::new();
        let mut last_page: Option<Value> = None;
        let mut last_partial_result: Option<bool> = None;
        let allowed_tools = kernel_operator_allowed_read_tools();
        let candidate_ref_details =
            writer_candidate_ref_details(&value, &calls, entry_ref, entry_kind);
        let candidate_refs = candidate_ref_details
            .iter()
            .filter_map(|detail| detail.get("ref").and_then(Value::as_str))
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        for (call_index, call) in calls.iter().enumerate() {
            let Some(tool) = call.get("tool").and_then(Value::as_str) else {
                data.failures.push(FailureRow {
                    source: "memoryarena.writer_results".to_string(),
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
                    source: "memoryarena.writer_results".to_string(),
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
                task_family: task_family.clone(),
                mode: "write_context_read".to_string(),
                source: "memoryarena.writer_results.pre_read_calls".to_string(),
                about: about.to_string(),
                step_id: format!("memoryarena:writer:{entry_ref}:read:{call_index}"),
                step_index: call_index,
                goal:
                    "Choose the next bounded KMP read move needed before writing a memory relation."
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
    entry_kind: &str,
) -> Vec<Value> {
    let mut candidates = BTreeMap::<String, CandidateDraft>::new();

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
        .map(|candidate| writer_candidate_detail(candidate, entry_ref, entry_kind))
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

fn writer_candidate_detail(candidate: &CandidateDraft, entry_ref: &str, entry_kind: &str) -> Value {
    let role = writer_candidate_role(entry_ref, &candidate.reference);
    let turn = parse_memoryarena_turn_ref(&candidate.reference);
    let temporal_distance = parse_memoryarena_turn_ref(entry_ref)
        .zip(turn.clone())
        .map(|(entry, candidate)| entry.subtask_index - candidate.subtask_index);
    json!({
        "ref": candidate.reference,
        "role": role,
        "turn_kind": turn
            .as_ref()
            .map(|turn| turn.turn_kind.as_str())
            .unwrap_or("unknown"),
        "relative_position": temporal_position(temporal_distance),
        "temporal_distance": temporal_distance,
        "priority": writer_candidate_priority(entry_kind, &role),
        "relation_hint": writer_candidate_relation_hint(entry_kind, &role),
    })
}

fn writer_candidate_role(entry_ref: &str, candidate_ref: &str) -> String {
    if candidate_ref.contains(":dimension:") {
        return "dimension_scope".to_string();
    }
    let Some(entry) = parse_memoryarena_turn_ref(entry_ref) else {
        return "unknown".to_string();
    };
    let Some(candidate) = parse_memoryarena_turn_ref(candidate_ref) else {
        return "unknown".to_string();
    };
    let position = temporal_position(Some(entry.subtask_index - candidate.subtask_index));
    format!("{position}_{}", candidate.turn_kind)
}

fn writer_candidate_priority(entry_kind: &str, role: &str) -> u64 {
    match (entry_kind, role) {
        ("subtask_answer_feedback", "same_subtask_question") => 10,
        ("subtask_answer_feedback", "previous_subtask_answer") => 30,
        ("subtask_answer_feedback", "previous_subtask_question") => 40,
        ("subtask_question", "previous_subtask_answer") => 10,
        ("subtask_question", "previous_subtask_question") => 30,
        ("subtask_question", "older_subtask_answer") => 50,
        (_, "same_subtask_question") => 30,
        (_, "previous_subtask_answer") => 40,
        (_, "dimension_scope") => 90,
        _ => 80,
    }
}

fn writer_candidate_relation_hint(entry_kind: &str, role: &str) -> &'static str {
    match (entry_kind, role) {
        ("subtask_answer_feedback", "same_subtask_question") => "answer_addresses_question",
        ("subtask_answer_feedback", "previous_subtask_answer") => "answer_uses_prior_answer",
        ("subtask_answer_feedback", "previous_subtask_question") => "answer_uses_prior_question",
        ("subtask_question", "previous_subtask_answer") => "question_follows_previous_answer",
        ("subtask_question", "previous_subtask_question") => "question_refines_previous_question",
        ("subtask_question", "older_subtask_answer") => "question_uses_older_answer",
        (_, "dimension_scope") => "scope_anchor",
        _ => "context_candidate",
    }
}

fn temporal_position(distance: Option<i64>) -> &'static str {
    match distance {
        Some(0) => "same_subtask",
        Some(1) => "previous_subtask",
        Some(distance) if distance > 1 => "older_subtask",
        Some(distance) if distance < 0 => "future_subtask",
        _ => "unknown",
    }
}

fn parse_memoryarena_turn_ref(reference: &str) -> Option<MemoryArenaTurnRef> {
    let (_, rest) = reference.rsplit_once(":subtask:")?;
    let mut parts = rest.split(':');
    let subtask_index = parts.next()?.parse::<i64>().ok()?;
    let turn_kind = parts.next()?;
    if turn_kind != "question" && turn_kind != "answer" {
        return None;
    }
    Some(MemoryArenaTurnRef {
        subtask_index,
        turn_kind: turn_kind.to_string(),
    })
}

fn write_outputs(
    args: &Args,
    run_id: &str,
    data: ExportData,
    bounded_failures: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let summary = summary(args, run_id, &data, bounded_failures)?;
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
        if let Some(run_id) = memoryarena_run_id(text) {
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

fn memoryarena_run_id(value: &str) -> Option<String> {
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
        0 => Err("no MemoryArena run id found in exported artifacts".into()),
        1 => run_ids
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| "no MemoryArena run id found in exported artifacts".into()),
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
    fn run_id_resolution_fails_on_mixed_runs() {
        let mut run_ids = BTreeMap::new();
        run_ids.insert("run-a".to_string(), 1);
        run_ids.insert("run-b".to_string(), 1);
        let error = resolve_run_id(&run_ids, None)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(error.contains("mixed run ids"));
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

    #[test]
    fn call_partial_result_uses_page_has_more_when_not_explicit() {
        let call = json!({
            "page": {
                "returned": 12,
                "total": 20,
                "has_more": true,
                "next_cursor": "ref:next"
            }
        });

        assert_eq!(call_partial_result(&call), Some(true));
        assert_eq!(
            call_page(&call).and_then(|page| page.get("total").cloned()),
            Some(json!(20))
        );
    }

    #[test]
    fn call_partial_result_prefers_explicit_value() {
        let call = json!({
            "partial_result": false,
            "page": {
                "has_more": true
            }
        });

        assert_eq!(call_partial_result(&call), Some(false));
    }

    #[test]
    fn writer_candidate_ref_details_collect_visible_structural_candidates() {
        let value = json!({
            "write_request": {
                "read_context": {
                    "inspected_refs": ["entry:1", "node:inspected"],
                    "temporal_refs": ["node:temporal"]
                },
                "connect_to": [
                    { "ref": "node:relation" },
                    { "ref": "entry:1" }
                ]
            },
            "relation_quality": [
                { "to": "node:quality" },
                { "to": "entry:1" }
            ]
        });
        let calls = vec![
            json!({
                "arguments": {
                    "around": { "ref": "node:near" }
                }
            }),
            json!({
                "arguments": {
                    "from": "node:from",
                    "to": "node:to",
                    "ref": "entry:1"
                }
            }),
        ];

        let details =
            writer_candidate_ref_details(&value, &calls, "entry:1", "subtask_answer_feedback");
        let refs = details
            .iter()
            .filter_map(|detail| detail.get("ref").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert_eq!(
            refs,
            [
                "node:from",
                "node:inspected",
                "node:near",
                "node:quality",
                "node:relation",
                "node:temporal",
                "node:to",
            ]
        );
        assert!(details.iter().all(|detail| detail.get("sources").is_none()));
    }

    #[test]
    fn writer_candidate_ref_details_rank_memoryarena_roles() {
        let value = json!({
            "write_request": {
                "read_context": {
                    "inspected_refs": [
                        "memoryarena:run:r:task_type:progressive_search:task:1:subtask:9:answer",
                        "memoryarena:run:r:task_type:progressive_search:task:1:subtask:10:question"
                    ],
                    "temporal_refs": [
                        "memoryarena:run:r:task_type:progressive_search:task:1:subtask:9:question"
                    ]
                }
            }
        });

        let details = writer_candidate_ref_details(
            &value,
            &[],
            "memoryarena:run:r:task_type:progressive_search:task:1:subtask:10:answer",
            "subtask_answer_feedback",
        );

        assert_eq!(
            details
                .first()
                .and_then(|detail| detail.get("role"))
                .and_then(Value::as_str),
            Some("same_subtask_question")
        );
        assert_eq!(
            details
                .first()
                .and_then(|detail| detail.get("relation_hint"))
                .and_then(Value::as_str),
            Some("answer_addresses_question")
        );
    }
}
