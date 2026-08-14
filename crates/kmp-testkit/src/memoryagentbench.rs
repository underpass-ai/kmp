use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

const ADAPTER_NAME: &str = "memoryagentbench-adapter";
const DEFAULT_SPLIT: &str = "Conflict_Resolution";
const GENERIC_CHUNK_TARGET_CHARS: usize = 1_800;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentBenchItem {
    pub context: String,
    pub questions: Vec<String>,
    pub answers: Vec<Value>,
    #[serde(default)]
    pub metadata: MemoryAgentBenchMetadata,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MemoryAgentBenchMetadata {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    pub question_ids: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    pub qa_pair_ids: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    pub question_types: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    pub question_dates: Vec<String>,
    #[serde(default)]
    pub previous_events: Value,
    #[serde(default)]
    pub haystack_sessions: Value,
    #[serde(default)]
    pub keypoints: Value,
    #[serde(default)]
    pub demo: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryAgentBenchPreparedItem {
    pub item_id: String,
    pub split: String,
    pub source: String,
    pub about: String,
    pub ingest_events: Vec<MemoryAgentBenchIngestArtifact>,
    pub ask_events: Vec<MemoryAgentBenchAskArtifact>,
    pub expected: Vec<MemoryAgentBenchExpected>,
    pub replay: MemoryAgentBenchReplay,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryAgentBenchIngestArtifact {
    pub tool: &'static str,
    pub item_id: String,
    pub split: String,
    pub source: String,
    pub event_index: usize,
    pub phase: String,
    pub about: String,
    pub context_entries: usize,
    pub truncated_context_entries: usize,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryAgentBenchAskArtifact {
    pub tool: &'static str,
    pub item_id: String,
    pub split: String,
    pub source: String,
    pub event_index: usize,
    pub query_index: usize,
    pub required_ingest_events: usize,
    pub available_after_event_index: usize,
    pub about: String,
    pub question_id: Option<String>,
    pub qa_pair_id: Option<String>,
    pub question_type: Option<String>,
    pub question_date: Option<String>,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAgentBenchExpected {
    pub item_id: String,
    pub split: String,
    pub source: String,
    pub query_index: usize,
    pub question: String,
    pub answer: Value,
    pub about: String,
    pub question_id: Option<String>,
    pub qa_pair_id: Option<String>,
    pub question_type: Option<String>,
    pub question_date: Option<String>,
    pub available_ref_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryAgentBenchReplay {
    pub item_id: String,
    pub split: String,
    pub source: String,
    pub about: String,
    pub timeline: Vec<MemoryAgentBenchReplayEvent>,
    pub known_at_snapshots: Vec<MemoryAgentBenchKnownAtSnapshot>,
    pub final_context_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryAgentBenchReplayEvent {
    pub event_index: usize,
    pub phase: String,
    pub query_index: Option<usize>,
    pub ref_id: String,
    pub kind: String,
    pub text: String,
    pub sequence: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryAgentBenchKnownAtSnapshot {
    pub query_index: usize,
    pub available_after_event_index: usize,
    pub question_id: Option<String>,
    pub qa_pair_id: Option<String>,
    pub available_ref_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryAgentBenchAdapterSummary {
    pub dataset_items: usize,
    pub prepared_items: usize,
    pub skipped_items: usize,
    pub questions: usize,
    pub ingest_events: usize,
    pub ask_events: usize,
    pub replay_events: usize,
    pub context_entries: usize,
    pub truncated_context_entries: usize,
    pub splits: BTreeMap<String, usize>,
    pub sources: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct MemoryAgentBenchAdapterConfig {
    pub split: String,
    pub source: Option<String>,
    pub limit: Option<usize>,
    pub limit_queries: Option<usize>,
    pub run_id: Option<String>,
    pub max_context_entries: Option<usize>,
}

impl Default for MemoryAgentBenchAdapterConfig {
    fn default() -> Self {
        Self {
            split: DEFAULT_SPLIT.to_string(),
            source: None,
            limit: None,
            limit_queries: None,
            run_id: None,
            max_context_entries: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryAgentBenchAdapterError {
    message: String,
}

impl MemoryAgentBenchAdapterError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MemoryAgentBenchAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for MemoryAgentBenchAdapterError {}

pub fn parse_memoryagentbench_dataset(
    payload: &str,
) -> Result<Vec<MemoryAgentBenchItem>, MemoryAgentBenchAdapterError> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        return serde_json::from_str(trimmed).map_err(|error| {
            MemoryAgentBenchAdapterError::new(format!(
                "invalid MemoryAgentBench JSON array: {error}"
            ))
        });
    }

    let mut items = Vec::new();
    for (line_index, line) in payload.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let item = serde_json::from_str::<MemoryAgentBenchItem>(line).map_err(|error| {
            MemoryAgentBenchAdapterError::new(format!(
                "invalid MemoryAgentBench JSONL at line {}: {error}",
                line_index + 1
            ))
        })?;
        items.push(item);
    }
    Ok(items)
}

pub fn prepare_memoryagentbench_items(
    items: &[MemoryAgentBenchItem],
    config: &MemoryAgentBenchAdapterConfig,
) -> Result<
    (
        Vec<MemoryAgentBenchPreparedItem>,
        MemoryAgentBenchAdapterSummary,
    ),
    MemoryAgentBenchAdapterError,
> {
    let mut prepared = Vec::new();
    let mut skipped_items = 0usize;

    for (item_index_zero, item) in items.iter().enumerate() {
        if config.source.as_deref().is_some_and(|source| {
            item.metadata
                .source
                .as_deref()
                .is_none_or(|item_source| item_source != source)
        }) {
            skipped_items += 1;
            continue;
        }
        if config.limit.is_some_and(|limit| prepared.len() >= limit) {
            skipped_items += 1;
            continue;
        }
        prepared.push(prepare_memoryagentbench_item(
            item,
            item_index_zero + 1,
            config,
        )?);
    }

    let summary = summarize_prepared_items(items.len(), skipped_items, &prepared);
    Ok((prepared, summary))
}

pub fn prepare_memoryagentbench_item(
    item: &MemoryAgentBenchItem,
    item_index: usize,
    config: &MemoryAgentBenchAdapterConfig,
) -> Result<MemoryAgentBenchPreparedItem, MemoryAgentBenchAdapterError> {
    validate_item_shape(item, item_index)?;
    if config.limit_queries == Some(0) {
        return Err(MemoryAgentBenchAdapterError::new(
            "MemoryAgentBench limit_queries must be greater than zero",
        ));
    }

    let split = sanitize_ref_segment(&config.split);
    if split.is_empty() {
        return Err(MemoryAgentBenchAdapterError::new(
            "MemoryAgentBench split must not be empty after normalization",
        ));
    }
    let source = item
        .metadata
        .source
        .as_deref()
        .unwrap_or("unknown")
        .trim()
        .to_string();
    let source_ref = sanitize_ref_segment(&source);
    if source_ref.is_empty() {
        return Err(MemoryAgentBenchAdapterError::new(format!(
            "MemoryAgentBench item {item_index} source must not be empty after normalization"
        )));
    }
    let item_id = item_id_string(item, item_index)?;
    let item_ref = sanitize_ref_segment(&item_id);
    if item_ref.is_empty() {
        return Err(MemoryAgentBenchAdapterError::new(format!(
            "MemoryAgentBench item {item_index} id must not be empty after normalization"
        )));
    }
    let ref_scope =
        memoryagentbench_ref_scope(&split, &source_ref, &item_ref, config.run_id.as_deref())?;
    let about = config
        .run_id
        .as_deref()
        .map(|run_id| {
            let run_ref = sanitize_ref_segment(run_id);
            if run_ref.is_empty() {
                return Err(MemoryAgentBenchAdapterError::new(
                    "MemoryAgentBench run_id must not be empty after normalization",
                ));
            }
            Ok(format!(
                "memoryagentbench:run:{run_ref}:split:{split}:source:{source_ref}:item:{item_ref}"
            ))
        })
        .transpose()?
        .unwrap_or_else(|| {
            format!("memoryagentbench:split:{split}:source:{source_ref}:item:{item_ref}")
        });
    let fingerprint = memoryagentbench_item_fingerprint(item)?;
    let split_scope = format!("memoryagentbench:split:{split}:source:{source_ref}");
    let item_scope = format!("memoryagentbench:item:{ref_scope}");
    let context_scope = format!("memoryagentbench:context:{ref_scope}");
    let context_entries =
        context_entry_plan(&item.context, &ref_scope, config.max_context_entries)?;
    let all_ref_ids = context_entries
        .entries
        .iter()
        .map(|entry| entry.ref_id.clone())
        .collect::<Vec<_>>();

    let ingest_context = IngestContext {
        about: &about,
        ref_scope: &ref_scope,
        item_id: &item_id,
        split: &split,
        source: &source,
        source_ref: &source_ref,
        split_scope: &split_scope,
        item_scope: &item_scope,
        context_scope: &context_scope,
        fingerprint: &fingerprint,
        truncated_context_entries: context_entries.truncated,
    };

    let ingest = build_ingest(&ingest_context, &context_entries.entries);
    let ingest_events = vec![MemoryAgentBenchIngestArtifact {
        tool: "kernel_ingest",
        item_id: item_id.clone(),
        split: split.clone(),
        source: source.clone(),
        event_index: 1,
        phase: "inject_context".to_string(),
        about: about.clone(),
        context_entries: context_entries.entries.len(),
        truncated_context_entries: context_entries.truncated,
        arguments: ingest,
    }];

    let mut ask_events = Vec::new();
    let mut expected = Vec::new();
    let mut snapshots = Vec::new();
    let mut timeline = Vec::new();

    for entry in &context_entries.entries {
        timeline.push(MemoryAgentBenchReplayEvent {
            event_index: 1,
            phase: "inject_context".to_string(),
            query_index: None,
            ref_id: entry.ref_id.clone(),
            kind: entry.kind.clone(),
            text: entry.text.clone(),
            sequence: entry.sequence,
        });
    }

    let query_limit = config.limit_queries.unwrap_or(item.questions.len());
    for (query_index_zero, (question, answer)) in item
        .questions
        .iter()
        .zip(item.answers.iter())
        .take(query_limit)
        .enumerate()
    {
        let query_index = query_index_zero + 1;
        let event_index = query_index + 1;
        let question_id = optional_metadata_value(&item.metadata.question_ids, query_index_zero);
        let qa_pair_id = optional_metadata_value(&item.metadata.qa_pair_ids, query_index_zero);
        let question_type =
            optional_metadata_value(&item.metadata.question_types, query_index_zero);
        let question_date =
            optional_metadata_value(&item.metadata.question_dates, query_index_zero);

        ask_events.push(MemoryAgentBenchAskArtifact {
            tool: "kernel_ask",
            item_id: item_id.clone(),
            split: split.clone(),
            source: source.clone(),
            event_index,
            query_index,
            required_ingest_events: 1,
            available_after_event_index: 1,
            about: about.clone(),
            question_id: question_id.clone(),
            qa_pair_id: qa_pair_id.clone(),
            question_type: question_type.clone(),
            question_date: question_date.clone(),
            arguments: json!({
                "about": about,
                "question": question,
                "answer_policy": "evidence_or_unknown",
                "dimensions": {
                    "scope": "current_about",
                    "mode": "all"
                },
                "metadata": {
                    "benchmark": "MemoryAgentBench",
                    "split": split,
                    "source": source,
                    "item_id": item_id,
                    "query_index": query_index.to_string(),
                    "question_id": question_id.as_deref().unwrap_or_default(),
                    "qa_pair_id": qa_pair_id.as_deref().unwrap_or_default(),
                    "question_type": question_type.as_deref().unwrap_or_default(),
                    "question_date": question_date.as_deref().unwrap_or_default(),
                    "available_after_event_index": "1"
                }
            }),
        });
        expected.push(MemoryAgentBenchExpected {
            item_id: item_id.clone(),
            split: split.clone(),
            source: source.clone(),
            query_index,
            question: question.clone(),
            answer: answer.clone(),
            about: about.clone(),
            question_id: question_id.clone(),
            qa_pair_id: qa_pair_id.clone(),
            question_type: question_type.clone(),
            question_date: question_date.clone(),
            available_ref_ids: all_ref_ids.clone(),
        });
        snapshots.push(MemoryAgentBenchKnownAtSnapshot {
            query_index,
            available_after_event_index: 1,
            question_id,
            qa_pair_id,
            available_ref_ids: all_ref_ids.clone(),
        });
    }

    Ok(MemoryAgentBenchPreparedItem {
        item_id: item_id.clone(),
        split: split.clone(),
        source: source.clone(),
        about: about.clone(),
        ingest_events,
        ask_events,
        expected,
        replay: MemoryAgentBenchReplay {
            item_id,
            split,
            source,
            about,
            timeline,
            known_at_snapshots: snapshots,
            final_context_refs: all_ref_ids,
        },
    })
}

pub fn memoryagentbench_ref_scope(
    split: &str,
    source: &str,
    item_id: &str,
    run_id: Option<&str>,
) -> Result<String, MemoryAgentBenchAdapterError> {
    match run_id {
        Some(run_id) => {
            let run_ref = sanitize_ref_segment(run_id);
            if run_ref.is_empty() {
                return Err(MemoryAgentBenchAdapterError::new(
                    "MemoryAgentBench run_id must not be empty after normalization",
                ));
            }
            Ok(format!(
                "run:{run_ref}:split:{split}:source:{source}:item:{item_id}"
            ))
        }
        None => Ok(format!("split:{split}:source:{source}:item:{item_id}")),
    }
}

fn summarize_prepared_items(
    dataset_items: usize,
    skipped_items: usize,
    prepared: &[MemoryAgentBenchPreparedItem],
) -> MemoryAgentBenchAdapterSummary {
    let mut splits = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let mut questions = 0usize;
    let mut ingest_events = 0usize;
    let mut ask_events = 0usize;
    let mut replay_events = 0usize;
    let mut context_entries = 0usize;
    let mut truncated_context_entries = 0usize;

    for item in prepared {
        *splits.entry(item.split.clone()).or_insert(0usize) += 1;
        *sources.entry(item.source.clone()).or_insert(0usize) += 1;
        questions += item.expected.len();
        ingest_events += item.ingest_events.len();
        ask_events += item.ask_events.len();
        replay_events += item.replay.timeline.len();
        context_entries += item.replay.timeline.len();
        truncated_context_entries += item
            .ingest_events
            .iter()
            .map(|event| event.truncated_context_entries)
            .sum::<usize>();
    }

    MemoryAgentBenchAdapterSummary {
        dataset_items,
        prepared_items: prepared.len(),
        skipped_items,
        questions,
        ingest_events,
        ask_events,
        replay_events,
        context_entries,
        truncated_context_entries,
        splits,
        sources,
    }
}

fn validate_item_shape(
    item: &MemoryAgentBenchItem,
    item_index: usize,
) -> Result<(), MemoryAgentBenchAdapterError> {
    if item.context.trim().is_empty() {
        return Err(MemoryAgentBenchAdapterError::new(format!(
            "MemoryAgentBench item {item_index} has empty context"
        )));
    }
    if item.questions.is_empty() {
        return Err(MemoryAgentBenchAdapterError::new(format!(
            "MemoryAgentBench item {item_index} has no questions"
        )));
    }
    if item.answers.len() != item.questions.len() {
        return Err(MemoryAgentBenchAdapterError::new(format!(
            "MemoryAgentBench item {item_index} has {} questions but {} answers",
            item.questions.len(),
            item.answers.len()
        )));
    }
    for (index, question) in item.questions.iter().enumerate() {
        if question.trim().is_empty() {
            return Err(MemoryAgentBenchAdapterError::new(format!(
                "MemoryAgentBench item {item_index} question {} is empty",
                index + 1
            )));
        }
    }
    validate_metadata_vector(
        item_index,
        "question_ids",
        item.questions.len(),
        &item.metadata.question_ids,
    )?;
    validate_metadata_vector(
        item_index,
        "qa_pair_ids",
        item.questions.len(),
        &item.metadata.qa_pair_ids,
    )?;
    validate_metadata_vector(
        item_index,
        "question_types",
        item.questions.len(),
        &item.metadata.question_types,
    )?;
    validate_metadata_vector(
        item_index,
        "question_dates",
        item.questions.len(),
        &item.metadata.question_dates,
    )?;
    Ok(())
}

fn validate_metadata_vector(
    item_index: usize,
    field: &str,
    questions: usize,
    values: &[String],
) -> Result<(), MemoryAgentBenchAdapterError> {
    if !values.is_empty() && values.len() != questions {
        return Err(MemoryAgentBenchAdapterError::new(format!(
            "MemoryAgentBench item {item_index} has {questions} questions but {} metadata.{field} entries",
            values.len()
        )));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ContextEntryPlan {
    entries: Vec<ContextEntry>,
    truncated: usize,
}

#[derive(Debug, Clone)]
struct ContextEntry {
    ref_id: String,
    kind: String,
    text: String,
    sequence: u32,
    metadata: Value,
}

fn context_entry_plan(
    context: &str,
    ref_scope: &str,
    max_context_entries: Option<usize>,
) -> Result<ContextEntryPlan, MemoryAgentBenchAdapterError> {
    let mut raw_entries = fact_line_entries(context);
    if raw_entries.is_empty() {
        raw_entries = paragraph_entries(context);
    }
    if raw_entries.is_empty() {
        return Err(MemoryAgentBenchAdapterError::new(
            "MemoryAgentBench context produced no entries after normalization",
        ));
    }

    let original_len = raw_entries.len();
    if let Some(max_entries) = max_context_entries {
        if max_entries == 0 {
            return Err(MemoryAgentBenchAdapterError::new(
                "MemoryAgentBench max_context_entries must be greater than zero",
            ));
        }
        raw_entries.truncate(max_entries);
    }
    let truncated = original_len.saturating_sub(raw_entries.len());

    let mut entries = Vec::with_capacity(raw_entries.len());
    for (index_zero, raw) in raw_entries.into_iter().enumerate() {
        let sequence = u32::try_from(index_zero + 1).map_err(|_| sequence_overflow())?;
        let ref_id = match raw.suffix {
            Some(suffix) => format!("memoryagentbench:{ref_scope}:context:{suffix}"),
            None => format!("memoryagentbench:{ref_scope}:context:chunk:{sequence}"),
        };
        let mut metadata = Map::new();
        metadata.insert(
            "benchmark".to_string(),
            Value::String("MemoryAgentBench".to_string()),
        );
        metadata.insert("context_kind".to_string(), Value::String(raw.kind.clone()));
        metadata.insert("sequence".to_string(), Value::String(sequence.to_string()));
        if let Some(serial) = raw.serial {
            metadata.insert(
                "serial_number".to_string(),
                Value::String(serial.to_string()),
            );
        }
        entries.push(ContextEntry {
            ref_id,
            kind: raw.kind,
            text: raw.text,
            sequence,
            metadata: Value::Object(metadata),
        });
    }
    Ok(ContextEntryPlan { entries, truncated })
}

#[derive(Debug, Clone)]
struct RawContextEntry {
    suffix: Option<String>,
    kind: String,
    text: String,
    serial: Option<u64>,
}

fn fact_line_entries(context: &str) -> Vec<RawContextEntry> {
    let mut entries = Vec::new();
    for line in context
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some((serial, text)) = parse_fact_line(line) {
            entries.push(RawContextEntry {
                suffix: Some(format!("fact:{serial}")),
                kind: "context_fact".to_string(),
                text: text.to_string(),
                serial: Some(serial),
            });
        }
    }
    if entries.len() >= 2 {
        entries
    } else {
        Vec::new()
    }
}

fn parse_fact_line(line: &str) -> Option<(u64, &str)> {
    let (serial, rest) = line.split_once('.')?;
    let serial = serial.trim();
    if serial.is_empty() || !serial.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let text = rest.trim();
    if text.is_empty() {
        return None;
    }
    Some((serial.parse().ok()?, line))
}

fn paragraph_entries(context: &str) -> Vec<RawContextEntry> {
    let paragraph_chunks = context
        .split("\n\n")
        .map(str::trim)
        .filter(|chunk| !chunk.is_empty())
        .collect::<Vec<_>>();
    let chunks = if paragraph_chunks.len() >= 2 {
        paragraph_chunks
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    } else {
        generic_text_chunks(context)
    };

    chunks
        .into_iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .map(|chunk| RawContextEntry {
            suffix: None,
            kind: "context_chunk".to_string(),
            text: chunk.trim().to_string(),
            serial: None,
        })
        .collect()
}

fn generic_text_chunks(context: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for word in context.split_whitespace() {
        if current.len() + word.len() + 1 > GENERIC_CHUNK_TARGET_CHARS && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

struct IngestContext<'a> {
    about: &'a str,
    ref_scope: &'a str,
    item_id: &'a str,
    split: &'a str,
    source: &'a str,
    source_ref: &'a str,
    split_scope: &'a str,
    item_scope: &'a str,
    context_scope: &'a str,
    fingerprint: &'a str,
    truncated_context_entries: usize,
}

fn build_ingest(context: &IngestContext<'_>, entries: &[ContextEntry]) -> Value {
    let dimensions = vec![
        json!({
            "id": context.split_scope,
            "kind": "benchmark_split",
            "title": format!("MemoryAgentBench {} / {}", context.split, context.source),
            "metadata": {
                "benchmark": "MemoryAgentBench",
                "split": context.split,
                "source": context.source
            }
        }),
        json!({
            "id": context.item_scope,
            "kind": "benchmark_item",
            "title": format!("MemoryAgentBench item {}", context.item_id),
            "metadata": {
                "benchmark": "MemoryAgentBench",
                "split": context.split,
                "source": context.source,
                "item_id": context.item_id
            }
        }),
        json!({
            "id": context.context_scope,
            "kind": "memory_context",
            "title": format!("MemoryAgentBench context for item {}", context.item_id),
            "metadata": {
                "benchmark": "MemoryAgentBench",
                "split": context.split,
                "source": context.source,
                "item_id": context.item_id,
                "truncated_context_entries": context.truncated_context_entries.to_string()
            }
        }),
    ];

    let memory_entries = entries
        .iter()
        .map(|entry| context_entry(entry, context))
        .collect::<Vec<_>>();
    let relations = entries
        .windows(2)
        .map(|window| {
            let previous = &window[0];
            let current = &window[1];
            json!({
                "from": current.ref_id,
                "to": previous.ref_id,
                "rel": "follows",
                "class": "procedural",
                "why": "MemoryAgentBench context order preserves the injected knowledge sequence.",
                "confidence": "high",
                "sequence": current.sequence
            })
        })
        .collect::<Vec<_>>();
    let evidence = entries
        .iter()
        .map(|entry| {
            json!({
                "id": format!("evidence:{}:{}", context.ref_scope, entry.sequence),
                "supports": [entry.ref_id],
                "text": entry.text,
                "source": format!(
                    "MemoryAgentBench {} {} item {} context entry {}",
                    context.split, context.source, context.item_id, entry.sequence
                ),
                "metadata": {
                    "benchmark": "MemoryAgentBench",
                    "split": context.split,
                    "source": context.source,
                    "item_id": context.item_id,
                    "sequence": entry.sequence.to_string()
                }
            })
        })
        .collect::<Vec<_>>();

    json!({
        "about": context.about,
        "memory": {
            "dimensions": dimensions,
            "entries": memory_entries,
            "relations": relations,
            "evidence": evidence
        },
        "provenance": {
            "source_kind": "agent",
            "source_agent": ADAPTER_NAME,
            "observed_at": synthetic_observed_at(1),
            "correlation_id": format!(
                "memoryagentbench:{}:{}:{}",
                context.split, context.source_ref, context.item_id
            ),
            "causation_id": format!(
                "memoryagentbench:{}:{}:{}:inject-context",
                context.split, context.source_ref, context.item_id
            )
        },
        "idempotency_key": format!(
            "memoryagentbench:{}:{}:kmp:v1:inject-context:{}",
            context.split, context.ref_scope, context.fingerprint
        )
    })
}

fn context_entry(entry: &ContextEntry, context: &IngestContext<'_>) -> Value {
    json!({
        "id": entry.ref_id,
        "kind": entry.kind,
        "text": entry.text,
        "coordinates": [
            {
                "dimension": "benchmark_split",
                "scope_id": context.split_scope,
                "sequence": 1,
                "observed_at": synthetic_observed_at(1)
            },
            {
                "dimension": "benchmark_item",
                "scope_id": context.item_scope,
                "sequence": entry.sequence,
                "observed_at": synthetic_observed_at(entry.sequence as usize)
            },
            {
                "dimension": "memory_context",
                "scope_id": context.context_scope,
                "sequence": entry.sequence,
                "observed_at": synthetic_observed_at(entry.sequence as usize)
            }
        ],
        "metadata": entry.metadata
    })
}

fn item_id_string(
    item: &MemoryAgentBenchItem,
    item_index: usize,
) -> Result<String, MemoryAgentBenchAdapterError> {
    if let Some(id) = item.extra.get("id") {
        return value_id_string(id).map_err(|error| {
            MemoryAgentBenchAdapterError::new(format!(
                "unsupported MemoryAgentBench item id at index {item_index}: {error}"
            ))
        });
    }
    if let Some(qa_pair_id) = item.metadata.qa_pair_ids.first() {
        let normalized = qa_pair_id.trim();
        if !normalized.is_empty() {
            return Ok(normalized.to_string());
        }
    }
    if let Some(question_id) = item.metadata.question_ids.first() {
        let normalized = question_id.trim();
        if !normalized.is_empty() {
            return Ok(normalized.to_string());
        }
    }
    Ok(format!("item-{item_index}"))
}

fn value_id_string(value: &Value) -> Result<String, &'static str> {
    match value {
        Value::Number(number) => Ok(number.to_string()),
        Value::String(text) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        _ => Err("expected non-empty string or number"),
    }
}

fn optional_metadata_value(values: &[String], index_zero: usize) -> Option<String> {
    values
        .get(index_zero)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn memoryagentbench_item_fingerprint(
    item: &MemoryAgentBenchItem,
) -> Result<String, MemoryAgentBenchAdapterError> {
    let payload = serde_json::to_vec(item).map_err(|error| {
        MemoryAgentBenchAdapterError::new(format!(
            "failed to serialize MemoryAgentBench item: {error}"
        ))
    })?;
    let digest = Sha256::digest(payload);
    Ok(format!("{digest:x}").chars().take(16).collect())
}

fn synthetic_observed_at(sequence: usize) -> String {
    let minute = sequence % 60;
    let hour = (sequence / 60) % 24;
    format!("2026-01-01T{hour:02}:{minute:02}:00Z")
}

fn sequence_overflow() -> MemoryAgentBenchAdapterError {
    MemoryAgentBenchAdapterError::new("MemoryAgentBench context sequence overflows u32")
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

fn deserialize_string_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Null => Vec::new(),
        Value::String(text) => vec![text],
        Value::Array(values) => values
            .into_iter()
            .map(|value| match value {
                Value::String(text) => text,
                other => other.to_string(),
            })
            .collect(),
        other => vec![other.to_string()],
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_jsonl_items() {
        let payload = r#"{"context":"0. Alice set the key to blue.\n1. Alice later set the key to green.","questions":["What is the latest key color?"],"answers":[["green"]],"metadata":{"source":"factconsolidation_mh_32k","qa_pair_ids":["qa-1"]}}"#;
        let items = parse_memoryagentbench_dataset(payload).expect("jsonl should parse");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].questions, vec!["What is the latest key color?"]);
        assert_eq!(
            items[0].metadata.source.as_deref(),
            Some("factconsolidation_mh_32k")
        );
    }

    #[test]
    fn rejects_mismatched_question_and_answer_counts() {
        let item = MemoryAgentBenchItem {
            context: "Context.".to_string(),
            questions: vec!["q1".to_string(), "q2".to_string()],
            answers: vec![json!(["a1"])],
            metadata: MemoryAgentBenchMetadata::default(),
            extra: BTreeMap::new(),
        };

        let error =
            prepare_memoryagentbench_item(&item, 1, &MemoryAgentBenchAdapterConfig::default())
                .expect_err("mismatched shape should fail");
        assert!(error.to_string().contains("2 questions but 1 answers"));
    }

    #[test]
    fn prepares_inject_once_query_many_kmp_artifacts() {
        let item = MemoryAgentBenchItem {
            context: "0. The API gateway timeout was 30 seconds.\n1. The API gateway timeout was raised to 45 seconds.\n2. The mobile client retries twice.".to_string(),
            questions: vec![
                "What is the latest API gateway timeout?".to_string(),
                "How many times does the mobile client retry?".to_string(),
            ],
            answers: vec![json!(["45 seconds"]), json!(["twice"])],
            metadata: MemoryAgentBenchMetadata {
                source: Some("factconsolidation_mh_32k".to_string()),
                qa_pair_ids: vec!["qa-timeout".to_string(), "qa-retry".to_string()],
                question_ids: vec!["q-timeout".to_string(), "q-retry".to_string()],
                question_types: vec!["conflict_resolution".to_string(), "retrieval".to_string()],
                question_dates: Vec::new(),
                previous_events: Value::Null,
                haystack_sessions: Value::Null,
                keypoints: Value::Null,
                demo: Value::Null,
            },
            extra: BTreeMap::new(),
        };

        let prepared = prepare_memoryagentbench_item(
            &item,
            1,
            &MemoryAgentBenchAdapterConfig {
                split: "Conflict_Resolution".to_string(),
                run_id: Some("demo run".to_string()),
                ..MemoryAgentBenchAdapterConfig::default()
            },
        )
        .expect("item should adapt");

        assert_eq!(
            prepared.about,
            "memoryagentbench:run:demo-run:split:conflict_resolution:source:factconsolidation_mh_32k:item:qa-timeout"
        );
        assert_eq!(prepared.ingest_events.len(), 1);
        assert_eq!(prepared.ingest_events[0].context_entries, 3);
        assert_eq!(prepared.ask_events.len(), 2);
        assert_eq!(prepared.expected.len(), 2);
        assert_eq!(prepared.ask_events[1].required_ingest_events, 1);
        assert_eq!(
            prepared.ingest_events[0].arguments["memory"]["dimensions"]
                .as_array()
                .expect("dimensions")
                .len(),
            3
        );
        assert_eq!(
            prepared.ingest_events[0].arguments["memory"]["relations"][0]["rel"],
            json!("follows")
        );
        assert_eq!(prepared.expected[0].available_ref_ids.len(), 3);
        assert_eq!(
            prepared.expected[0].available_ref_ids[1],
            "memoryagentbench:run:demo-run:split:conflict_resolution:source:factconsolidation_mh_32k:item:qa-timeout:context:fact:1"
        );
    }

    #[test]
    fn limit_queries_keeps_inject_once_context_and_bounds_asks() {
        let item = MemoryAgentBenchItem {
            context: "0. A first fact.\n1. A second fact.".to_string(),
            questions: vec![
                "Question 1?".to_string(),
                "Question 2?".to_string(),
                "Question 3?".to_string(),
            ],
            answers: vec![json!(["A1"]), json!(["A2"]), json!(["A3"])],
            metadata: MemoryAgentBenchMetadata {
                source: Some("factconsolidation_mh_6k".to_string()),
                qa_pair_ids: vec!["qa-1".to_string(), "qa-2".to_string(), "qa-3".to_string()],
                ..MemoryAgentBenchMetadata::default()
            },
            extra: BTreeMap::new(),
        };

        let prepared = prepare_memoryagentbench_item(
            &item,
            1,
            &MemoryAgentBenchAdapterConfig {
                limit_queries: Some(2),
                ..MemoryAgentBenchAdapterConfig::default()
            },
        )
        .expect("item should adapt");

        assert_eq!(prepared.ingest_events.len(), 1);
        assert_eq!(prepared.ask_events.len(), 2);
        assert_eq!(prepared.expected.len(), 2);
        assert_eq!(
            prepared.replay.known_at_snapshots[1].available_ref_ids,
            prepared.replay.final_context_refs
        );
    }

    #[test]
    fn source_filter_is_applied_by_prepare_items() {
        let payload = r#"
{"context":"A context.","questions":["Q?"],"answers":[["A"]],"metadata":{"source":"eventqa"}}
{"context":"B context.","questions":["Q?"],"answers":[["B"]],"metadata":{"source":"factconsolidation_mh_32k"}}
"#;
        let items = parse_memoryagentbench_dataset(payload).expect("dataset should parse");
        let (prepared, summary) = prepare_memoryagentbench_items(
            &items,
            &MemoryAgentBenchAdapterConfig {
                source: Some("factconsolidation_mh_32k".to_string()),
                ..MemoryAgentBenchAdapterConfig::default()
            },
        )
        .expect("items should adapt");

        assert_eq!(prepared.len(), 1);
        assert_eq!(summary.dataset_items, 2);
        assert_eq!(summary.prepared_items, 1);
        assert_eq!(summary.skipped_items, 1);
        assert_eq!(prepared[0].source, "factconsolidation_mh_32k");
    }
}
