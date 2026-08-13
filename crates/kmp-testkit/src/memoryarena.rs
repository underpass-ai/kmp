use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

const ADAPTER_NAME: &str = "memoryarena-adapter";
const DEFAULT_TASK_TYPE: &str = "memoryarena";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryArenaItem {
    pub id: Value,
    pub questions: Vec<String>,
    pub answers: Vec<Value>,
    #[serde(default)]
    pub backgrounds: Value,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub paper_name: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryArenaPreparedTask {
    pub task_id: String,
    pub task_type: String,
    pub category: Option<String>,
    pub about: String,
    pub ingest_events: Vec<MemoryArenaIngestArtifact>,
    pub ask_events: Vec<MemoryArenaAskArtifact>,
    pub expected: Vec<MemoryArenaExpected>,
    pub replay: MemoryArenaReplay,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryArenaIngestArtifact {
    pub tool: &'static str,
    pub task_id: String,
    pub task_type: String,
    pub category: Option<String>,
    pub event_index: usize,
    pub phase: String,
    pub subtask_index: Option<usize>,
    pub about: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryArenaAskArtifact {
    pub tool: &'static str,
    pub task_id: String,
    pub task_type: String,
    pub category: Option<String>,
    pub event_index: usize,
    pub subtask_index: usize,
    pub required_ingest_events: usize,
    pub available_after_event_index: usize,
    pub about: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryArenaExpected {
    pub task_id: String,
    pub task_type: String,
    pub category: Option<String>,
    pub subtask_index: usize,
    pub question: String,
    pub answer: Value,
    pub about: String,
    pub current_question_ref: String,
    pub expected_answer_ref: String,
    pub available_ref_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryArenaReplay {
    pub task_id: String,
    pub task_type: String,
    pub category: Option<String>,
    pub about: String,
    pub timeline: Vec<MemoryArenaReplayEvent>,
    pub known_at_snapshots: Vec<MemoryArenaKnownAtSnapshot>,
    pub final_path_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryArenaReplayEvent {
    pub event_index: usize,
    pub phase: String,
    pub subtask_index: Option<usize>,
    pub ref_id: String,
    pub kind: String,
    pub text: String,
    pub available_before_subtask: Option<usize>,
    pub produced_after_subtask: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryArenaKnownAtSnapshot {
    pub subtask_index: usize,
    pub available_after_event_index: usize,
    pub current_question_ref: String,
    pub expected_answer_ref: String,
    pub available_ref_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryArenaAdapterSummary {
    pub dataset_items: usize,
    pub prepared_tasks: usize,
    pub skipped_tasks: usize,
    pub subtasks: usize,
    pub ingest_events: usize,
    pub ask_events: usize,
    pub replay_events: usize,
    pub background_entries: usize,
    pub categories: BTreeMap<String, usize>,
    pub task_types: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct MemoryArenaAdapterConfig {
    pub task_type: String,
    pub category: Option<String>,
    pub limit: Option<usize>,
    pub run_id: Option<String>,
}

impl Default for MemoryArenaAdapterConfig {
    fn default() -> Self {
        Self {
            task_type: DEFAULT_TASK_TYPE.to_string(),
            category: None,
            limit: None,
            run_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryArenaAdapterError {
    message: String,
}

impl MemoryArenaAdapterError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MemoryArenaAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for MemoryArenaAdapterError {}

pub fn parse_memoryarena_dataset(
    payload: &str,
) -> Result<Vec<MemoryArenaItem>, MemoryArenaAdapterError> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        return serde_json::from_str(trimmed).map_err(|error| {
            MemoryArenaAdapterError::new(format!("invalid MemoryArena JSON array: {error}"))
        });
    }

    let mut items = Vec::new();
    for (line_index, line) in payload.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let item = serde_json::from_str::<MemoryArenaItem>(line).map_err(|error| {
            MemoryArenaAdapterError::new(format!(
                "invalid MemoryArena JSONL at line {}: {error}",
                line_index + 1
            ))
        })?;
        items.push(item);
    }
    Ok(items)
}

pub fn prepare_memoryarena_items(
    items: &[MemoryArenaItem],
    config: &MemoryArenaAdapterConfig,
) -> Result<(Vec<MemoryArenaPreparedTask>, MemoryArenaAdapterSummary), MemoryArenaAdapterError> {
    let mut prepared = Vec::new();
    let mut skipped_tasks = 0usize;

    for item in items {
        if config.category.as_deref().is_some_and(|category| {
            item.category
                .as_deref()
                .is_none_or(|item_category| item_category != category)
        }) {
            skipped_tasks += 1;
            continue;
        }
        if config.limit.is_some_and(|limit| prepared.len() >= limit) {
            skipped_tasks += 1;
            continue;
        }
        prepared.push(prepare_memoryarena_item(item, config)?);
    }

    let summary = summarize_prepared_items(items.len(), skipped_tasks, &prepared);
    Ok((prepared, summary))
}

pub fn prepare_memoryarena_item(
    item: &MemoryArenaItem,
    config: &MemoryArenaAdapterConfig,
) -> Result<MemoryArenaPreparedTask, MemoryArenaAdapterError> {
    validate_item_shape(item)?;

    let task_type = sanitize_ref_segment(&config.task_type);
    if task_type.is_empty() {
        return Err(MemoryArenaAdapterError::new(
            "MemoryArena task_type must not be empty after normalization",
        ));
    }
    let task_id = item_id_string(&item.id)?;
    let task_ref = sanitize_ref_segment(&task_id);
    if task_ref.is_empty() {
        return Err(MemoryArenaAdapterError::new(
            "MemoryArena id must not be empty after normalization",
        ));
    }
    let ref_scope = memoryarena_ref_scope(&task_type, &task_ref, config.run_id.as_deref())?;
    let about = config
        .run_id
        .as_deref()
        .map(|run_id| {
            let run_ref = sanitize_ref_segment(run_id);
            if run_ref.is_empty() {
                return Err(MemoryArenaAdapterError::new(
                    "MemoryArena run_id must not be empty after normalization",
                ));
            }
            Ok(format!(
                "memoryarena:run:{run_ref}:task_type:{task_type}:task:{task_ref}"
            ))
        })
        .transpose()?
        .unwrap_or_else(|| format!("memoryarena:task_type:{task_type}:task:{task_ref}"));
    let category = item.category.clone();
    let fingerprint = memoryarena_item_fingerprint(item)?;
    let background = background_plan_for_item(item);
    let task_scope = format!("memoryarena:task:{ref_scope}");
    let process_scope = format!("memoryarena:process:{ref_scope}");
    let ingest_context = IngestContext {
        about: &about,
        ref_scope: &ref_scope,
        task_id: &task_id,
        task_type: &task_type,
        category: category.as_deref(),
        task_scope: &task_scope,
        process_scope: &process_scope,
        fingerprint: &fingerprint,
    };

    let mut ingest_events = Vec::new();
    let mut ask_events = Vec::new();
    let mut expected = Vec::new();
    let mut timeline = Vec::new();
    let mut snapshots = Vec::new();
    let mut available_refs = Vec::new();
    let mut final_path_refs = Vec::new();
    let mut event_index = 1usize;
    let mut ingest_count = 0usize;
    let mut task_sequence = 1u32;
    let mut base_dimensions_declared = false;
    let mut episode_dimensions_declared = BTreeSet::new();

    if let Some(global_background) = background.global.as_ref() {
        let background_ref = format!("memoryarena:{ref_scope}:background:global");
        let declare_base_dimensions = !base_dimensions_declared;
        base_dimensions_declared = true;
        let ingest = build_ingest(
            &ingest_context,
            IngestPayload {
                episode_scope: None,
                declare_base_dimensions,
                declare_episode_dimension: false,
                entries: vec![entry(
                    &background_ref,
                    "background",
                    global_background,
                    &EntryContext {
                        task_scope: &task_scope,
                        process_scope: &process_scope,
                        episode_scope: None,
                        task_sequence,
                        event_index,
                    },
                    metadata([
                        ("task_id", task_id.as_str()),
                        ("task_type", task_type.as_str()),
                        ("background_scope", "global"),
                    ]),
                )],
                relations: Vec::new(),
                evidence: Vec::new(),
                phase: "initial",
                event_index,
            },
        );
        ingest_events.push(MemoryArenaIngestArtifact {
            tool: "kernel_ingest",
            task_id: task_id.clone(),
            task_type: task_type.clone(),
            category: category.clone(),
            event_index,
            phase: "initial".to_string(),
            subtask_index: None,
            about: about.clone(),
            arguments: ingest,
        });
        timeline.push(replay_event(ReplayEventInput {
            event_index,
            phase: "initial",
            subtask_index: None,
            ref_id: &background_ref,
            kind: "background",
            text: global_background,
            available_before_subtask: Some(1),
            produced_after_subtask: None,
        }));
        available_refs.push(background_ref);
        ingest_count += 1;
        task_sequence = task_sequence.checked_add(1).ok_or_else(sequence_overflow)?;
        event_index += 1;
    }

    for (subtask_index_zero, (question, answer)) in
        item.questions.iter().zip(item.answers.iter()).enumerate()
    {
        let subtask_index = subtask_index_zero + 1;
        let episode_scope = format!("memoryarena:episode:{ref_scope}:{subtask_index}");
        let declare_base_dimensions = !base_dimensions_declared;
        base_dimensions_declared = true;
        let declare_episode_dimension = episode_dimensions_declared.insert(episode_scope.clone());
        let question_ref = memoryarena_question_ref(&ref_scope, subtask_index);
        let answer_ref = memoryarena_answer_ref(&ref_scope, subtask_index);
        let mut pre_entries = Vec::new();
        let mut pre_relations = Vec::new();
        let mut pre_evidence = Vec::new();

        if let Some(background_text) = background
            .per_subtask
            .get(subtask_index_zero)
            .filter(|text| !text.trim().is_empty())
        {
            let background_ref =
                format!("memoryarena:{ref_scope}:subtask:{subtask_index}:background");
            pre_entries.push(entry(
                &background_ref,
                "subtask_background",
                background_text,
                &EntryContext {
                    task_scope: &task_scope,
                    process_scope: &process_scope,
                    episode_scope: Some(&episode_scope),
                    task_sequence,
                    event_index,
                },
                metadata([
                    ("task_id", task_id.as_str()),
                    ("task_type", task_type.as_str()),
                    ("subtask_index", &subtask_index.to_string()),
                    ("background_scope", "subtask"),
                ]),
            ));
            pre_relations.push(json!({
                "from": question_ref,
                "to": background_ref,
                "rel": "uses_background",
                "class": "evidential",
                "why": "The subtask-specific background is available before this question is attempted.",
                "confidence": "high",
                "sequence": task_sequence
            }));
            pre_evidence.push(json!({
                "id": format!("evidence:{ref_scope}:subtask:{subtask_index}:background"),
                "supports": [background_ref, question_ref],
                "text": background_text,
                "source": format!("MemoryArena subtask {subtask_index} background"),
                "metadata": {
                    "task_id": task_id,
                    "task_type": task_type,
                    "subtask_index": subtask_index.to_string()
                }
            }));
            timeline.push(replay_event(ReplayEventInput {
                event_index,
                phase: "pre_subtask",
                subtask_index: Some(subtask_index),
                ref_id: &background_ref,
                kind: "subtask_background",
                text: background_text,
                available_before_subtask: Some(subtask_index),
                produced_after_subtask: None,
            }));
            available_refs.push(background_ref);
            task_sequence = task_sequence.checked_add(1).ok_or_else(sequence_overflow)?;
        }

        pre_entries.push(entry(
            &question_ref,
            "subtask_question",
            question,
            &EntryContext {
                task_scope: &task_scope,
                process_scope: &process_scope,
                episode_scope: Some(&episode_scope),
                task_sequence,
                event_index,
            },
            metadata([
                ("task_id", task_id.as_str()),
                ("task_type", task_type.as_str()),
                ("subtask_index", &subtask_index.to_string()),
            ]),
        ));
        if subtask_index > 1 {
            let previous_answer_ref = memoryarena_answer_ref(&ref_scope, subtask_index - 1);
            pre_relations.push(json!({
                "from": question_ref,
                "to": previous_answer_ref,
                "rel": "follows",
                "class": "procedural",
                "why": "This subtask is attempted after the previous subtask feedback is available.",
                "confidence": "high",
                "sequence": task_sequence
            }));
        }
        timeline.push(replay_event(ReplayEventInput {
            event_index,
            phase: "pre_subtask",
            subtask_index: Some(subtask_index),
            ref_id: &question_ref,
            kind: "subtask_question",
            text: question,
            available_before_subtask: Some(subtask_index),
            produced_after_subtask: None,
        }));
        available_refs.push(question_ref.clone());
        final_path_refs.push(question_ref.clone());

        let pre_ingest = build_ingest(
            &ingest_context,
            IngestPayload {
                episode_scope: Some(&episode_scope),
                declare_base_dimensions,
                declare_episode_dimension,
                entries: pre_entries,
                relations: pre_relations,
                evidence: pre_evidence,
                phase: "pre_subtask",
                event_index,
            },
        );
        ingest_events.push(MemoryArenaIngestArtifact {
            tool: "kernel_ingest",
            task_id: task_id.clone(),
            task_type: task_type.clone(),
            category: category.clone(),
            event_index,
            phase: "pre_subtask".to_string(),
            subtask_index: Some(subtask_index),
            about: about.clone(),
            arguments: pre_ingest,
        });
        ingest_count += 1;
        task_sequence = task_sequence.checked_add(1).ok_or_else(sequence_overflow)?;
        event_index += 1;

        ask_events.push(MemoryArenaAskArtifact {
            tool: "kernel_ask",
            task_id: task_id.clone(),
            task_type: task_type.clone(),
            category: category.clone(),
            event_index,
            subtask_index,
            required_ingest_events: ingest_count,
            available_after_event_index: event_index - 1,
            about: about.clone(),
            arguments: json!({
                "about": about,
                "question": question,
                "answer_policy": "evidence_or_unknown",
                "dimensions": {
                    "scope": "current_about",
                    "mode": "all"
                },
                "metadata": {
                    "benchmark": "MemoryArena",
                    "task_id": task_id,
                    "task_type": task_type,
                    "subtask_index": subtask_index.to_string(),
                    "available_after_event_index": (event_index - 1).to_string()
                }
            }),
        });
        timeline.push(replay_event(ReplayEventInput {
            event_index,
            phase: "ask",
            subtask_index: Some(subtask_index),
            ref_id: &question_ref,
            kind: "subtask_query",
            text: question,
            available_before_subtask: Some(subtask_index),
            produced_after_subtask: None,
        }));
        expected.push(MemoryArenaExpected {
            task_id: task_id.clone(),
            task_type: task_type.clone(),
            category: category.clone(),
            subtask_index,
            question: question.clone(),
            answer: answer.clone(),
            about: about.clone(),
            current_question_ref: question_ref.clone(),
            expected_answer_ref: answer_ref.clone(),
            available_ref_ids: available_refs.clone(),
        });
        snapshots.push(MemoryArenaKnownAtSnapshot {
            subtask_index,
            available_after_event_index: event_index - 1,
            current_question_ref: question_ref.clone(),
            expected_answer_ref: answer_ref.clone(),
            available_ref_ids: available_refs.clone(),
        });
        event_index += 1;

        let answer_text = answer_text(answer);
        let post_ingest = build_ingest(
            &ingest_context,
            IngestPayload {
                episode_scope: Some(&episode_scope),
                declare_base_dimensions: false,
                declare_episode_dimension: false,
                entries: vec![entry(
                    &answer_ref,
                    "subtask_answer_feedback",
                    &answer_text,
                    &EntryContext {
                        task_scope: &task_scope,
                        process_scope: &process_scope,
                        episode_scope: Some(&episode_scope),
                        task_sequence,
                        event_index,
                    },
                    metadata([
                        ("task_id", task_id.as_str()),
                        ("task_type", task_type.as_str()),
                        ("subtask_index", &subtask_index.to_string()),
                        ("answer_feedback", "true"),
                    ]),
                )],
                relations: vec![json!({
                    "from": answer_ref,
                    "to": question_ref,
                    "rel": "answers",
                    "class": "evidential",
                    "why": "MemoryArena provides this answer as environment feedback after the subtask is completed.",
                    "evidence": answer_text,
                    "confidence": "high",
                    "sequence": task_sequence
                })],
                evidence: vec![json!({
                    "id": format!("evidence:{ref_scope}:subtask:{subtask_index}:answer"),
                    "supports": [answer_ref, question_ref],
                    "text": answer_text,
                    "source": format!("MemoryArena subtask {subtask_index} answer"),
                    "metadata": {
                        "task_id": task_id,
                        "task_type": task_type,
                        "subtask_index": subtask_index.to_string()
                    }
                })],
                phase: "post_subtask",
                event_index,
            },
        );
        ingest_events.push(MemoryArenaIngestArtifact {
            tool: "kernel_ingest",
            task_id: task_id.clone(),
            task_type: task_type.clone(),
            category: category.clone(),
            event_index,
            phase: "post_subtask".to_string(),
            subtask_index: Some(subtask_index),
            about: about.clone(),
            arguments: post_ingest,
        });
        timeline.push(replay_event(ReplayEventInput {
            event_index,
            phase: "post_subtask",
            subtask_index: Some(subtask_index),
            ref_id: &answer_ref,
            kind: "subtask_answer_feedback",
            text: &answer_text,
            available_before_subtask: None,
            produced_after_subtask: Some(subtask_index),
        }));
        available_refs.push(answer_ref.clone());
        final_path_refs.push(answer_ref);
        ingest_count += 1;
        task_sequence = task_sequence.checked_add(1).ok_or_else(sequence_overflow)?;
        event_index += 1;
    }

    Ok(MemoryArenaPreparedTask {
        task_id: task_id.clone(),
        task_type: task_type.clone(),
        category: category.clone(),
        about: about.clone(),
        ingest_events,
        ask_events,
        expected,
        replay: MemoryArenaReplay {
            task_id,
            task_type,
            category,
            about,
            timeline,
            known_at_snapshots: snapshots,
            final_path_refs,
        },
    })
}

pub fn memoryarena_question_ref(task_ref_scope: &str, subtask_index: usize) -> String {
    format!("memoryarena:{task_ref_scope}:subtask:{subtask_index}:question")
}

pub fn memoryarena_answer_ref(task_ref_scope: &str, subtask_index: usize) -> String {
    format!("memoryarena:{task_ref_scope}:subtask:{subtask_index}:answer")
}

pub fn memoryarena_ref_scope(
    task_type: &str,
    task_id: &str,
    run_id: Option<&str>,
) -> Result<String, MemoryArenaAdapterError> {
    match run_id {
        Some(run_id) => {
            let run_ref = sanitize_ref_segment(run_id);
            if run_ref.is_empty() {
                return Err(MemoryArenaAdapterError::new(
                    "MemoryArena run_id must not be empty after normalization",
                ));
            }
            Ok(format!(
                "run:{run_ref}:task_type:{task_type}:task:{task_id}"
            ))
        }
        None => Ok(format!("task_type:{task_type}:task:{task_id}")),
    }
}

fn summarize_prepared_items(
    dataset_items: usize,
    skipped_tasks: usize,
    prepared: &[MemoryArenaPreparedTask],
) -> MemoryArenaAdapterSummary {
    let mut categories = BTreeMap::new();
    let mut task_types = BTreeMap::new();
    let mut subtasks = 0usize;
    let mut ingest_events = 0usize;
    let mut ask_events = 0usize;
    let mut replay_events = 0usize;
    let mut background_entries = 0usize;

    for task in prepared {
        *task_types.entry(task.task_type.clone()).or_insert(0usize) += 1;
        *categories
            .entry(
                task.category
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .or_insert(0usize) += 1;
        subtasks += task.expected.len();
        ingest_events += task.ingest_events.len();
        ask_events += task.ask_events.len();
        replay_events += task.replay.timeline.len();
        background_entries += task
            .replay
            .timeline
            .iter()
            .filter(|event| event.kind.contains("background"))
            .count();
    }

    MemoryArenaAdapterSummary {
        dataset_items,
        prepared_tasks: prepared.len(),
        skipped_tasks,
        subtasks,
        ingest_events,
        ask_events,
        replay_events,
        background_entries,
        categories,
        task_types,
    }
}

fn validate_item_shape(item: &MemoryArenaItem) -> Result<(), MemoryArenaAdapterError> {
    if item.id.is_null() {
        return Err(MemoryArenaAdapterError::new(
            "MemoryArena item id must not be null",
        ));
    }
    if item.questions.is_empty() {
        return Err(MemoryArenaAdapterError::new(format!(
            "MemoryArena item {} has no questions",
            item_id_string(&item.id).unwrap_or_else(|_| "<invalid-id>".to_string())
        )));
    }
    if item.answers.len() != item.questions.len() {
        return Err(MemoryArenaAdapterError::new(format!(
            "MemoryArena item {} has {} questions but {} answers",
            item_id_string(&item.id).unwrap_or_else(|_| "<invalid-id>".to_string()),
            item.questions.len(),
            item.answers.len()
        )));
    }
    for (index, question) in item.questions.iter().enumerate() {
        if question.trim().is_empty() {
            return Err(MemoryArenaAdapterError::new(format!(
                "MemoryArena item {} question {} is empty",
                item_id_string(&item.id).unwrap_or_else(|_| "<invalid-id>".to_string()),
                index + 1
            )));
        }
    }
    if let Value::Array(backgrounds) = &item.backgrounds
        && !backgrounds.is_empty()
        && backgrounds.len() != item.questions.len()
    {
        return Err(MemoryArenaAdapterError::new(format!(
            "MemoryArena item {} has {} questions but {} background entries",
            item_id_string(&item.id).unwrap_or_else(|_| "<invalid-id>".to_string()),
            item.questions.len(),
            backgrounds.len()
        )));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct BackgroundPlan {
    global: Option<String>,
    per_subtask: Vec<String>,
}

fn background_plan(value: &Value) -> BackgroundPlan {
    match value {
        Value::String(text) if !text.trim().is_empty() => BackgroundPlan {
            global: Some(text.trim().to_string()),
            per_subtask: Vec::new(),
        },
        Value::Array(values) => BackgroundPlan {
            global: None,
            per_subtask: values
                .iter()
                .map(value_to_text)
                .map(|text| text.trim().to_string())
                .collect(),
        },
        Value::Null => BackgroundPlan {
            global: None,
            per_subtask: Vec::new(),
        },
        other => BackgroundPlan {
            global: Some(value_to_text(other)),
            per_subtask: Vec::new(),
        },
    }
}

fn background_plan_for_item(item: &MemoryArenaItem) -> BackgroundPlan {
    let mut plan = background_plan(&item.backgrounds);
    if let Some(base_person) = item.extra.get("base_person") {
        let base_person_text = value_to_text(base_person).trim().to_string();
        if !base_person_text.is_empty() && base_person_text != "null" {
            let base_person_background =
                format!("MemoryArena group travel base traveler state: {base_person_text}");
            plan.global = match plan.global.take() {
                Some(existing) if !existing.trim().is_empty() => {
                    Some(format!("{existing}\n\n{base_person_background}"))
                }
                _ => Some(base_person_background),
            };
        }
    }
    plan
}

struct IngestContext<'a> {
    about: &'a str,
    ref_scope: &'a str,
    task_id: &'a str,
    task_type: &'a str,
    category: Option<&'a str>,
    task_scope: &'a str,
    process_scope: &'a str,
    fingerprint: &'a str,
}

struct IngestPayload<'a> {
    episode_scope: Option<&'a str>,
    declare_base_dimensions: bool,
    declare_episode_dimension: bool,
    entries: Vec<Value>,
    relations: Vec<Value>,
    evidence: Vec<Value>,
    phase: &'a str,
    event_index: usize,
}

fn build_ingest(context: &IngestContext<'_>, payload: IngestPayload<'_>) -> Value {
    let mut dimensions = Vec::new();
    if payload.declare_base_dimensions {
        dimensions.push(json!({
            "id": context.task_scope,
            "kind": "benchmark_task",
            "title": format!("MemoryArena {} task {}", context.task_type, context.task_id),
            "metadata": {
                "benchmark": "MemoryArena",
                "task_id": context.task_id,
                "task_type": context.task_type,
                "category": context.category.unwrap_or("unknown")
            }
        }));
        dimensions.push(json!({
            "id": context.process_scope,
            "kind": "agentic_process",
            "title": format!("MemoryArena process for task {}", context.task_id),
            "metadata": {
                "benchmark": "MemoryArena",
                "task_id": context.task_id,
                "task_type": context.task_type
            }
        }));
    }
    if payload.declare_episode_dimension
        && let Some(episode_scope) = payload.episode_scope
    {
        dimensions.push(json!({
            "id": episode_scope,
            "kind": "agentic_episode",
            "title": format!("MemoryArena task {} episode", context.task_id),
            "metadata": {
                "benchmark": "MemoryArena",
                "task_id": context.task_id,
                "task_type": context.task_type
            }
        }));
    }

    json!({
        "about": context.about,
        "memory": {
            "dimensions": dimensions,
            "entries": payload.entries,
            "relations": payload.relations,
            "evidence": payload.evidence
        },
        "provenance": {
            "source_kind": "agent",
            "source_agent": ADAPTER_NAME,
            "observed_at": synthetic_observed_at(payload.event_index),
            "correlation_id": format!("memoryarena:{}:{}", context.task_type, context.task_id),
            "causation_id": format!(
                "memoryarena:{}:{}:{}:{}",
                context.task_type, context.task_id, payload.phase, payload.event_index
            )
        },
        "idempotency_key": format!(
            "memoryarena:{}:{}:kmp:v2:{}:{}:{}",
            context.task_type, context.ref_scope, payload.phase, payload.event_index, context.fingerprint
        )
    })
}

struct EntryContext<'a> {
    task_scope: &'a str,
    process_scope: &'a str,
    episode_scope: Option<&'a str>,
    task_sequence: u32,
    event_index: usize,
}

fn entry(id: &str, kind: &str, text: &str, context: &EntryContext<'_>, metadata: Value) -> Value {
    let mut coordinates = vec![
        json!({
            "dimension": "benchmark_task",
            "scope_id": context.task_scope,
            "sequence": context.task_sequence,
            "observed_at": synthetic_observed_at(context.event_index)
        }),
        json!({
            "dimension": "agentic_process",
            "scope_id": context.process_scope,
            "sequence": context.event_index,
            "observed_at": synthetic_observed_at(context.event_index)
        }),
    ];
    if let Some(episode_scope) = context.episode_scope {
        coordinates.push(json!({
            "dimension": "agentic_episode",
            "scope_id": episode_scope,
            "sequence": context.task_sequence,
            "observed_at": synthetic_observed_at(context.event_index)
        }));
    }
    json!({
        "id": id,
        "kind": kind,
        "text": text,
        "coordinates": coordinates,
        "metadata": metadata
    })
}

struct ReplayEventInput<'a> {
    event_index: usize,
    phase: &'a str,
    subtask_index: Option<usize>,
    ref_id: &'a str,
    kind: &'a str,
    text: &'a str,
    available_before_subtask: Option<usize>,
    produced_after_subtask: Option<usize>,
}

fn replay_event(input: ReplayEventInput<'_>) -> MemoryArenaReplayEvent {
    MemoryArenaReplayEvent {
        event_index: input.event_index,
        phase: input.phase.to_string(),
        subtask_index: input.subtask_index,
        ref_id: input.ref_id.to_string(),
        kind: input.kind.to_string(),
        text: input.text.to_string(),
        available_before_subtask: input.available_before_subtask,
        produced_after_subtask: input.produced_after_subtask,
    }
}

fn metadata<'a>(values: impl IntoIterator<Item = (&'static str, &'a str)>) -> Value {
    let mut object = Map::new();
    for (key, value) in values {
        object.insert(key.to_string(), Value::String(value.to_string()));
    }
    Value::Object(object)
}

fn item_id_string(value: &Value) -> Result<String, MemoryArenaAdapterError> {
    match value {
        Value::Number(number) => Ok(number.to_string()),
        Value::String(text) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        other => Err(MemoryArenaAdapterError::new(format!(
            "unsupported MemoryArena id value: {other}"
        ))),
    }
}

fn answer_text(answer: &Value) -> String {
    value_to_text(answer)
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn memoryarena_item_fingerprint(item: &MemoryArenaItem) -> Result<String, MemoryArenaAdapterError> {
    let payload = serde_json::to_vec(item).map_err(|error| {
        MemoryArenaAdapterError::new(format!("failed to serialize MemoryArena item: {error}"))
    })?;
    let digest = Sha256::digest(payload);
    Ok(format!("{digest:x}").chars().take(16).collect())
}

fn synthetic_observed_at(event_index: usize) -> String {
    let minute = event_index % 60;
    let hour = (event_index / 60) % 24;
    format!("2026-01-01T{hour:02}:{minute:02}:00Z")
}

fn sequence_overflow() -> MemoryArenaAdapterError {
    MemoryArenaAdapterError::new("MemoryArena task sequence overflows u32")
}

fn sanitize_ref_segment(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut previous_was_separator = false;
    for ch in input.trim().chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else if matches!(ch, ':' | '_' | '-' | '.') {
            previous_was_separator = false;
            output.push(ch);
            continue;
        } else {
            '-'
        };

        if normalized == '-' {
            if !previous_was_separator {
                output.push(normalized);
            }
            previous_was_separator = true;
        } else {
            output.push(normalized);
            previous_was_separator = false;
        }
    }
    output.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_jsonl_items() {
        let payload = r#"{"id":1,"questions":["q1"],"answers":["a1"],"category":"demo"}"#;
        let items = parse_memoryarena_dataset(payload).expect("jsonl should parse");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].questions, vec!["q1"]);
    }

    #[test]
    fn rejects_mismatched_question_and_answer_counts() {
        let item = MemoryArenaItem {
            id: json!(1),
            questions: vec!["q1".to_string(), "q2".to_string()],
            answers: vec![json!("a1")],
            backgrounds: Value::Null,
            category: None,
            paper_name: None,
            extra: BTreeMap::new(),
        };

        let error = prepare_memoryarena_item(&item, &MemoryArenaAdapterConfig::default())
            .expect_err("mismatched shape should fail");
        assert!(error.to_string().contains("2 questions but 1 answers"));
    }

    #[test]
    fn prepares_staged_kmp_artifacts() {
        let item = MemoryArenaItem {
            id: json!(7),
            questions: vec![
                "Find the first clue.".to_string(),
                "Use the clue to choose the final item.".to_string(),
            ],
            answers: vec![json!("clue-a"), json!({"target": "item-b"})],
            backgrounds: json!("Global task rule."),
            category: Some("progressive".to_string()),
            paper_name: None,
            extra: BTreeMap::new(),
        };

        let prepared = prepare_memoryarena_item(
            &item,
            &MemoryArenaAdapterConfig {
                task_type: "progressive_search".to_string(),
                ..MemoryArenaAdapterConfig::default()
            },
        )
        .expect("task should adapt");

        assert_eq!(
            prepared.about,
            "memoryarena:task_type:progressive_search:task:7"
        );
        assert_eq!(prepared.ingest_events.len(), 5);
        assert_eq!(prepared.ask_events.len(), 2);
        assert_eq!(prepared.expected.len(), 2);
        assert_eq!(prepared.ask_events[0].required_ingest_events, 2);
        assert_eq!(prepared.ask_events[1].required_ingest_events, 4);
        assert_eq!(
            prepared.expected[1].available_ref_ids,
            vec![
                "memoryarena:task_type:progressive_search:task:7:background:global",
                "memoryarena:task_type:progressive_search:task:7:subtask:1:question",
                "memoryarena:task_type:progressive_search:task:7:subtask:1:answer",
                "memoryarena:task_type:progressive_search:task:7:subtask:2:question",
            ]
        );
        assert_eq!(
            prepared.ingest_events[2].arguments["memory"]["relations"][0]["rel"],
            json!("answers")
        );
        assert_eq!(
            prepared
                .ingest_events
                .iter()
                .map(|event| event.arguments["memory"]["dimensions"]
                    .as_array()
                    .expect("dimensions")
                    .len())
                .collect::<Vec<_>>(),
            vec![2, 1, 0, 1, 0]
        );
        assert_eq!(
            prepared.ingest_events[3].arguments["memory"]["relations"][0]["class"],
            json!("procedural")
        );
        assert_eq!(
            prepared.replay.final_path_refs,
            vec![
                "memoryarena:task_type:progressive_search:task:7:subtask:1:question",
                "memoryarena:task_type:progressive_search:task:7:subtask:1:answer",
                "memoryarena:task_type:progressive_search:task:7:subtask:2:question",
                "memoryarena:task_type:progressive_search:task:7:subtask:2:answer",
            ]
        );
    }

    #[test]
    fn run_id_isolates_refs() {
        let item = MemoryArenaItem {
            id: json!("case A"),
            questions: vec!["Question?".to_string()],
            answers: vec![json!("Answer.")],
            backgrounds: Value::Null,
            category: None,
            paper_name: None,
            extra: BTreeMap::new(),
        };

        let prepared = prepare_memoryarena_item(
            &item,
            &MemoryArenaAdapterConfig {
                task_type: "group_travel_planner".to_string(),
                run_id: Some("demo run".to_string()),
                ..MemoryArenaAdapterConfig::default()
            },
        )
        .expect("task should adapt");

        assert_eq!(
            prepared.about,
            "memoryarena:run:demo-run:task_type:group_travel_planner:task:case-a"
        );
        assert!(
            prepared.expected[0]
                .current_question_ref
                .contains("run:demo-run")
        );
        assert!(
            prepared.ingest_events[0].arguments["idempotency_key"]
                .as_str()
                .expect("idempotency key")
                .contains("run:demo-run")
        );
    }

    #[test]
    fn group_travel_base_person_becomes_initial_background() {
        let mut extra = BTreeMap::new();
        extra.insert(
            "base_person".to_string(),
            json!({
                "name": "Jennifer",
                "query": "I am Jennifer.",
                "daily_plans": [
                    {"days": 1, "current_city": "Rockford", "dinner": "Coco Bambu"}
                ]
            }),
        );
        let item = MemoryArenaItem {
            id: json!(1),
            questions: vec!["I am Eric. I am joining Jennifer.".to_string()],
            answers: vec![json!([
                {"days": 1, "current_city": "Rockford", "dinner": "Coco Bambu"}
            ])],
            backgrounds: Value::Null,
            category: None,
            paper_name: None,
            extra,
        };

        let prepared = prepare_memoryarena_item(
            &item,
            &MemoryArenaAdapterConfig {
                task_type: "group_travel_planner".to_string(),
                ..MemoryArenaAdapterConfig::default()
            },
        )
        .expect("task should adapt");

        assert_eq!(prepared.ingest_events[0].phase, "initial");
        assert_eq!(
            prepared.replay.timeline[0].kind, "background",
            "base traveler state must be visible before the first traveler turn"
        );
        assert!(prepared.replay.timeline[0].text.contains("Jennifer"));
        assert!(prepared.expected[0].available_ref_ids[0].contains("background:global"));
    }
}
