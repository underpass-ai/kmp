use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    MemoryArenaExpected, memoryarena_answer_candidates_from_text, memoryarena_task_success_rule,
    score_memoryarena_answer,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

type TaskKey = (String, String);
type TaskSubtaskKey = (String, String, usize);
type ExpectedByAsk<'a> = BTreeMap<TaskSubtaskKey, &'a MemoryArenaExpected>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    artifacts: PathBuf,
    run: PathBuf,
    output: PathBuf,
    limit_tasks: Option<usize>,
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
    allowed_known_at_refs: Vec<String>,
    #[serde(default)]
    observed_allowed_refs: Vec<String>,
    #[serde(default)]
    unexpected_refs: Vec<String>,
    #[serde(default)]
    missing_allowed_refs: Vec<String>,
    current_question_observed: bool,
    future_answer_leaked: bool,
    known_at_clean: bool,
    #[serde(default)]
    ask_answer: Option<String>,
    #[serde(default)]
    ask_elapsed_ms: u128,
}

#[derive(Debug, Clone, Deserialize)]
struct RunnerSummaryInput {
    #[serde(default)]
    total_events: Option<usize>,
    #[serde(default)]
    successful_events: Option<usize>,
    #[serde(default)]
    failed_events: Option<usize>,
    #[serde(default)]
    elapsed_ms: Option<u128>,
}

