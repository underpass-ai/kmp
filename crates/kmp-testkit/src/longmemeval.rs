use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const ADAPTER_NAME: &str = "longmemeval-adapter";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LongMemEvalItem {
    pub question_id: String,
    pub question_type: String,
    pub question: String,
    pub answer: Value,
    pub question_date: String,
    pub haystack_session_ids: Vec<String>,
    pub haystack_dates: Vec<String>,
    pub haystack_sessions: Vec<Vec<LongMemEvalTurn>>,
    #[serde(default)]
    pub answer_session_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LongMemEvalTurn {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub has_answer: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LongMemEvalCandidateTurn {
    pub turn_ref: String,
    pub role: String,
    pub content: String,
    pub session_id: String,
    pub session_date: String,
    pub one_based_turn_index: usize,
    pub has_answer: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongMemEvalPreparedItem {
    pub question_id: String,
    pub question_type: String,
    pub about: String,
    pub ingest: Value,
    pub ask: Value,
    pub expected: LongMemEvalExpected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongMemEvalExpected {
    pub question_id: String,
    pub question_type: String,
    pub answer: Value,
    pub question: String,
    pub question_date: String,
    pub answer_session_ids: Vec<String>,
    pub answer_turn_refs: Vec<String>,
    pub answer_session_refs: Vec<String>,
    pub abstention: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongMemEvalEvidenceLabels {
    pub question_id: String,
    pub evidence_turns: Vec<LongMemEvalEvidenceTurnLabel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongMemEvalEvidenceTurnLabel {
    pub turn_ref: String,
    pub reason: String,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongMemEvalAdapterSummary {
    pub dataset_items: usize,
    pub prepared_items: usize,
    pub skipped_items: usize,
    pub sessions: usize,
    pub turns: usize,
    pub expected_evidence_turns: usize,
    pub relation_evidence_turns: usize,
    pub abstention_items: usize,
    pub question_types: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct LongMemEvalAdapterConfig {
    pub limit: Option<usize>,
    pub per_question_type_limit: Option<usize>,
    pub question_type: Option<String>,
    pub include_abstention: bool,
    pub strict_temporal: bool,
    pub run_id: Option<String>,
    pub generated_evidence: Option<BTreeMap<String, LongMemEvalEvidenceLabels>>,
}

impl Default for LongMemEvalAdapterConfig {
    fn default() -> Self {
        Self {
            limit: None,
            per_question_type_limit: None,
            question_type: None,
            include_abstention: true,
            strict_temporal: true,
            run_id: None,
            generated_evidence: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongMemEvalAdapterError {
    message: String,
}

impl LongMemEvalAdapterError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for LongMemEvalAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for LongMemEvalAdapterError {}

pub fn parse_longmemeval_dataset(
    payload: &str,
) -> Result<Vec<LongMemEvalItem>, LongMemEvalAdapterError> {
    serde_json::from_str(payload)
        .map_err(|error| LongMemEvalAdapterError::new(format!("invalid LongMemEval JSON: {error}")))
}

pub fn prepare_longmemeval_items(
    items: &[LongMemEvalItem],
    config: &LongMemEvalAdapterConfig,
) -> Result<(Vec<LongMemEvalPreparedItem>, LongMemEvalAdapterSummary), LongMemEvalAdapterError> {
    let mut prepared = Vec::new();
    let mut skipped_items = 0usize;
    let mut prepared_by_question_type = BTreeMap::<String, usize>::new();

    for item in items {
        if config
            .question_type
            .as_deref()
            .is_some_and(|question_type| item.question_type != question_type)
        {
            skipped_items += 1;
            continue;
        }
        if is_abstention(item) && !config.include_abstention {
            skipped_items += 1;
            continue;
        }
        if config.limit.is_some_and(|limit| prepared.len() >= limit) {
            skipped_items += 1;
            continue;
        }
        if config.per_question_type_limit.is_some_and(|limit| {
            prepared_by_question_type
                .get(&item.question_type)
                .copied()
                .unwrap_or_default()
                >= limit
        }) {
            skipped_items += 1;
            continue;
        }
        prepared.push(prepare_longmemeval_item(item, config)?);
        *prepared_by_question_type
            .entry(item.question_type.clone())
            .or_insert(0) += 1;
    }

    let summary = summarize_prepared_items(items.len(), skipped_items, &prepared);
    Ok((prepared, summary))
}

pub fn prepare_longmemeval_item(
    item: &LongMemEvalItem,
    config: &LongMemEvalAdapterConfig,
) -> Result<LongMemEvalPreparedItem, LongMemEvalAdapterError> {
    validate_item_shape(item)?;

    let question_id = sanitize_ref_segment(&item.question_id);
    let ref_scope = longmemeval_ref_scope(&item.question_id, config.run_id.as_deref())?;
    let about = config
        .run_id
        .as_deref()
        .map(|run_id| {
            let run_ref = sanitize_ref_segment(run_id);
            if run_ref.is_empty() {
                return Err(LongMemEvalAdapterError::new(
                    "LongMemEval run_id must not be empty after normalization",
                ));
            }
            Ok(format!("longmemeval:run:{run_ref}:item:{question_id}"))
        })
        .transpose()?
        .unwrap_or_else(|| format!("longmemeval:item:{question_id}"));
    let benchmark_scope = about.clone();
    let question_ref = format!("question:{ref_scope}");
    let question_scope = format!("longmemeval:question:{ref_scope}");
    let evidence_scope = format!("longmemeval:evidence:{ref_scope}");
    let item_fingerprint = longmemeval_item_fingerprint(item)?;
    let generated_labels = config
        .generated_evidence
        .as_ref()
        .map(|evidence| {
            evidence.get(&item.question_id).ok_or_else(|| {
                LongMemEvalAdapterError::new(format!(
                    "missing generated evidence labels for question {}",
                    item.question_id
                ))
            })
        })
        .transpose()?;
    if let Some(labels) = generated_labels
        && labels.question_id != item.question_id
    {
        return Err(LongMemEvalAdapterError::new(format!(
            "generated evidence label question_id mismatch: item={} labels={}",
            item.question_id, labels.question_id
        )));
    }
    let evidence_fingerprint = generated_labels
        .map(generated_evidence_fingerprint)
        .transpose()?
        .unwrap_or_else(|| "oracle".to_string());
    let question_time = normalize_longmemeval_date(&item.question_date).ok_or_else(|| {
        invalid_date_error(
            &item.question_id,
            "question_date",
            &item.question_date,
            config,
        )
    })?;

    let mut dimensions = vec![
        json!({
            "id": benchmark_scope,
            "kind": "benchmark_record",
            "title": format!("LongMemEval item {}", item.question_id),
            "metadata": {
                "question_id": item.question_id,
                "question_type": item.question_type,
                "abstention": is_abstention(item).to_string()
            }
        }),
        json!({
            "id": question_scope,
            "kind": "question",
            "title": item.question,
            "metadata": {
                "question_id": item.question_id,
                "question_type": item.question_type
            }
        }),
        json!({
            "id": evidence_scope,
            "kind": "evidence_set",
            "title": format!("Evidence for {}", item.question_id),
            "metadata": {
                "answer_session_ids": string_vec_metadata(&item.answer_session_ids)?
            }
        }),
    ];

    let mut entries = Vec::new();
    let mut evidence = Vec::new();
    let mut relations = Vec::new();
    let mut answer_turn_refs = Vec::new();
    let mut answer_session_refs = Vec::new();
    let mut pending_generated_evidence = generated_labels
        .map(generated_evidence_by_ref)
        .transpose()?;
    let mut global_sequence = 1u32;

    for (session_index, ((session_id, raw_date), turns)) in item
        .haystack_session_ids
        .iter()
        .zip(item.haystack_dates.iter())
        .zip(item.haystack_sessions.iter())
        .enumerate()
    {
        let session_ref = sanitize_ref_segment(session_id);
        let session_scope = format!("longmemeval:session:{ref_scope}:{session_ref}");
        let occurred_at = normalize_longmemeval_date(raw_date).ok_or_else(|| {
            invalid_date_error(
                &item.question_id,
                &format!("haystack_dates[{session_index}]"),
                raw_date,
                config,
            )
        })?;
        if item.answer_session_ids.iter().any(|id| id == session_id) {
            answer_session_refs.push(session_scope.clone());
        }

        dimensions.push(json!({
            "id": session_scope,
            "kind": "conversation",
            "title": format!("LongMemEval session {}", session_id),
            "metadata": {
                "session_id": session_id,
                "session_index": session_index.to_string(),
                "session_date": raw_date,
                "evidence_session": item
                    .answer_session_ids
                    .iter()
                    .any(|id| id == session_id)
                    .to_string()
            }
        }));

        for (turn_index, turn) in turns.iter().enumerate() {
            let role = normalize_role(&turn.role)?;
            let entry_ref = longmemeval_turn_ref(&ref_scope, session_id, turn_index + 1);
            let turn_sequence = u32::try_from(turn_index + 1).map_err(|_| {
                LongMemEvalAdapterError::new(format!(
                    "turn index overflows u32 for question {} session {}",
                    item.question_id, session_id
                ))
            })?;

            entries.push(json!({
                "id": entry_ref,
                "kind": format!("{role}_message"),
                "text": turn.content,
                "coordinates": [
                    {
                        "dimension": "benchmark_record",
                        "scope_id": benchmark_scope,
                        "sequence": global_sequence,
                        "occurred_at": occurred_at,
                        "metadata": {
                            "session_id": session_id,
                            "turn_index": turn_index.to_string(),
                            "role": role
                        }
                    },
                    {
                        "dimension": "conversation",
                        "scope_id": session_scope,
                        "sequence": turn_sequence,
                        "occurred_at": occurred_at,
                        "metadata": {
                            "session_id": session_id,
                            "turn_index": turn_index.to_string(),
                            "role": role
                        }
                    }
                ],
                "metadata": {
                    "question_id": item.question_id,
                    "question_type": item.question_type,
                    "session_id": session_id,
                    "session_index": session_index.to_string(),
                    "turn_index": turn_index.to_string(),
                    "role": role,
                    "has_answer": turn.has_answer.to_string()
                }
            }));

            if turn.has_answer {
                answer_turn_refs.push(entry_ref.clone());
            }

            let generated_evidence = pending_generated_evidence
                .as_mut()
                .and_then(|labels| labels.remove(&entry_ref));
            if generated_evidence.is_some()
                || (pending_generated_evidence.is_none() && turn.has_answer)
            {
                let evidence_reason = generated_evidence
                    .as_ref()
                    .map(|evidence| evidence.reason.as_str())
                    .unwrap_or("LongMemEval labels this turn as containing answer evidence.");
                let evidence_confidence = generated_evidence
                    .as_ref()
                    .map(|evidence| evidence.confidence.as_str())
                    .unwrap_or("high");
                let evidence_id =
                    format!("evidence:{ref_scope}:{}:{}", session_ref, turn_index + 1);
                evidence.push(json!({
                    "id": evidence_id,
                    "supports": [entry_ref, question_ref],
                    "text": turn.content,
                    "source": format!("LongMemEval session {} turn {}", session_id, turn_index + 1),
                    "time": occurred_at,
                    "metadata": {
                        "question_id": item.question_id,
                        "session_id": session_id,
                        "turn_index": turn_index.to_string(),
                        "role": role
                    }
                }));
                relations.push(json!({
                    "from": entry_ref,
                    "to": question_ref,
                    "rel": "supports_answer",
                    "class": "evidential",
                    "why": evidence_reason,
                    "evidence": turn.content,
                    "confidence": evidence_confidence,
                    "sequence": global_sequence
                }));
            }

            global_sequence = global_sequence.checked_add(1).ok_or_else(|| {
                LongMemEvalAdapterError::new(format!(
                    "global sequence overflows u32 for question {}",
                    item.question_id
                ))
            })?;
        }
    }

    if let Some(pending) = pending_generated_evidence
        && !pending.is_empty()
    {
        return Err(LongMemEvalAdapterError::new(format!(
            "question {} generated evidence references unknown turns: {}",
            item.question_id,
            pending.keys().cloned().collect::<Vec<_>>().join(", ")
        )));
    }

    entries.push(json!({
        "id": question_ref,
        "kind": "question",
        "text": item.question,
        "coordinates": [
            {
                "dimension": "benchmark_record",
                "scope_id": benchmark_scope,
                "sequence": global_sequence,
                "occurred_at": question_time
            },
            {
                "dimension": "question",
                "scope_id": question_scope,
                "sequence": 1,
                "occurred_at": question_time
            }
        ],
        "metadata": {
            "question_id": item.question_id,
            "question_type": item.question_type,
            "answer": answer_metadata(&item.answer),
            "question_date": item.question_date,
            "abstention": is_abstention(item).to_string()
        }
    }));

    let ingest = json!({
        "about": about,
        "memory": {
            "dimensions": dimensions,
            "entries": entries,
            "relations": relations,
            "evidence": evidence
        },
        "provenance": {
            "source_kind": "agent",
            "source_agent": ADAPTER_NAME,
            "observed_at": question_time,
            "correlation_id": format!("longmemeval:{}", item.question_id),
            "causation_id": format!("longmemeval:item:{}", item.question_id)
        },
        "idempotency_key": format!(
            "longmemeval:{}:kmp:v1:{}:{}:{}",
            item.question_id,
            config
                .run_id
                .as_deref()
                .map(sanitize_ref_segment)
                .unwrap_or_else(|| "default-run".to_string()),
            item_fingerprint,
            evidence_fingerprint
        )
    });

    let ask = json!({
        "about": about,
        "question": item.question,
        "answer_policy": "evidence_or_unknown",
        "dimensions": {
            "scope": "current_about",
            "mode": "all"
        }
    });

    Ok(LongMemEvalPreparedItem {
        question_id: item.question_id.clone(),
        question_type: item.question_type.clone(),
        about,
        ingest,
        ask,
        expected: LongMemEvalExpected {
            question_id: item.question_id.clone(),
            question_type: item.question_type.clone(),
            answer: item.answer.clone(),
            question: item.question.clone(),
            question_date: item.question_date.clone(),
            answer_session_ids: item.answer_session_ids.clone(),
            answer_turn_refs,
            answer_session_refs,
            abstention: is_abstention(item),
        },
    })
}

pub fn normalize_longmemeval_date(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.len() >= 20 && trimmed.contains('T') && trimmed.ends_with('Z') {
        return Some(trimmed.to_string());
    }

    let date = trimmed.get(0..10)?;
    let normalized_date =
        if date.as_bytes().get(4) == Some(&b'/') && date.as_bytes().get(7) == Some(&b'/') {
            date.replace('/', "-")
        } else if date.as_bytes().get(4) == Some(&b'-') && date.as_bytes().get(7) == Some(&b'-') {
            date.to_string()
        } else {
            return None;
        };

    if !normalized_date
        .chars()
        .enumerate()
        .all(|(index, ch)| matches!(index, 4 | 7) && ch == '-' || ch.is_ascii_digit())
    {
        return None;
    }

    let time = trimmed
        .rsplit_once(' ')
        .map(|(_, suffix)| suffix)
        .filter(|suffix| is_hour_minute(suffix))
        .unwrap_or("00:00");

    Some(format!("{normalized_date}T{time}:00Z"))
}

pub fn longmemeval_turn_ref(
    question_ref_scope: &str,
    session_id: &str,
    one_based_turn_index: usize,
) -> String {
    format!(
        "turn:{question_ref_scope}:{}:{one_based_turn_index}",
        sanitize_ref_segment(session_id)
    )
}

pub fn longmemeval_ref_scope(
    question_id: &str,
    run_id: Option<&str>,
) -> Result<String, LongMemEvalAdapterError> {
    let question_ref = sanitize_ref_segment(question_id);
    if question_ref.is_empty() {
        return Err(LongMemEvalAdapterError::new(
            "LongMemEval question_id must not be empty after normalization",
        ));
    }
    match run_id {
        Some(run_id) => {
            let run_ref = sanitize_ref_segment(run_id);
            if run_ref.is_empty() {
                return Err(LongMemEvalAdapterError::new(
                    "LongMemEval run_id must not be empty after normalization",
                ));
            }
            Ok(format!("run:{run_ref}:question:{question_ref}"))
        }
        None => Ok(question_ref),
    }
}

pub fn longmemeval_candidate_turns(
    item: &LongMemEvalItem,
    run_id: Option<&str>,
) -> Result<Vec<LongMemEvalCandidateTurn>, LongMemEvalAdapterError> {
    validate_item_shape(item)?;
    let ref_scope = longmemeval_ref_scope(&item.question_id, run_id)?;
    let mut turns = Vec::new();

    for ((session_id, session_date), session_turns) in item
        .haystack_session_ids
        .iter()
        .zip(item.haystack_dates.iter())
        .zip(item.haystack_sessions.iter())
    {
        for (turn_index, turn) in session_turns.iter().enumerate() {
            if turn.content.trim().is_empty() {
                continue;
            }
            let role = normalize_role(&turn.role)?.to_string();
            turns.push(LongMemEvalCandidateTurn {
                turn_ref: longmemeval_turn_ref(&ref_scope, session_id, turn_index + 1),
                role,
                content: turn.content.clone(),
                session_id: session_id.clone(),
                session_date: session_date.clone(),
                one_based_turn_index: turn_index + 1,
                has_answer: turn.has_answer,
            });
        }
    }

    Ok(turns)
}

pub fn longmemeval_answer_turn_refs(
    item: &LongMemEvalItem,
    run_id: Option<&str>,
) -> Result<Vec<String>, LongMemEvalAdapterError> {
    validate_item_shape(item)?;
    let ref_scope = longmemeval_ref_scope(&item.question_id, run_id)?;
    let mut refs = Vec::new();

    for (session_id, session_turns) in item
        .haystack_session_ids
        .iter()
        .zip(item.haystack_sessions.iter())
    {
        for (turn_index, turn) in session_turns.iter().enumerate() {
            if turn.has_answer {
                refs.push(longmemeval_turn_ref(&ref_scope, session_id, turn_index + 1));
            }
        }
    }

    Ok(refs)
}

fn summarize_prepared_items(
    dataset_items: usize,
    skipped_items: usize,
    prepared: &[LongMemEvalPreparedItem],
) -> LongMemEvalAdapterSummary {
    let mut question_types = BTreeMap::new();
    let mut sessions = 0usize;
    let mut turns = 0usize;
    let mut expected_evidence_turns = 0usize;
    let mut relation_evidence_turns = 0usize;
    let mut abstention_items = 0usize;

    for item in prepared {
        *question_types
            .entry(item.question_type.clone())
            .or_insert(0usize) += 1;
        if item.expected.abstention {
            abstention_items += 1;
        }
        expected_evidence_turns += item.expected.answer_turn_refs.len();

        let dimensions = item
            .ingest
            .get("memory")
            .and_then(|memory| memory.get("dimensions"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        sessions += dimensions
            .iter()
            .filter(|dimension| {
                dimension
                    .get("kind")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| kind == "conversation")
            })
            .count();

        let entries = item
            .ingest
            .get("memory")
            .and_then(|memory| memory.get("entries"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        turns += entries
            .iter()
            .filter(|entry| {
                entry
                    .get("kind")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| kind.ends_with("_message"))
            })
            .count();

        relation_evidence_turns += item
            .ingest
            .get("memory")
            .and_then(|memory| memory.get("evidence"))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default();
    }

    LongMemEvalAdapterSummary {
        dataset_items,
        prepared_items: prepared.len(),
        skipped_items,
        sessions,
        turns,
        expected_evidence_turns,
        relation_evidence_turns,
        abstention_items,
        question_types,
    }
}

fn validate_item_shape(item: &LongMemEvalItem) -> Result<(), LongMemEvalAdapterError> {
    let session_count = item.haystack_session_ids.len();
    if item.haystack_dates.len() != session_count {
        return Err(LongMemEvalAdapterError::new(format!(
            "question {} has {} session ids but {} dates",
            item.question_id,
            session_count,
            item.haystack_dates.len()
        )));
    }
    if item.haystack_sessions.len() != session_count {
        return Err(LongMemEvalAdapterError::new(format!(
            "question {} has {} session ids but {} session bodies",
            item.question_id,
            session_count,
            item.haystack_sessions.len()
        )));
    }
    if item.question_id.trim().is_empty() {
        return Err(LongMemEvalAdapterError::new(
            "LongMemEval question_id must not be empty",
        ));
    }
    if item.question.trim().is_empty() {
        return Err(LongMemEvalAdapterError::new(format!(
            "question {} has empty question text",
            item.question_id
        )));
    }
    let mut session_refs = BTreeSet::new();
    for (session_index, session_id) in item.haystack_session_ids.iter().enumerate() {
        let session_ref = sanitize_ref_segment(session_id);
        if session_ref.is_empty() {
            return Err(LongMemEvalAdapterError::new(format!(
                "question {} has empty session_id after normalization at session index {}; this LongMemEval shape is not supported",
                item.question_id, session_index
            )));
        }
        if !session_refs.insert(session_ref.clone()) {
            return Err(LongMemEvalAdapterError::new(format!(
                "question {} has duplicate or colliding session_id `{}` at session index {}; repeated LongMemEval session ids are not supported by the KMP adapter",
                item.question_id, session_id, session_index
            )));
        }
    }
    Ok(())
}

fn invalid_date_error(
    question_id: &str,
    field: &str,
    value: &str,
    config: &LongMemEvalAdapterConfig,
) -> LongMemEvalAdapterError {
    if config.strict_temporal {
        LongMemEvalAdapterError::new(format!(
            "question {question_id} has unsupported {field} date `{value}`"
        ))
    } else {
        LongMemEvalAdapterError::new(format!(
            "question {question_id} has unsupported {field} date `{value}`; non-strict temporal mode is not implemented for KMP output"
        ))
    }
}

fn normalize_role(role: &str) -> Result<&'static str, LongMemEvalAdapterError> {
    match role.trim().to_ascii_lowercase().as_str() {
        "user" => Ok("user"),
        "assistant" => Ok("assistant"),
        other => Err(LongMemEvalAdapterError::new(format!(
            "unsupported LongMemEval turn role `{other}`"
        ))),
    }
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

fn answer_metadata(answer: &Value) -> String {
    answer
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| answer.to_string())
}

fn longmemeval_item_fingerprint(item: &LongMemEvalItem) -> Result<String, LongMemEvalAdapterError> {
    let payload = serde_json::to_vec(item).map_err(|error| {
        LongMemEvalAdapterError::new(format!("failed to serialize LongMemEval item: {error}"))
    })?;
    let digest = Sha256::digest(payload);
    Ok(format!("{digest:x}").chars().take(16).collect())
}

fn generated_evidence_fingerprint(
    labels: &LongMemEvalEvidenceLabels,
) -> Result<String, LongMemEvalAdapterError> {
    let payload = serde_json::to_vec(labels).map_err(|error| {
        LongMemEvalAdapterError::new(format!(
            "failed to serialize generated evidence labels: {error}"
        ))
    })?;
    let digest = Sha256::digest(payload);
    Ok(format!(
        "llm-{}",
        format!("{digest:x}").chars().take(16).collect::<String>()
    ))
}

fn generated_evidence_by_ref(
    labels: &LongMemEvalEvidenceLabels,
) -> Result<BTreeMap<String, LongMemEvalEvidenceTurnLabel>, LongMemEvalAdapterError> {
    let mut by_ref = BTreeMap::new();
    for label in &labels.evidence_turns {
        if label.turn_ref.trim().is_empty() {
            return Err(LongMemEvalAdapterError::new(format!(
                "question {} has generated evidence with empty turn_ref",
                labels.question_id
            )));
        }
        if !matches!(
            label.confidence.as_str(),
            "high" | "medium" | "low" | "unknown"
        ) {
            return Err(LongMemEvalAdapterError::new(format!(
                "question {} generated evidence {} has invalid confidence `{}`",
                labels.question_id, label.turn_ref, label.confidence
            )));
        }
        if by_ref
            .insert(label.turn_ref.clone(), label.clone())
            .is_some()
        {
            return Err(LongMemEvalAdapterError::new(format!(
                "question {} has duplicate generated evidence turn_ref {}",
                labels.question_id, label.turn_ref
            )));
        }
    }
    Ok(by_ref)
}

fn string_vec_metadata(values: &[String]) -> Result<String, LongMemEvalAdapterError> {
    serde_json::to_string(values).map_err(|error| {
        LongMemEvalAdapterError::new(format!(
            "failed to serialize string-vector metadata value: {error}"
        ))
    })
}

fn is_abstention(item: &LongMemEvalItem) -> bool {
    item.question_id.ends_with("_abs")
}

fn is_hour_minute(value: &str) -> bool {
    let Some((hour, minute)) = value.split_once(':') else {
        return false;
    };
    hour.len() == 2
        && minute.len() == 2
        && hour.chars().all(|ch| ch.is_ascii_digit())
        && minute.chars().all(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn normalizes_longmemeval_session_date() {
        assert_eq!(
            normalize_longmemeval_date("2023/05/20 (Sat) 00:04"),
            Some("2023-05-20T00:04:00Z".to_string())
        );
        assert_eq!(
            normalize_longmemeval_date("2023-05-20"),
            Some("2023-05-20T00:00:00Z".to_string())
        );
    }

    #[test]
    fn rejects_mismatched_dataset_shape() {
        let item = LongMemEvalItem {
            question_id: "q1".to_string(),
            question_type: "multi-session".to_string(),
            question: "What changed?".to_string(),
            answer: json!("answer"),
            question_date: "2023/05/21 (Sun) 00:00".to_string(),
            haystack_session_ids: vec!["s1".to_string()],
            haystack_dates: Vec::new(),
            haystack_sessions: vec![Vec::new()],
            answer_session_ids: Vec::new(),
        };

        let error = prepare_longmemeval_item(&item, &LongMemEvalAdapterConfig::default())
            .expect_err("mismatched dates should fail");
        assert!(error.to_string().contains("session ids but 0 dates"));
    }

    #[test]
    fn rejects_duplicate_session_ids_as_unsupported() {
        let item = LongMemEvalItem {
            question_id: "q-duplicate".to_string(),
            question_type: "single-session-user".to_string(),
            question: "What happened?".to_string(),
            answer: json!("answer"),
            question_date: "2023/05/21 (Sun) 00:00".to_string(),
            haystack_session_ids: vec!["same-session".to_string(), "same-session".to_string()],
            haystack_dates: vec![
                "2023/05/20 (Sat) 00:04".to_string(),
                "2023/05/20 (Sat) 00:05".to_string(),
            ],
            haystack_sessions: vec![
                vec![LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "First.".to_string(),
                    has_answer: false,
                }],
                vec![LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "Second.".to_string(),
                    has_answer: false,
                }],
            ],
            answer_session_ids: Vec::new(),
        };

        let error = longmemeval_candidate_turns(&item, Some("run-a"))
            .expect_err("duplicate session ids should fail fast");

        assert!(
            error
                .to_string()
                .contains("repeated LongMemEval session ids")
        );
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn rejects_normalized_session_id_collisions_as_unsupported() {
        let item = LongMemEvalItem {
            question_id: "q-collision".to_string(),
            question_type: "single-session-user".to_string(),
            question: "What happened?".to_string(),
            answer: json!("answer"),
            question_date: "2023/05/21 (Sun) 00:00".to_string(),
            haystack_session_ids: vec!["same session".to_string(), "same-session".to_string()],
            haystack_dates: vec![
                "2023/05/20 (Sat) 00:04".to_string(),
                "2023/05/20 (Sat) 00:05".to_string(),
            ],
            haystack_sessions: vec![
                vec![LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "First.".to_string(),
                    has_answer: false,
                }],
                vec![LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "Second.".to_string(),
                    has_answer: false,
                }],
            ],
            answer_session_ids: Vec::new(),
        };

        let error = prepare_longmemeval_item(&item, &LongMemEvalAdapterConfig::default())
            .expect_err("normalized session id collisions should fail fast");

        assert!(
            error
                .to_string()
                .contains("duplicate or colliding session_id")
        );
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn prepares_kmp_ingest_and_expected_evidence() {
        let item = LongMemEvalItem {
            question_id: "830ce83f".to_string(),
            question_type: "knowledge-update".to_string(),
            question: "Where did Rachel move?".to_string(),
            answer: json!("Austin"),
            question_date: "2023/05/21 (Sun) 12:00".to_string(),
            haystack_session_ids: vec!["session-a".to_string()],
            haystack_dates: vec!["2023/05/20 (Sat) 00:04".to_string()],
            haystack_sessions: vec![vec![
                LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "Rachel said Denver.".to_string(),
                    has_answer: false,
                },
                LongMemEvalTurn {
                    role: "assistant".to_string(),
                    content: "Rachel corrected it to Austin.".to_string(),
                    has_answer: true,
                },
            ]],
            answer_session_ids: vec!["session-a".to_string()],
        };

        let prepared = prepare_longmemeval_item(&item, &LongMemEvalAdapterConfig::default())
            .expect("item should adapt");

        assert_eq!(prepared.about, "longmemeval:item:830ce83f");
        assert_eq!(
            prepared.ask["dimensions"],
            json!({"scope": "current_about", "mode": "all"})
        );
        assert_eq!(
            prepared.expected.answer_turn_refs,
            vec!["turn:830ce83f:session-a:2"]
        );
        assert_eq!(
            prepared.expected.answer_session_refs,
            vec!["longmemeval:session:830ce83f:session-a"]
        );
        assert_eq!(
            prepared.ingest["memory"]["relations"][0]["rel"],
            json!("supports_answer")
        );
        assert!(
            prepared.ingest["idempotency_key"]
                .as_str()
                .expect("idempotency key should be a string")
                .starts_with("longmemeval:830ce83f:kmp:v1:")
        );
    }

    #[test]
    fn prepares_kmp_ingest_from_generated_evidence_labels() {
        let item = LongMemEvalItem {
            question_id: "q-generated".to_string(),
            question_type: "single-session-user".to_string(),
            question: "Where did Rachel move?".to_string(),
            answer: json!("Austin"),
            question_date: "2023/05/21 (Sun) 12:00".to_string(),
            haystack_session_ids: vec!["session-a".to_string()],
            haystack_dates: vec!["2023/05/20 (Sat) 00:04".to_string()],
            haystack_sessions: vec![vec![
                LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "Rachel said Denver.".to_string(),
                    has_answer: true,
                },
                LongMemEvalTurn {
                    role: "assistant".to_string(),
                    content: "Rachel corrected it to Austin.".to_string(),
                    has_answer: false,
                },
            ]],
            answer_session_ids: vec!["session-a".to_string()],
        };
        let labels = LongMemEvalEvidenceLabels {
            question_id: "q-generated".to_string(),
            evidence_turns: vec![LongMemEvalEvidenceTurnLabel {
                turn_ref: "turn:q-generated:session-a:2".to_string(),
                reason: "LLM selected the corrected destination.".to_string(),
                confidence: "medium".to_string(),
            }],
        };

        let prepared = prepare_longmemeval_item(
            &item,
            &LongMemEvalAdapterConfig {
                generated_evidence: Some(BTreeMap::from([("q-generated".to_string(), labels)])),
                ..LongMemEvalAdapterConfig::default()
            },
        )
        .expect("item should adapt with generated labels");

        assert_eq!(
            prepared.expected.answer_turn_refs,
            vec!["turn:q-generated:session-a:1"]
        );
        assert_eq!(
            prepared.ingest["memory"]["relations"][0]["from"],
            json!("turn:q-generated:session-a:2")
        );
        assert_eq!(
            prepared.ingest["memory"]["relations"][0]["confidence"],
            json!("medium")
        );
        assert_eq!(
            prepared.ingest["memory"]["relations"][0]["rel"],
            json!("supports_answer")
        );
        assert!(
            prepared.ingest["idempotency_key"]
                .as_str()
                .expect("idempotency key should be a string")
                .contains(":llm-")
        );
    }

    #[test]
    fn run_id_isolates_about_and_memory_refs() {
        let item = LongMemEvalItem {
            question_id: "q-isolated".to_string(),
            question_type: "knowledge-update".to_string(),
            question: "Where did Rachel move?".to_string(),
            answer: json!("Austin"),
            question_date: "2023/05/21 (Sun) 12:00".to_string(),
            haystack_session_ids: vec!["session-a".to_string()],
            haystack_dates: vec!["2023/05/20 (Sat) 00:04".to_string()],
            haystack_sessions: vec![vec![LongMemEvalTurn {
                role: "user".to_string(),
                content: "Rachel moved to Austin.".to_string(),
                has_answer: true,
            }]],
            answer_session_ids: vec!["session-a".to_string()],
        };

        let prepared = prepare_longmemeval_item(
            &item,
            &LongMemEvalAdapterConfig {
                run_id: Some("clean-run".to_string()),
                ..LongMemEvalAdapterConfig::default()
            },
        )
        .expect("item should adapt with isolated run id");

        assert_eq!(prepared.about, "longmemeval:run:clean-run:item:q-isolated");
        assert_eq!(
            prepared.expected.answer_turn_refs,
            vec!["turn:run:clean-run:question:q-isolated:session-a:1"]
        );
        assert_eq!(
            prepared.ingest["memory"]["relations"][0]["from"],
            json!("turn:run:clean-run:question:q-isolated:session-a:1")
        );
        assert_eq!(
            prepared.ask["about"],
            json!("longmemeval:run:clean-run:item:q-isolated")
        );
    }

    #[test]
    fn applies_per_question_type_limit() {
        let mut items = Vec::new();
        for (question_id, question_type) in [
            ("q1", "temporal-reasoning"),
            ("q2", "temporal-reasoning"),
            ("q3", "knowledge-update"),
        ] {
            items.push(LongMemEvalItem {
                question_id: question_id.to_string(),
                question_type: question_type.to_string(),
                question: "What happened?".to_string(),
                answer: json!("answer"),
                question_date: "2023/05/21 (Sun) 12:00".to_string(),
                haystack_session_ids: vec!["session-a".to_string()],
                haystack_dates: vec!["2023/05/20 (Sat) 00:04".to_string()],
                haystack_sessions: vec![vec![LongMemEvalTurn {
                    role: "user".to_string(),
                    content: "The answer happened.".to_string(),
                    has_answer: true,
                }]],
                answer_session_ids: vec!["session-a".to_string()],
            });
        }

        let (prepared, summary) = prepare_longmemeval_items(
            &items,
            &LongMemEvalAdapterConfig {
                question_type: Some("temporal-reasoning".to_string()),
                per_question_type_limit: Some(1),
                ..LongMemEvalAdapterConfig::default()
            },
        )
        .expect("items should adapt");

        assert_eq!(prepared.len(), 1);
        assert_eq!(summary.skipped_items, 2);
        assert_eq!(
            summary.question_types,
            BTreeMap::from([("temporal-reasoning".to_string(), 1)])
        );
    }
}