#[derive(Debug, Clone, Copy)]
struct FailureClassificationInput<'a> {
    task_type: &'a str,
    hard_success: bool,
    known_at_clean: bool,
    full_ref_recall: bool,
    current_question_observed: bool,
    future_answer_leaked: bool,
    unexpected_refs: &'a [String],
    ask_answer: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct SubtaskScoreResult {
    task_id: String,
    task_type: String,
    category: Option<String>,
    subtask_index: usize,
    question: String,
    expected_answer: Value,
    expected_answer_kind: String,
    expected_answers: Vec<String>,
    candidate_answers: Vec<String>,
    chosen_answer: Option<String>,
    hard_success: bool,
    candidate_answer_hit: bool,
    failure_class: &'static str,
    soft_score: Option<f64>,
    soft_score_basis: Option<&'static str>,
    known_at_clean: bool,
    full_ref_recall: bool,
    current_question_observed: bool,
    future_answer_leaked: bool,
    unexpected_refs: Vec<String>,
    missing_allowed_refs: Vec<String>,
    allowed_known_at_refs: usize,
    observed_allowed_refs: usize,
    ask_answer_chars: usize,
    ask_elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
struct TaskScoreResult {
    task_id: String,
    task_type: String,
    category: Option<String>,
    subtasks: usize,
    passed_subtasks: usize,
    process_score: f64,
    soft_process_score: Option<f64>,
    task_success_rule: &'static str,
    final_subtask_index: usize,
    final_question: String,
    expected_answers: Vec<String>,
    candidate_answers: Vec<String>,
    chosen_answer: Option<String>,
    final_subtask_success: bool,
    task_success: bool,
    candidate_answer_hit: bool,
    final_failure_class: &'static str,
    known_at_clean_subtasks: usize,
    known_at_clean_all_subtasks: bool,
    full_ref_recall_subtasks: usize,
    full_ref_recall_all_subtasks: bool,
    current_question_observed_subtasks: usize,
    future_answer_leaks: usize,
    unexpected_ref_asks: usize,
    missing_allowed_ref_asks: usize,
    final_allowed_known_at_refs: usize,
    final_observed_allowed_refs: usize,
    final_ask_answer_chars: usize,
    final_ask_elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
struct TaskTypeScoreSummary {
    tasks: usize,
    subtasks: usize,
    task_successes: usize,
    task_success_rate: f64,
    passed_subtasks: usize,
    process_score: f64,
    micro_process_score: f64,
    soft_process_score: Option<f64>,
}

#[derive(Debug, Serialize)]
struct DepthSuccessRate {
    subtasks: usize,
    successes: usize,
    success_rate: f64,
}

#[derive(Debug, Serialize)]
struct ScoreSummary {
    benchmark: &'static str,
    scorecard: &'static str,
    schema_version: &'static str,
    evaluation_protocol: &'static str,
    evaluator_limitations: &'static str,
    generated_at_unix_seconds: u64,
    artifacts: String,
    run: String,
    tasks: usize,
    subtasks: usize,
    ask_count: usize,
    task_successes: usize,
    task_success_rate: f64,
    passed_subtasks: usize,
    process_score: f64,
    micro_process_score: f64,
    soft_process_score: Option<f64>,
    candidate_answer_hits: usize,
    candidate_answer_hit_rate: f64,
    failure_classes: BTreeMap<String, usize>,
    known_at_clean_asks: usize,
    full_ref_recall_asks: usize,
    current_question_observed_asks: usize,
    future_answer_leaks: usize,
    unexpected_ref_asks: usize,
    missing_allowed_ref_asks: usize,
    runner_total_events: Option<usize>,
    runner_successful_events: Option<usize>,
    runner_failed_events: Option<usize>,
    runner_elapsed_ms: Option<u128>,
    by_task_type: BTreeMap<String, TaskTypeScoreSummary>,
    sr_at_depth: BTreeMap<usize, DepthSuccessRate>,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    ensure_output_dir(&args.output, args.force)?;

    let expected = read_expected(&args.artifacts.join("expected.jsonl"), args.limit_tasks)?;
    let run_results = read_run_results(&args.run.join("results.jsonl"), args.limit_tasks)?;
    ensure_non_empty_inputs(&expected, &run_results)?;
    let runner_summary = read_optional_runner_summary(&args.run.join("summary.json"))?;
    let subtask_results = score_subtasks(&expected, &run_results)?;
    let task_results = score_tasks(&subtask_results)?;
    let summary = summarize_scorecard(
        &args,
        &task_results,
        &subtask_results,
        &run_results,
        runner_summary,
    )?;

    write_jsonl(
        &args.output.join("subtask_results.jsonl"),
        subtask_results.iter().map(serde_json::to_value),
    )?;
    write_jsonl(
        &args.output.join("task_results.jsonl"),
        task_results.iter().map(serde_json::to_value),
    )?;
    write_jsonl(
        &args.output.join("hypotheses.jsonl"),
        task_results.iter().map(|item| {
            Ok(json!({
                "task_id": item.task_id,
                "hypothesis": item.chosen_answer.as_deref().unwrap_or("UNKNOWN"),
                "task_success": item.task_success
            }))
        }),
    )?;
    write_json_pretty(&args.output.join("score_summary.json"), &summary)?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn score_subtasks(
    expected: &[MemoryArenaExpected],
    run_results: &[RunAskResult],
) -> Result<Vec<SubtaskScoreResult>, Box<dyn Error + Send + Sync>> {
    let expected_by_key = expected_by_ask_key(expected)?;
    let mut run_by_key = BTreeMap::<TaskSubtaskKey, &RunAskResult>::new();

    for run in run_results {
        let key = ask_key(&run.task_type, &run.task_id, run.subtask_index);
        let expected = expected_by_key.get(&key).ok_or_else(|| {
            format!(
                "run result has no expected row for task_type {} task {} subtask {}",
                key.0, key.1, key.2
            )
        })?;
        validate_matching_item(expected, run)?;
        if run_by_key.insert(key.clone(), run).is_some() {
            return Err(format!(
                "duplicate run result for task_type {} task {} subtask {}",
                key.0, key.1, key.2
            )
            .into());
        }
    }

    for key in expected_by_key.keys() {
        if !run_by_key.contains_key(key) {
            return Err(format!(
                "expected row has no run result for task_type {} task {} subtask {}",
                key.0, key.1, key.2
            )
            .into());
        }
    }

    let mut scored = Vec::new();
    for (key, expected) in expected_by_key {
        let run = run_by_key.get(&key).ok_or_else(|| {
            format!(
                "missing run result for task_type {} task {} subtask {}",
                key.0, key.1, key.2
            )
        })?;
        let candidate_answers = run
            .ask_answer
            .as_deref()
            .map(memoryarena_answer_candidates_from_text)
            .unwrap_or_default();
        let chosen_answer = candidate_answers.last().cloned();
        let answer_score = score_memoryarena_answer(
            &expected.task_type,
            &expected.answer,
            run.ask_answer.as_deref(),
        );
        let full_ref_recall = run.missing_allowed_refs.is_empty();
        let failure_class = classify_subtask_failure(FailureClassificationInput {
            task_type: &run.task_type,
            hard_success: answer_score.hard_success,
            known_at_clean: run.known_at_clean,
            full_ref_recall,
            current_question_observed: run.current_question_observed,
            future_answer_leaked: run.future_answer_leaked,
            unexpected_refs: &run.unexpected_refs,
            ask_answer: run.ask_answer.as_deref(),
        });

        scored.push(SubtaskScoreResult {
            task_id: run.task_id.clone(),
            task_type: run.task_type.clone(),
            category: run.category.clone(),
            subtask_index: run.subtask_index,
            question: run.question.clone(),
            expected_answer: expected.answer.clone(),
            expected_answer_kind: answer_score.expected_answer_kind,
            expected_answers: answer_score.expected_answers,
            candidate_answers,
            chosen_answer,
            hard_success: answer_score.hard_success,
            candidate_answer_hit: answer_score.candidate_answer_hit,
            failure_class,
            soft_score: answer_score.soft_score,
            soft_score_basis: answer_score.soft_score_basis,
            known_at_clean: run.known_at_clean,
            full_ref_recall,
            current_question_observed: run.current_question_observed,
            future_answer_leaked: run.future_answer_leaked,
            unexpected_refs: run.unexpected_refs.clone(),
            missing_allowed_refs: run.missing_allowed_refs.clone(),
            allowed_known_at_refs: run.allowed_known_at_refs.len(),
            observed_allowed_refs: run.observed_allowed_refs.len(),
            ask_answer_chars: run.ask_answer.as_deref().unwrap_or_default().len(),
            ask_elapsed_ms: run.ask_elapsed_ms,
        });
    }

    scored.sort_by(|left, right| {
        left.task_type
            .cmp(&right.task_type)
            .then(left.task_id.cmp(&right.task_id))
            .then(left.subtask_index.cmp(&right.subtask_index))
    });
    Ok(scored)
}

fn ensure_non_empty_inputs(
    expected: &[MemoryArenaExpected],
    run_results: &[RunAskResult],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if expected.is_empty() {
        return Err("scorecard input expected.jsonl has no rows after filtering".into());
    }
    if run_results.is_empty() {
        return Err("scorecard input results.jsonl has no rows after filtering".into());
    }
    Ok(())
}

fn score_tasks(
    subtask_results: &[SubtaskScoreResult],
) -> Result<Vec<TaskScoreResult>, Box<dyn Error + Send + Sync>> {
    let mut subtasks_by_task = BTreeMap::<TaskKey, Vec<&SubtaskScoreResult>>::new();
    for subtask in subtask_results {
        subtasks_by_task
            .entry(task_key(&subtask.task_type, &subtask.task_id))
            .or_default()
            .push(subtask);
    }

    let mut scored = Vec::new();
    for (_task_id, mut task_subtasks) in subtasks_by_task {
        task_subtasks.sort_by_key(|subtask| subtask.subtask_index);
        let final_subtask = task_subtasks
            .last()
            .ok_or("internal error: task group has no subtask rows")?;
        let subtasks = task_subtasks.len();
        let passed_subtasks = task_subtasks
            .iter()
            .filter(|subtask| subtask.hard_success)
            .count();
        let process_score = ratio(passed_subtasks, subtasks);
        let soft_scores = task_subtasks
            .iter()
            .filter_map(|subtask| subtask.soft_score)
            .collect::<Vec<_>>();
        let soft_process_score = if soft_scores.is_empty() {
            None
        } else {
            Some(soft_scores.iter().sum::<f64>() / soft_scores.len() as f64)
        };
        let final_subtask_success = final_subtask.hard_success;
        let task_success_rule = memoryarena_task_success_rule(&final_subtask.task_type);
        let task_success = match task_success_rule {
            "all_subtasks_hard_success" => passed_subtasks == subtasks,
            "final_subtask_hard_success" => final_subtask_success,
            _ => final_subtask_success,
        };
        let known_at_clean_subtasks = task_subtasks
            .iter()
            .filter(|subtask| subtask.known_at_clean)
            .count();
        let full_ref_recall_subtasks = task_subtasks
            .iter()
            .filter(|subtask| subtask.full_ref_recall)
            .count();
        let current_question_observed_subtasks = task_subtasks
            .iter()
            .filter(|subtask| subtask.current_question_observed)
            .count();
        let future_answer_leaks = task_subtasks
            .iter()
            .filter(|subtask| subtask.future_answer_leaked)
            .count();
        let unexpected_ref_asks = task_subtasks
            .iter()
            .filter(|subtask| !subtask.unexpected_refs.is_empty())
            .count();
        let missing_allowed_ref_asks = task_subtasks
            .iter()
            .filter(|subtask| !subtask.missing_allowed_refs.is_empty())
            .count();

        scored.push(TaskScoreResult {
            task_id: final_subtask.task_id.clone(),
            task_type: final_subtask.task_type.clone(),
            category: final_subtask.category.clone(),
            subtasks,
            passed_subtasks,
            process_score,
            soft_process_score,
            task_success_rule,
            final_subtask_index: final_subtask.subtask_index,
            final_question: final_subtask.question.clone(),
            expected_answers: final_subtask.expected_answers.clone(),
            candidate_answers: final_subtask.candidate_answers.clone(),
            chosen_answer: final_subtask.chosen_answer.clone(),
            final_subtask_success,
            task_success,
            candidate_answer_hit: final_subtask.candidate_answer_hit,
            final_failure_class: final_subtask.failure_class,
            known_at_clean_subtasks,
            known_at_clean_all_subtasks: known_at_clean_subtasks == subtasks,
            full_ref_recall_subtasks,
            full_ref_recall_all_subtasks: full_ref_recall_subtasks == subtasks,
            current_question_observed_subtasks,
            future_answer_leaks,
            unexpected_ref_asks,
            missing_allowed_ref_asks,
            final_allowed_known_at_refs: final_subtask.allowed_known_at_refs,
            final_observed_allowed_refs: final_subtask.observed_allowed_refs,
            final_ask_answer_chars: final_subtask.ask_answer_chars,
            final_ask_elapsed_ms: final_subtask.ask_elapsed_ms,
        });
    }

    Ok(scored)
}

fn summarize_scorecard(
    args: &Args,
    task_results: &[TaskScoreResult],
    subtask_results: &[SubtaskScoreResult],
    run_results: &[RunAskResult],
    runner_summary: Option<RunnerSummaryInput>,
) -> Result<ScoreSummary, Box<dyn Error + Send + Sync>> {
    let tasks = task_results.len();
    let subtasks = subtask_results.len();
    let task_successes = task_results
        .iter()
        .filter(|result| result.task_success)
        .count();
    let passed_subtasks = subtask_results
        .iter()
        .filter(|result| result.hard_success)
        .count();
    let process_score = mean_task_process_score(task_results);
    let soft_process_score = mean_optional_task_process_score(task_results);
    let candidate_answer_hits = task_results
        .iter()
        .filter(|result| result.candidate_answer_hit)
        .count();
    let ask_count = run_results.len();
    let full_ref_recall_asks = run_results
        .iter()
        .filter(|result| result.missing_allowed_refs.is_empty())
        .count();
    let runner_total_events = runner_summary
        .as_ref()
        .and_then(|summary| summary.total_events);
    let runner_successful_events = runner_summary
        .as_ref()
        .and_then(|summary| summary.successful_events);
    let runner_failed_events = runner_summary
        .as_ref()
        .and_then(|summary| summary.failed_events);
    let runner_elapsed_ms = runner_summary
        .as_ref()
        .and_then(|summary| summary.elapsed_ms);

    Ok(ScoreSummary {
        benchmark: "MemoryArena",
        scorecard: "memoryarena-kmp-scorecard-paper-aligned-v1",
        schema_version: "memoryarena-score-summary-v1",
        evaluation_protocol: "paper-aligned local evaluator for arXiv:2602.16313 Section 4.2 metrics: SR, PS, and SR@depth; group-travel sPS is a local slot-coverage proxy because the official environment evaluator is not published.",
        evaluator_limitations: "This is not an official MemoryArena score. It scores KMP runner answer artifacts, not a full web/travel/formal task-agent environment rollout.",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        artifacts: args.artifacts.display().to_string(),
        run: args.run.display().to_string(),
        tasks,
        subtasks,
        ask_count,
        task_successes,
        task_success_rate: ratio(task_successes, tasks),
        passed_subtasks,
        process_score,
        micro_process_score: ratio(passed_subtasks, subtasks),
        soft_process_score,
        candidate_answer_hits,
        candidate_answer_hit_rate: ratio(candidate_answer_hits, tasks),
        failure_classes: summarize_failure_classes(subtask_results),
        known_at_clean_asks: run_results
            .iter()
            .filter(|result| result.known_at_clean)
            .count(),
        full_ref_recall_asks,
        current_question_observed_asks: run_results
            .iter()
            .filter(|result| result.current_question_observed)
            .count(),
        future_answer_leaks: run_results
            .iter()
            .filter(|result| result.future_answer_leaked)
            .count(),
        unexpected_ref_asks: run_results
            .iter()
            .filter(|result| !result.unexpected_refs.is_empty())
            .count(),
        missing_allowed_ref_asks: run_results
            .iter()
            .filter(|result| !result.missing_allowed_refs.is_empty())
            .count(),
        runner_total_events,
        runner_successful_events,
        runner_failed_events,
        runner_elapsed_ms,
        by_task_type: summarize_by_task_type(task_results, subtask_results),
        sr_at_depth: summarize_sr_at_depth(subtask_results),
    })
}

fn mean_task_process_score(task_results: &[TaskScoreResult]) -> f64 {
    if task_results.is_empty() {
        return 0.0;
    }
    task_results
        .iter()
        .map(|task| task.process_score)
        .sum::<f64>()
        / task_results.len() as f64
}

fn mean_optional_task_process_score(task_results: &[TaskScoreResult]) -> Option<f64> {
    let scores = task_results
        .iter()
        .filter_map(|task| task.soft_process_score)
        .collect::<Vec<_>>();
    if scores.is_empty() {
        None
    } else {
        Some(scores.iter().sum::<f64>() / scores.len() as f64)
    }
}

fn summarize_failure_classes(subtask_results: &[SubtaskScoreResult]) -> BTreeMap<String, usize> {
    let mut summary = BTreeMap::new();
    for result in subtask_results {
        *summary
            .entry(result.failure_class.to_string())
            .or_insert(0usize) += 1;
    }
    summary
}

fn summarize_by_task_type(
    task_results: &[TaskScoreResult],
    subtask_results: &[SubtaskScoreResult],
) -> BTreeMap<String, TaskTypeScoreSummary> {
    let mut task_results_by_type = BTreeMap::<String, Vec<&TaskScoreResult>>::new();
    for task in task_results {
        task_results_by_type
            .entry(task.task_type.clone())
            .or_default()
            .push(task);
    }
    let mut subtask_results_by_type = BTreeMap::<String, Vec<&SubtaskScoreResult>>::new();
    for subtask in subtask_results {
        subtask_results_by_type
            .entry(subtask.task_type.clone())
            .or_default()
            .push(subtask);
    }

    let mut summaries = BTreeMap::new();
    for (task_type, tasks_for_type) in task_results_by_type {
        let subtasks_for_type = subtask_results_by_type
            .get(&task_type)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let tasks = tasks_for_type.len();
        let subtasks = subtasks_for_type.len();
        let task_successes = tasks_for_type
            .iter()
            .filter(|task| task.task_success)
            .count();
        let passed_subtasks = subtasks_for_type
            .iter()
            .filter(|subtask| subtask.hard_success)
            .count();
        let process_score = if tasks_for_type.is_empty() {
            0.0
        } else {
            tasks_for_type
                .iter()
                .map(|task| task.process_score)
                .sum::<f64>()
                / tasks_for_type.len() as f64
        };
        let soft_scores = tasks_for_type
            .iter()
            .filter_map(|task| task.soft_process_score)
            .collect::<Vec<_>>();
        let soft_process_score = if soft_scores.is_empty() {
            None
        } else {
            Some(soft_scores.iter().sum::<f64>() / soft_scores.len() as f64)
        };
        summaries.insert(
            task_type,
            TaskTypeScoreSummary {
                tasks,
                subtasks,
                task_successes,
                task_success_rate: ratio(task_successes, tasks),
                passed_subtasks,
                process_score,
                micro_process_score: ratio(passed_subtasks, subtasks),
                soft_process_score,
            },
        );
    }
    summaries
}

fn summarize_sr_at_depth(
    subtask_results: &[SubtaskScoreResult],
) -> BTreeMap<usize, DepthSuccessRate> {
    let mut by_depth = BTreeMap::<usize, Vec<&SubtaskScoreResult>>::new();
    for subtask in subtask_results {
        by_depth
            .entry(subtask.subtask_index)
            .or_default()
            .push(subtask);
    }

    by_depth
        .into_iter()
        .map(|(depth, subtasks)| {
            let successes = subtasks
                .iter()
                .filter(|subtask| subtask.hard_success)
                .count();
            (
                depth,
                DepthSuccessRate {
                    subtasks: subtasks.len(),
                    successes,
                    success_rate: ratio(successes, subtasks.len()),
                },
            )
        })
        .collect()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn expected_by_ask_key(
    expected: &[MemoryArenaExpected],
) -> Result<ExpectedByAsk<'_>, Box<dyn Error + Send + Sync>> {
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

fn classify_subtask_failure(input: FailureClassificationInput<'_>) -> &'static str {
    if input.hard_success {
        return "passed";
    }
    if input.future_answer_leaked {
        return "kernel_temporal_leak";
    }
    if !input.known_at_clean || !input.unexpected_refs.is_empty() {
        return "kernel_known_at_gap";
    }
    if !input.current_question_observed {
        return "kernel_current_question_gap";
    }
    if !input.full_ref_recall {
        return "kernel_ref_recall_gap";
    }
    if answer_is_unknown(input.ask_answer) {
        return "no_prior_answer_evidence";
    }
    match input.task_type {
        "bundled_shopping" | "group_travel_planner" => "domain_agent_solution_gap",
        "formal_reasoning_math" | "formal_reasoning_phys" => "formal_reader_reasoning_gap",
        _ => "reader_reasoning_or_extraction_gap",
    }
}

fn answer_is_unknown(answer: Option<&str>) -> bool {
    let Some(answer) = answer else {
        return true;
    };
    let normalized = answer.trim();
    normalized.is_empty() || normalized.eq_ignore_ascii_case("UNKNOWN")
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

fn read_expected(
    path: &Path,
    limit_tasks: Option<usize>,
) -> Result<Vec<MemoryArenaExpected>, Box<dyn Error + Send + Sync>> {
    let selected_task_keys = selected_task_keys(path, limit_tasks)?;
    let expected = read_jsonl(path)?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|expected: &MemoryArenaExpected| {
            selected_task_keys
                .as_ref()
                .is_none_or(|keys| keys.contains(&task_key(&expected.task_type, &expected.task_id)))
        })
        .collect::<Vec<_>>();
    Ok(expected)
}

fn read_run_results(
    path: &Path,
    limit_tasks: Option<usize>,
) -> Result<Vec<RunAskResult>, Box<dyn Error + Send + Sync>> {
    let selected_task_keys = selected_task_keys(path, limit_tasks)?;
    let run_results = read_jsonl(path)?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|result: &RunAskResult| {
            selected_task_keys
                .as_ref()
                .is_none_or(|keys| keys.contains(&task_key(&result.task_type, &result.task_id)))
        })
        .collect::<Vec<_>>();
    Ok(run_results)
}

fn selected_task_keys(
    path: &Path,
    limit_tasks: Option<usize>,
) -> Result<Option<BTreeSet<TaskKey>>, Box<dyn Error + Send + Sync>> {
    let Some(limit) = limit_tasks else {
        return Ok(None);
    };
    let mut selected = BTreeSet::new();
    for value in read_jsonl(path)? {
        let task_type = required_string(&value, "task_type")?;
        let task_id = required_string(&value, "task_id")?;
        selected.insert(task_key(&task_type, &task_id));
        if selected.len() >= limit {
            break;
        }
    }
    Ok(Some(selected))
}

fn task_key(task_type: &str, task_id: &str) -> TaskKey {
    (task_type.to_string(), task_id.to_string())
}

fn ask_key(task_type: &str, task_id: &str, subtask_index: usize) -> TaskSubtaskKey {
    (task_type.to_string(), task_id.to_string(), subtask_index)
}

fn read_optional_runner_summary(
    path: &Path,
) -> Result<Option<RunnerSummaryInput>, Box<dyn Error + Send + Sync>> {
    if !path.exists() {
        return Ok(None);
    }
    let file = File::open(path)?;
    Ok(Some(serde_json::from_reader(file)?))
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
    let mut run = None;
    let mut output = None;
    let mut limit_tasks = None;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--artifacts" => artifacts = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--run" => run = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--limit-tasks" => {
                let value = required_flag_value(&mut args, &arg)?;
                let parsed = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --limit-tasks value `{value}`: {error}"))?;
                if parsed == 0 {
                    return Err("--limit-tasks must be greater than zero".into());
                }
                limit_tasks = Some(parsed);
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
        run: run.ok_or("--run is required")?,
        output: output.ok_or("--output is required")?,
        limit_tasks,
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
        "Usage: memoryarena_kmp_scorecard --artifacts <adapter-output-dir> --run <runner-output-dir> --output <score-dir> [--limit-tasks N] [--force]"
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rejects_empty_scorecard_inputs() {
        let empty_expected = Vec::new();
        let empty_run_results = Vec::new();

        let error = ensure_non_empty_inputs(&empty_expected, &empty_run_results)
            .expect_err("empty inputs must fail fast");

        assert!(
            error
                .to_string()
                .contains("expected.jsonl has no rows after filtering")
        );
    }

    #[test]
    fn task_success_uses_final_subtask_for_progressive_but_all_subtasks_for_shopping() {
        let progressive = score_tasks(&[
            subtask_fixture("progressive_search", "p1", 1, false),
            subtask_fixture("progressive_search", "p1", 2, true),
        ])
        .expect("progressive task should score");
        let shopping = score_tasks(&[
            subtask_fixture("bundled_shopping", "s1", 1, true),
            subtask_fixture("bundled_shopping", "s1", 2, false),
        ])
        .expect("shopping task should score");

        assert!(progressive[0].task_success);
        assert_eq!(progressive[0].process_score, 0.5);
        assert!(!shopping[0].task_success);
        assert_eq!(shopping[0].process_score, 0.5);
    }

    #[test]
    fn scoring_keys_include_task_type() {
        let expected = vec![
            expected_fixture("progressive_search", "1", 1, "alpha"),
            expected_fixture("formal_reasoning_phys", "1", 1, "beta"),
        ];
        let run_results = vec![
            run_fixture("progressive_search", "1", 1, "alpha"),
            run_fixture("formal_reasoning_phys", "1", 1, "beta"),
        ];

        let subtask_results =
            score_subtasks(&expected, &run_results).expect("same task_id across configs is valid");
        let task_results = score_tasks(&subtask_results).expect("tasks should score separately");

        assert_eq!(subtask_results.len(), 2);
        assert!(subtask_results.iter().all(|result| result.hard_success));
        assert_eq!(task_results.len(), 2);
    }

    #[test]
    fn failure_classification_separates_kernel_and_reader_gaps() {
        assert_eq!(
            classify_subtask_failure(failure_input(
                "progressive_search",
                true,
                true,
                Some("answer")
            )),
            "passed"
        );
        assert_eq!(
            classify_subtask_failure(failure_input(
                "progressive_search",
                false,
                false,
                Some("answer")
            )),
            "kernel_ref_recall_gap"
        );
        assert_eq!(
            classify_subtask_failure(failure_input(
                "bundled_shopping",
                false,
                true,
                Some("UNKNOWN")
            )),
            "no_prior_answer_evidence"
        );
        assert_eq!(
            classify_subtask_failure(failure_input(
                "group_travel_planner",
                false,
                true,
                Some(r#"[{"days":1}]"#)
            )),
            "domain_agent_solution_gap"
        );
        assert_eq!(
            classify_subtask_failure(failure_input(
                "formal_reasoning_phys",
                false,
                true,
                Some("long evidence context")
            )),
            "formal_reader_reasoning_gap"
        );
    }

    fn failure_input<'a>(
        task_type: &'a str,
        hard_success: bool,
        full_ref_recall: bool,
        ask_answer: Option<&'a str>,
    ) -> FailureClassificationInput<'a> {
        FailureClassificationInput {
            task_type,
            hard_success,
            known_at_clean: true,
            full_ref_recall,
            current_question_observed: true,
            future_answer_leaked: false,
            unexpected_refs: &[],
            ask_answer,
        }
    }

    fn expected_fixture(
        task_type: &str,
        task_id: &str,
        subtask_index: usize,
        answer: &str,
    ) -> MemoryArenaExpected {
        MemoryArenaExpected {
            task_id: task_id.to_string(),
            task_type: task_type.to_string(),
            category: None,
            subtask_index,
            question: format!("question {subtask_index}"),
            answer: json!(answer),
            about: format!("memoryarena:task_type:{task_type}:task:{task_id}"),
            current_question_ref: format!(
                "memoryarena:task_type:{task_type}:task:{task_id}:subtask:{subtask_index}:question"
            ),
            expected_answer_ref: format!(
                "memoryarena:task_type:{task_type}:task:{task_id}:subtask:{subtask_index}:answer"
            ),
            available_ref_ids: vec![format!(
                "memoryarena:task_type:{task_type}:task:{task_id}:subtask:{subtask_index}:question"
            )],
        }
    }

    fn run_fixture(
        task_type: &str,
        task_id: &str,
        subtask_index: usize,
        answer: &str,
    ) -> RunAskResult {
        RunAskResult {
            task_id: task_id.to_string(),
            task_type: task_type.to_string(),
            category: None,
            subtask_index,
            question: format!("question {subtask_index}"),
            allowed_known_at_refs: vec![format!(
                "memoryarena:task_type:{task_type}:task:{task_id}:subtask:{subtask_index}:question"
            )],
            observed_allowed_refs: vec![format!(
                "memoryarena:task_type:{task_type}:task:{task_id}:subtask:{subtask_index}:question"
            )],
            unexpected_refs: Vec::new(),
            missing_allowed_refs: Vec::new(),
            current_question_observed: true,
            future_answer_leaked: false,
            known_at_clean: true,
            ask_answer: Some(answer.to_string()),
            ask_elapsed_ms: 1,
        }
    }

    fn subtask_fixture(
        task_type: &str,
        task_id: &str,
        subtask_index: usize,
        hard_success: bool,
    ) -> SubtaskScoreResult {
        SubtaskScoreResult {
            task_id: task_id.to_string(),
            task_type: task_type.to_string(),
            category: None,
            subtask_index,
            question: format!("question {subtask_index}"),
            expected_answer: json!("answer"),
            expected_answer_kind: "string".to_string(),
            expected_answers: vec!["answer".to_string()],
            candidate_answers: vec!["answer".to_string()],
            chosen_answer: Some("answer".to_string()),
            hard_success,
            candidate_answer_hit: hard_success,
            failure_class: if hard_success {
                "passed"
            } else {
                "reader_reasoning_or_extraction_gap"
            },
            soft_score: None,
            soft_score_basis: None,
            known_at_clean: true,
            full_ref_recall: true,
            current_question_observed: true,
            future_answer_leaked: false,
            unexpected_refs: Vec::new(),
            missing_allowed_refs: Vec::new(),
            allowed_known_at_refs: subtask_index,
            observed_allowed_refs: subtask_index,
            ask_answer_chars: 6,
            ask_elapsed_ms: 1,
        }
    }
}
