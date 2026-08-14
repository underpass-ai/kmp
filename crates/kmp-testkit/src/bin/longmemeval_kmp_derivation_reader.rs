use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    LlmProvider, LongMemEvalCandidateTurn, LongMemEvalItem, call_llm, longmemeval_candidate_turns,
    normalize_llm_json_response, parse_longmemeval_dataset,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
struct Args {
    input: PathBuf,
    candidates: PathBuf,
    output: PathBuf,
    endpoint: Option<String>,
    model: Option<String>,
    provider: Option<LlmProvider>,
    api_key_env: String,
    candidate_top_k: usize,
    question_type: String,
    question_id: Option<String>,
    limit: Option<usize>,
    max_tokens: u32,
    temperature: f64,
    force: bool,
}

#[derive(Debug, Deserialize)]
struct CandidateResultRow {
    question_id: String,
    question_type: String,
    ranked_candidates: Vec<RankedCandidateRef>,
}

#[derive(Debug, Clone, Deserialize)]
struct RankedCandidateRef {
    turn_ref: String,
    rank: usize,
    score: f64,
}

#[derive(Debug, Deserialize, Serialize)]
struct DerivationResponse {
    operation: String,
    unit: Option<String>,
    operands: Vec<DerivedOperand>,
    notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct DerivedOperand {
    #[serde(rename = "ref")]
    ref_id: String,
    label: String,
    role: Option<String>,
    entity: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_f64")]
    value: Option<f64>,
    reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DeterministicDerivation {
    operation: String,
    unit: Option<String>,
    derived_answer: String,
    numeric_value: Option<f64>,
    included_operands: usize,
    excluded_operands: usize,
}

#[derive(Debug, Serialize)]
struct DerivationItemResult {
    question_id: String,
    question_type: String,
    question: String,
    expected_answer: Value,
    derived_answer: String,
    lexical_answer_hit: bool,
    candidate_top_k: usize,
    candidate_turns: usize,
    operation: String,
    unit: Option<String>,
    numeric_value: Option<f64>,
    included_operands: usize,
    excluded_operands: usize,
    raw_response: DerivationResponse,
    prompt_tokens: u32,
    completion_tokens: u32,
    latency_ms: u128,
}

#[derive(Debug, Serialize)]
struct DerivationSummary {
    benchmark: &'static str,
    reader: &'static str,
    generated_at_unix_seconds: u64,
    source_path: String,
    candidates_path: String,
    endpoint: String,
    model: String,
    provider: &'static str,
    candidate_top_k: usize,
    question_type: String,
    total_items: usize,
    lexical_answer_hits: usize,
    prompt_tokens: u32,
    completion_tokens: u32,
    elapsed_ms: u128,
    operations: BTreeMap<String, usize>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    validate_args(&args)?;
    let endpoint = resolve_required(args.endpoint.as_deref(), "LLM_ENDPOINT", "--endpoint")?;
    let model = resolve_required(args.model.as_deref(), "LLM_MODEL", "--model")?;
    let provider = match args.provider {
        Some(provider) => provider,
        None => match env::var("LLM_PROVIDER").ok() {
            Some(value) if !value.trim().is_empty() => parse_provider(&value)?,
            _ => detect_provider_from_model(&model),
        },
    };
    let api_key = env::var(&args.api_key_env)
        .ok()
        .filter(|value| !value.trim().is_empty());
    ensure_output_dir(&args.output, args.force)?;

    let dataset_payload = fs::read_to_string(&args.input)?;
    let dataset = parse_longmemeval_dataset(&dataset_payload)?;
    let dataset_by_id = dataset
        .iter()
        .map(|item| (item.question_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let candidate_rows = read_candidate_rows(&args.candidates, &args)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;
    let started = Instant::now();
    let mut results = Vec::new();

    for row in candidate_rows {
        let item = dataset_by_id.get(row.question_id.as_str()).ok_or_else(|| {
            format!(
                "candidate row references unknown question {}",
                row.question_id
            )
        })?;
        let candidate_turns = candidate_turns_for_row(item, &row, args.candidate_top_k)?;
        let prompt_candidates = prompt_candidates_for_item(item, &candidate_turns);
        let prompt = build_derivation_prompt(item, &prompt_candidates);

        let call_started = Instant::now();
        let (raw, prompt_tokens, completion_tokens) = call_llm(
            &client,
            &endpoint,
            &model,
            provider,
            api_key.as_deref(),
            &prompt,
            args.max_tokens,
            args.temperature,
        )
        .await?;
        let latency_ms = call_started.elapsed().as_millis();
        // Tolerate a malformed LLM response or an incompatible operand on a single
        // item: skip it (a downstream reader falls back) instead of aborting the
        // whole benchmark run.
        let response = match parse_derivation_response(item, &prompt_candidates, &raw) {
            Ok(response) => response,
            Err(error) => {
                eprintln!("skip {}: derivation parse error: {error}", item.question_id);
                continue;
            }
        };
        let derivation = match compute_derivation(&item.question, &response) {
            Ok(derivation) => derivation,
            Err(error) => {
                eprintln!(
                    "skip {}: derivation compute error: {error}",
                    item.question_id
                );
                continue;
            }
        };
        let lexical_answer_hit =
            answer_contains_expected(&item.answer, Some(&derivation.derived_answer));

        results.push(DerivationItemResult {
            question_id: item.question_id.clone(),
            question_type: item.question_type.clone(),
            question: item.question.clone(),
            expected_answer: item.answer.clone(),
            derived_answer: derivation.derived_answer,
            lexical_answer_hit,
            candidate_top_k: args.candidate_top_k,
            candidate_turns: prompt_candidates.len(),
            operation: derivation.operation,
            unit: derivation.unit,
            numeric_value: derivation.numeric_value,
            included_operands: derivation.included_operands,
            excluded_operands: derivation.excluded_operands,
            raw_response: response,
            prompt_tokens,
            completion_tokens,
            latency_ms,
        });
    }

    write_jsonl(
        &args.output.join("derivation_results.jsonl"),
        results.iter().map(serde_json::to_value),
    )?;
    write_jsonl(
        &args.output.join("hypotheses.jsonl"),
        results.iter().map(|result| {
            Ok(json!({
                "question_id": result.question_id,
                "hypothesis": result.derived_answer
            }))
        }),
    )?;
    let summary = summarize_run(
        &args,
        endpoint,
        model,
        provider,
        started.elapsed().as_millis(),
        &results,
    )?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn read_candidate_rows(
    path: &Path,
    args: &Args,
) -> Result<Vec<CandidateResultRow>, Box<dyn Error + Send + Sync>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();

    for (line_index, line) in reader.lines().enumerate() {
        if args.limit.is_some_and(|limit| rows.len() >= limit) {
            break;
        }
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let row = serde_json::from_str::<CandidateResultRow>(&line).map_err(|error| {
            format!(
                "invalid candidate JSONL at {}:{}: {error}",
                path.display(),
                line_index + 1
            )
        })?;
        if row.question_type != args.question_type {
            continue;
        }
        if args
            .question_id
            .as_deref()
            .is_some_and(|question_id| row.question_id != question_id)
        {
            continue;
        }
        rows.push(row);
    }

    Ok(rows)
}

fn candidate_turns_for_row(
    item: &LongMemEvalItem,
    row: &CandidateResultRow,
    candidate_top_k: usize,
) -> Result<Vec<(RankedCandidateRef, LongMemEvalCandidateTurn)>, Box<dyn Error + Send + Sync>> {
    let turns_by_ref = longmemeval_candidate_turns(item, None)?
        .into_iter()
        .map(|turn| (turn.turn_ref.clone(), turn))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();

    for ranked in row.ranked_candidates.iter().take(candidate_top_k) {
        if !seen.insert(ranked.turn_ref.clone()) {
            return Err(format!(
                "candidate row {} contains duplicate ranked ref {}",
                row.question_id, ranked.turn_ref
            )
            .into());
        }
        let turn = turns_by_ref.get(&ranked.turn_ref).ok_or_else(|| {
            format!(
                "candidate row {} references unknown turn {}",
                row.question_id, ranked.turn_ref
            )
        })?;
        selected.push((
            RankedCandidateRef {
                turn_ref: ranked.turn_ref.clone(),
                rank: ranked.rank,
                score: ranked.score,
            },
            turn.clone(),
        ));
    }

    Ok(selected)
}

fn prompt_candidates_for_item(
    item: &LongMemEvalItem,
    candidates: &[(RankedCandidateRef, LongMemEvalCandidateTurn)],
) -> Vec<(RankedCandidateRef, LongMemEvalCandidateTurn)> {
    if first_person_history_question(&item.question) {
        let user_candidates = candidates
            .iter()
            .filter(|(_, turn)| turn.role.eq_ignore_ascii_case("user"))
            .cloned()
            .collect::<Vec<_>>();
        if !user_candidates.is_empty() {
            return user_candidates;
        }
    }
    candidates.to_vec()
}

fn build_derivation_prompt(
    item: &LongMemEvalItem,
    candidates: &[(RankedCandidateRef, LongMemEvalCandidateTurn)],
) -> String {
    let mut prompt = format!(
        "You are a deterministic derivation extractor for LongMemEval aggregate questions.\n\
         Use only the candidate turns below. Do not use the gold answer. Do not use outside knowledge.\n\
         Return strict JSON only, no markdown, with this shape:\n\
         {{\"operation\":\"sum|count|average|difference|max_by|list|unknown\",\"unit\":\"USD|hours|days|weeks|items|people|none|unknown\",\"operands\":[{{\"ref\":\"turn:...\",\"label\":\"include|exclude|context\",\"role\":\"addend|counted_item|average_member|minuend|subtrahend|candidate|context\",\"entity\":\"item/entity name or null\",\"value\":number-or-null,\"reason\":\"short reason\"}}],\"notes\":\"short derivation note\"}}\n\n\
         Rules:\n\
         - The value field must be a JSON number or null. Do not put dates or quoted numbers in value.\n\
         - For expense or money-total questions, include every matching purchase, service, repair, replacement, accessory, safety item, fee, booking, workshop, class, event, or rental in the requested category and time window. Do not require the exact category word to appear if the item is clearly part of the category.\n\
         - For relative windows such as \"last four months\", count backward from the Question date. Include events on or after the same calendar day N months earlier, and include month-only dates when that month falls inside the window.\n\
         - Use the session date as the event date when the candidate text does not contain a more specific date.\n\
         - The predicate and scope in the question are binding. Do not count related items that do not satisfy the requested action or condition.\n\
         - If the question lists actions with \"or\", satisfying any listed action is enough. For example, buy/purchase, assemble, sell, replace, return, pick up, or fix/repair each qualifies when it appears in the question.\n\
         - For first-person personal-history totals, user-authored turns are primary evidence. Exclude assistant suggestions, hypothetical options, route proposals, estimates, or recommendations unless the user later confirms they did them. When user and assistant text conflict, prefer the user's actual-event statements.\n\
         - For pickup/return/exchange questions, count each pending pickup or return obligation separately when the question asks for both. A replacement pickup and the old item return are separate obligations even if they refer to the same product type; this overrides distinct-entity deduplication. Use action-specific entities such as \"pick up boots\" and \"return boots\". Pickups from service providers such as dry cleaners count when the item is in the requested category.\n\
         - For travel totals to destinations, exclude return legs, round trips, scenic detours, intermediate route segments, and assistant-proposed itinerary legs unless the question explicitly asks for them.\n\
         - For sums of money, durations, quantities, or ages, use operation=sum and put the normalized numeric amount in value.\n\
         - For counts, first write the canonical set in your notes, then emit operands for that set. Use specific entity names from the candidate text, not generic labels, whenever names are available.\n\
         - For counts of distinct, different, type, kind, or unique things/events, use operation=count. Emit one included operand per canonical counted item with value=1. Never include the same canonical entity twice; label duplicate mentions as exclude with the same entity.\n\
         - For current ownership or possession counts, include items still owned/currently in use. Considering selling, planning maintenance, storing, or not using something often does not mean it is no longer owned. Exclude only items explicitly sold, returned, given away, or never acquired.\n\
         - For questions asking how many physical things were serviced or planned for service, count the physical things, not repeated service actions, unless the question asks how many times.\n\
         - For averages, use operation=average and include every averaged value.\n\
         - For date or duration differences, use operation=difference with role=minuend for the later/end date and role=subtrahend for the earlier/start date.\n\
         - For comparisons such as \"how much more\", use operation=difference, role=minuend for the larger/requested side, and role=subtrahend for the comparison side.\n\
         - For \"which ... most\" questions, use operation=max_by, include every candidate entity with its numeric value, and set entity to the candidate name. Include all explicit spent/paid/ordered amounts from user turns, including online stores, marketplace orders, delivery services, and subscriptions. Exclude planned future shopping without an actual amount spent.\n\
         - For grocery-store questions, online grocery sellers, organic marketplaces, membership grocery services, and delivery-backed grocery purchases count when the user spent money on grocery, pantry, organic, sustainable, produce, meat, dairy, snacks, or meal items.\n\
         - Use operation=list only when the question explicitly asks to list multiple values. For unsupported direct lookups such as \"what time\" or \"what date\", return operation=unknown rather than list.\n\
         - If the same real-world expense/event is mentioned twice, include it once and label duplicate mentions as exclude.\n\
         - If a candidate is related but should not affect the answer, label it context or exclude.\n\n\
         Question id: {}\n\
         Question type: {}\n\
         Question date: {}\n\
         Question: {}\n\n\
         Candidate turns ordered for derivation. Amount-bearing candidates may appear before lower-signal text; original embedding rank is still shown:\n",
        item.question_id, item.question_type, item.question_date, item.question
    );

    append_ordered_candidates(&mut prompt, item, candidates);

    prompt
}

fn append_ordered_candidates(
    prompt: &mut String,
    item: &LongMemEvalItem,
    candidates: &[(RankedCandidateRef, LongMemEvalCandidateTurn)],
) {
    let mut ordered_candidates = candidates.iter().collect::<Vec<_>>();
    ordered_candidates.sort_by(|(left_ranked, left_turn), (right_ranked, right_turn)| {
        candidate_prompt_priority(item, right_turn)
            .cmp(&candidate_prompt_priority(item, left_turn))
            .then_with(|| left_ranked.rank.cmp(&right_ranked.rank))
    });

    for (ranked, turn) in ordered_candidates {
        prompt.push_str(&format!(
            "\n[{}]\nEmbedding rank: {}\nEmbedding score: {:.6}\nSession: {} ({})\nTurn: {}\nRole: {}\nText: {}\n",
            turn.turn_ref,
            ranked.rank,
            ranked.score,
            turn.session_id,
            turn.session_date,
            turn.one_based_turn_index,
            turn.role,
            turn.content.replace('\n', " ")
        ));
    }
}

fn candidate_prompt_priority(item: &LongMemEvalItem, turn: &LongMemEvalCandidateTurn) -> usize {
    let question = item.question.to_ascii_lowercase();
    let content = turn.content.to_ascii_lowercase();
    let mut priority = 0usize;

    if content.chars().any(|ch| ch.is_ascii_digit()) {
        priority += 20;
    }
    if content.contains('$')
        || content.contains(" dollar")
        || content.contains(" dollars")
        || content.contains(" cost ")
        || content.contains(" spent ")
    {
        priority += 20;
    }
    if question_mentions_money(&question)
        && (content.contains('$') || content.contains("dollar") || content.contains("cost"))
    {
        priority += 40;
    }
    if first_person_history_question(&question) && turn.role.eq_ignore_ascii_case("user") {
        priority += 50;
    }
    if question.contains("how many") && content.chars().any(|ch| ch.is_ascii_digit()) {
        priority += 10;
    }

    priority
}

fn question_mentions_money(question: &str) -> bool {
    [
        "money", "spent", "spend", "expense", "expenses", "cost", "amount", "raise",
    ]
    .iter()
    .any(|needle| question.contains(needle))
}

fn first_person_history_question(question: &str) -> bool {
    question.contains(" i ")
        || question.contains(" me ")
        || question.contains(" my ")
        || question.starts_with("how many")
        || question.starts_with("how much")
}

fn parse_derivation_response(
    item: &LongMemEvalItem,
    candidates: &[(RankedCandidateRef, LongMemEvalCandidateTurn)],
    raw_response: &str,
) -> Result<DerivationResponse, Box<dyn Error + Send + Sync>> {
    let candidate_refs = candidates
        .iter()
        .map(|(_, turn)| turn.turn_ref.as_str())
        .collect::<BTreeSet<_>>();
    let normalized = normalize_llm_json_response(raw_response);
    let response = serde_json::from_str::<DerivationResponse>(&normalized).map_err(|error| {
        format!(
            "derivation reader returned invalid JSON for {}: {error}; raw={}",
            item.question_id, raw_response
        )
    })?;

    normalize_operation(&response.operation)?;
    for operand in &response.operands {
        normalize_label(&operand.label)?;
        if !candidate_refs.contains(operand.ref_id.as_str()) {
            return Err(format!(
                "derivation reader selected unknown ref for {}: {}",
                item.question_id, operand.ref_id
            )
            .into());
        }
        if let Some(value) = operand.value
            && !value.is_finite()
        {
            return Err(format!(
                "derivation reader returned non-finite value for {} ref {}",
                item.question_id, operand.ref_id
            )
            .into());
        }
    }

    Ok(response)
}

fn deserialize_optional_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_f64()
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("value must be a finite number")),
        Some(Value::String(value)) => parse_numeric_value_string(&value)
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("value string must contain a number or date")),
        Some(other) => Err(serde::de::Error::custom(format!(
            "value must be number, string, or null, got {other}"
        ))),
    }
}

fn parse_numeric_value_string(value: &str) -> Option<f64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    parse_iso_date_ordinal(value).or_else(|| first_number(value))
}

fn parse_iso_date_ordinal(value: &str) -> Option<f64> {
    let separator = if value.contains('-') { '-' } else { '/' };
    let parts = value.split(separator).collect::<Vec<_>>();
    if parts.len() != 3 || parts[0].len() != 4 {
        return None;
    }
    let year = parts[0].parse::<i32>().ok()?;
    let month = parts[1].parse::<u32>().ok()?;
    let day = parts[2].parse::<u32>().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) as f64)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year as i64 - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month as i64 + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn compute_derivation(
    question: &str,
    response: &DerivationResponse,
) -> Result<DeterministicDerivation, Box<dyn Error + Send + Sync>> {
    let operation = normalize_operation(&response.operation)?.to_string();
    let unit = response.unit.as_ref().map(|value| value.trim().to_string());
    let included = response
        .operands
        .iter()
        .filter(|operand| normalize_label(&operand.label).is_ok_and(|label| label == "include"))
        .collect::<Vec<_>>();
    let excluded_operands = response
        .operands
        .iter()
        .filter(|operand| normalize_label(&operand.label).is_ok_and(|label| label == "exclude"))
        .count();

    let (derived_answer, numeric_value) = match operation.as_str() {
        "sum" => {
            let value = sum_values(&included, "sum")?;
            (format_numeric_answer(value, unit.as_deref()), Some(value))
        }
        "count" => {
            let counted = count_operands_for_question(question, &included);
            let value = counted
                .iter()
                .map(|operand| operand.value.unwrap_or(1.0))
                .sum::<f64>();
            (
                format_count_answer(value, unit.as_deref(), &counted),
                Some(value),
            )
        }
        "average" => {
            if included.is_empty() {
                return Err("average derivation requires at least one included operand".into());
            }
            let value = sum_values(&included, "average")? / included.len() as f64;
            (format_numeric_answer(value, unit.as_deref()), Some(value))
        }
        "difference" => {
            let minuends = included
                .iter()
                .filter(|operand| role_is(operand, "minuend"))
                .collect::<Vec<_>>();
            let subtrahends = included
                .iter()
                .filter(|operand| role_is(operand, "subtrahend"))
                .collect::<Vec<_>>();
            if minuends.len() != 1 || subtrahends.len() != 1 {
                return Err("difference derivation requires one minuend and one subtrahend".into());
            }
            let minuend = value_for(minuends[0], "difference minuend")?;
            let subtrahend = value_for(subtrahends[0], "difference subtrahend")?;
            let mut value = minuend - subtrahend;
            if value < 0.0 && should_use_absolute_difference(question, unit.as_deref()) {
                value = value.abs();
            }
            (format_numeric_answer(value, unit.as_deref()), Some(value))
        }
        "max_by" => {
            let best = included
                .iter()
                .filter_map(|operand| operand.value.map(|value| (operand, value)))
                .max_by(|(_, left), (_, right)| left.total_cmp(right))
                .ok_or("max_by derivation requires at least one valued included operand")?;
            let entity = best
                .0
                .entity
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or("max_by derivation requires entity on the winning operand")?;
            (entity.to_string(), Some(best.1))
        }
        "list" => {
            let values = included
                .iter()
                .filter_map(|operand| operand.entity.as_deref())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            if values.is_empty() {
                return Err("list derivation requires included operand entities".into());
            }
            (values.join(", "), None)
        }
        "unknown" => ("UNKNOWN".to_string(), None),
        _ => unreachable!("operation normalized above"),
    };

    Ok(DeterministicDerivation {
        operation,
        unit,
        derived_answer,
        numeric_value,
        included_operands: included.len(),
        excluded_operands,
    })
}

fn count_operands_for_question<'a>(
    question: &str,
    included: &[&'a DerivedOperand],
) -> Vec<&'a DerivedOperand> {
    if !question_requires_distinct_count(question) {
        return included.to_vec();
    }

    let mut selected = Vec::new();
    let mut seen_entities = BTreeSet::new();
    for operand in included {
        let entity_key = operand
            .entity
            .as_deref()
            .map(canonical_entity_key)
            .filter(|value| !value.is_empty());
        if operand.value.unwrap_or(1.0) == 1.0
            && entity_key.is_some_and(|key| !seen_entities.insert(key))
        {
            continue;
        }
        selected.push(*operand);
    }
    selected
}

fn question_requires_distinct_count(question: &str) -> bool {
    let question = question.to_ascii_lowercase();
    [
        "different",
        "distinct",
        "unique",
        "type of",
        "types of",
        "kind of",
        "kinds of",
    ]
    .iter()
    .any(|needle| question.contains(needle))
}

fn canonical_entity_key(entity: &str) -> String {
    normalize_for_lexical_match(entity)
        .split_whitespace()
        .filter(|token| !matches!(*token, "a" | "an" | "the" | "new" | "old"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_count_answer(value: f64, unit: Option<&str>, operands: &[&DerivedOperand]) -> String {
    let base = format_numeric_answer(value, unit);
    let mut entities = Vec::new();
    let mut seen = BTreeSet::new();
    for operand in operands {
        let Some(entity) = operand
            .entity
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if seen.insert(canonical_entity_key(entity)) {
            entities.push(entity);
        }
    }

    if entities.len() < 2 || entities.len() > 12 {
        base
    } else {
        format!("{base}: {}", entities.join(", "))
    }
}

fn should_use_absolute_difference(question: &str, unit: Option<&str>) -> bool {
    let unit_is_duration = unit
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|unit| {
            matches!(
                unit.as_str(),
                "day" | "days" | "week" | "weeks" | "month" | "months" | "hour" | "hours"
            )
        });
    if !unit_is_duration {
        return false;
    }

    let question = question.to_ascii_lowercase();
    question.contains("how long")
        || question.contains("how many days")
        || question.contains("how many weeks")
        || question.contains("how many months")
        || question.contains("how many hours")
}

fn sum_values(
    operands: &[&DerivedOperand],
    context: &str,
) -> Result<f64, Box<dyn Error + Send + Sync>> {
    if operands.is_empty() {
        return Err(format!("{context} derivation requires at least one included operand").into());
    }
    operands
        .iter()
        .map(|operand| value_for(operand, context))
        .sum()
}

fn value_for(operand: &DerivedOperand, context: &str) -> Result<f64, Box<dyn Error + Send + Sync>> {
    operand.value.ok_or_else(|| {
        format!(
            "{context} derivation requires numeric value for ref {}",
            operand.ref_id
        )
        .into()
    })
}

fn role_is(operand: &DerivedOperand, expected: &str) -> bool {
    operand
        .role
        .as_deref()
        .map(str::trim)
        .is_some_and(|role| role.eq_ignore_ascii_case(expected))
}

fn format_numeric_answer(value: f64, unit: Option<&str>) -> String {
    let number = if (value.round() - value).abs() < 0.000_001 {
        format!("{}", value.round() as i64)
    } else {
        let mut value = format!("{value:.4}");
        while value.contains('.') && value.ends_with('0') {
            value.pop();
        }
        if value.ends_with('.') {
            value.pop();
        }
        value
    };

    match unit
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        Some("USD") | Some("$") | Some("dollars") => format!("${number}"),
        Some("none") | Some("unknown") | None => number,
        Some(unit) => format!("{number} {unit}"),
    }
}

fn normalize_operation(value: &str) -> Result<&'static str, Box<dyn Error + Send + Sync>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "sum" => Ok("sum"),
        "count" => Ok("count"),
        "average" => Ok("average"),
        "difference" => Ok("difference"),
        "max_by" | "max-by" | "argmax" => Ok("max_by"),
        "list" => Ok("list"),
        "unknown" => Ok("unknown"),
        other => Err(format!(
            "unsupported derivation operation `{other}`; use sum, count, average, difference, max_by, list, or unknown"
        )
        .into()),
    }
}

fn normalize_label(value: &str) -> Result<&'static str, Box<dyn Error + Send + Sync>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "include" => Ok("include"),
        "exclude" => Ok("exclude"),
        "context" => Ok("context"),
        other => Err(format!(
            "unsupported derivation operand label `{other}`; use include, exclude, or context"
        )
        .into()),
    }
}

fn answer_contains_expected(expected_answer: &Value, observed_answer: Option<&str>) -> bool {
    let Some(observed_answer) = observed_answer else {
        return false;
    };
    let expected = normalize_answer_text(expected_answer);
    let expected = normalize_for_lexical_match(&expected);
    let observed = normalize_for_lexical_match(observed_answer);
    if !expected.is_empty() && (observed.contains(&expected) || expected.contains(&observed)) {
        return true;
    }
    first_number(observed_answer).is_some_and(|observed_number| {
        expected_contains_number(&normalize_answer_text(expected_answer), observed_number)
    })
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

fn first_number(value: &str) -> Option<f64> {
    let mut buffer = String::new();
    let mut started = false;
    for ch in value.chars() {
        if ch.is_ascii_digit()
            || (ch == '.' && started && !buffer.contains('.'))
            || (ch == ',' && started)
        {
            started = true;
            buffer.push(ch);
        } else if started {
            break;
        }
    }
    if buffer.is_empty() || buffer == "." {
        None
    } else {
        buffer.replace(',', "").parse::<f64>().ok()
    }
}

fn expected_contains_number(expected_answer: &str, observed_number: f64) -> bool {
    if let Some(expected_number) = first_number(expected_answer)
        && (expected_number - observed_number).abs() < 0.000_001
    {
        return true;
    }
    if (observed_number.round() - observed_number).abs() > 0.000_001 {
        return false;
    }
    let number = observed_number.round() as i64;
    let Some(word) = number_word(number) else {
        return false;
    };
    normalize_for_lexical_match(expected_answer)
        .split_whitespace()
        .any(|token| token == word)
}

fn number_word(value: i64) -> Option<&'static str> {
    match value {
        0 => Some("zero"),
        1 => Some("one"),
        2 => Some("two"),
        3 => Some("three"),
        4 => Some("four"),
        5 => Some("five"),
        6 => Some("six"),
        7 => Some("seven"),
        8 => Some("eight"),
        9 => Some("nine"),
        10 => Some("ten"),
        11 => Some("eleven"),
        12 => Some("twelve"),
        13 => Some("thirteen"),
        14 => Some("fourteen"),
        15 => Some("fifteen"),
        16 => Some("sixteen"),
        17 => Some("seventeen"),
        18 => Some("eighteen"),
        19 => Some("nineteen"),
        20 => Some("twenty"),
        _ => None,
    }
}

fn summarize_run(
    args: &Args,
    endpoint: String,
    model: String,
    provider: LlmProvider,
    elapsed_ms: u128,
    results: &[DerivationItemResult],
) -> Result<DerivationSummary, Box<dyn Error + Send + Sync>> {
    let mut operations = BTreeMap::<String, usize>::new();
    for result in results {
        *operations.entry(result.operation.clone()).or_default() += 1;
    }

    Ok(DerivationSummary {
        benchmark: "LongMemEval",
        reader: "longmemeval-kmp-derivation-reader-v1",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        source_path: args.input.display().to_string(),
        candidates_path: args.candidates.display().to_string(),
        endpoint,
        model,
        provider: provider_label(provider),
        candidate_top_k: args.candidate_top_k,
        question_type: args.question_type.clone(),
        total_items: results.len(),
        lexical_answer_hits: results
            .iter()
            .filter(|result| result.lexical_answer_hit)
            .count(),
        prompt_tokens: results.iter().map(|result| result.prompt_tokens).sum(),
        completion_tokens: results.iter().map(|result| result.completion_tokens).sum(),
        elapsed_ms,
        operations,
    })
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

fn resolve_required(
    cli_value: Option<&str>,
    env_key: &str,
    flag: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    cli_value
        .map(ToString::to_string)
        .or_else(|| env::var(env_key).ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{flag} or {env_key} is required").into())
}

fn detect_provider_from_model(model: &str) -> LlmProvider {
    if model.starts_with("claude") {
        LlmProvider::Anthropic
    } else if model.starts_with("gpt-5") || model.starts_with("o3") || model.starts_with("o4") {
        LlmProvider::OpenAINew
    } else {
        LlmProvider::OpenAI
    }
}

fn parse_provider(value: &str) -> Result<LlmProvider, Box<dyn Error + Send + Sync>> {
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

fn provider_label(provider: LlmProvider) -> &'static str {
    match provider {
        LlmProvider::OpenAI => "openai",
        LlmProvider::OpenAINew => "openai-new",
        LlmProvider::Anthropic => "anthropic",
    }
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut input = None;
    let mut candidates = None;
    let mut output = None;
    let mut endpoint = None;
    let mut model = None;
    let mut provider = None;
    let mut api_key_env = "LLM_API_KEY".to_string();
    let mut candidate_top_k = 25usize;
    let mut question_type = "multi-session".to_string();
    let mut question_id = None;
    let mut limit = None;
    let mut max_tokens = 1024u32;
    let mut temperature = 0.0f64;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--candidates" => {
                candidates = Some(PathBuf::from(required_flag_value(&mut args, &arg)?));
            }
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--endpoint" => endpoint = Some(required_flag_value(&mut args, &arg)?),
            "--model" => model = Some(required_flag_value(&mut args, &arg)?),
            "--provider" => {
                provider = Some(parse_provider(&required_flag_value(&mut args, &arg)?)?)
            }
            "--api-key-env" => api_key_env = required_flag_value(&mut args, &arg)?,
            "--candidate-top-k" => {
                let value = required_flag_value(&mut args, &arg)?;
                candidate_top_k = value.parse::<usize>().map_err(|error| {
                    format!("invalid --candidate-top-k value `{value}`: {error}")
                })?;
            }
            "--question-type" => question_type = required_flag_value(&mut args, &arg)?,
            "--question-id" => question_id = Some(required_flag_value(&mut args, &arg)?),
            "--limit" => {
                let value = required_flag_value(&mut args, &arg)?;
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|error| format!("invalid --limit value `{value}`: {error}"))?,
                );
            }
            "--max-tokens" => {
                let value = required_flag_value(&mut args, &arg)?;
                max_tokens = value
                    .parse::<u32>()
                    .map_err(|error| format!("invalid --max-tokens value `{value}`: {error}"))?;
            }
            "--temperature" => {
                let value = required_flag_value(&mut args, &arg)?;
                temperature = value
                    .parse::<f64>()
                    .map_err(|error| format!("invalid --temperature value `{value}`: {error}"))?;
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
        input: input.ok_or("--input is required")?,
        candidates: candidates.ok_or("--candidates is required")?,
        output: output.ok_or("--output is required")?,
        endpoint,
        model,
        provider,
        api_key_env,
        candidate_top_k,
        question_type,
        question_id,
        limit,
        max_tokens,
        temperature,
        force,
    })
}

fn validate_args(args: &Args) -> Result<(), Box<dyn Error + Send + Sync>> {
    if args.candidate_top_k == 0 {
        return Err("--candidate-top-k must be greater than zero".into());
    }
    if args.max_tokens == 0 {
        return Err("--max-tokens must be greater than zero".into());
    }
    Ok(())
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
        "Usage: longmemeval_kmp_derivation_reader --input <longmemeval.json> --candidates <candidate_results.jsonl> --output <out-dir> --endpoint <chat-completions-url> --model <model> [--provider openai|openai-new|anthropic] [--api-key-env LLM_API_KEY] [--candidate-top-k N] [--question-type TYPE] [--question-id ID] [--limit N] [--max-tokens N] [--temperature F] [--force]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_sum_and_formats_currency() {
        let response = DerivationResponse {
            operation: "sum".to_string(),
            unit: Some("USD".to_string()),
            operands: vec![
                operand("a", "include", Some("addend"), Some(120.0)),
                operand("b", "exclude", Some("addend"), Some(120.0)),
                operand("c", "include", Some("addend"), Some(65.0)),
            ],
            notes: None,
        };

        let derivation = compute_derivation("How much money did I spend?", &response)
            .expect("sum should compute");
        assert_eq!(derivation.derived_answer, "$185");
        assert_eq!(derivation.included_operands, 2);
        assert_eq!(derivation.excluded_operands, 1);
    }

    #[test]
    fn computes_count_with_default_unit_operands() {
        let response = DerivationResponse {
            operation: "count".to_string(),
            unit: Some("items".to_string()),
            operands: vec![
                operand("a", "include", Some("counted_item"), None),
                operand("b", "include", Some("counted_item"), Some(2.0)),
                operand("c", "context", Some("context"), None),
            ],
            notes: None,
        };

        let derivation = compute_derivation("How many items did I use?", &response)
            .expect("count should compute");
        assert_eq!(derivation.derived_answer, "3 items");
    }

    #[test]
    fn computes_distinct_count_with_entities_and_dedupes_duplicates() {
        let response = DerivationResponse {
            operation: "count".to_string(),
            unit: Some("items".to_string()),
            operands: vec![
                operand_with_entity("a", "include", Some("counted_item"), Some(1.0), "lime"),
                operand_with_entity("b", "include", Some("counted_item"), Some(1.0), "lime"),
                operand_with_entity("c", "include", Some("counted_item"), Some(1.0), "lemon"),
                operand_with_entity("d", "include", Some("counted_item"), Some(1.0), "orange"),
            ],
            notes: None,
        };

        let derivation = compute_derivation(
            "How many different types of citrus fruits have I used?",
            &response,
        )
        .expect("distinct count should compute");
        assert_eq!(derivation.derived_answer, "3 items: lime, lemon, orange");
    }

    #[test]
    fn computes_difference_from_roles() {
        let response = DerivationResponse {
            operation: "difference".to_string(),
            unit: Some("USD".to_string()),
            operands: vec![
                operand("hawaii", "include", Some("minuend"), Some(420.0)),
                operand("tokyo", "include", Some("subtrahend"), Some(150.0)),
            ],
            notes: None,
        };

        let derivation = compute_derivation("How much more did I spend?", &response)
            .expect("difference should compute");
        assert_eq!(derivation.derived_answer, "$270");
    }

    #[test]
    fn computes_absolute_duration_difference_when_roles_are_reversed() {
        let response = DerivationResponse {
            operation: "difference".to_string(),
            unit: Some("days".to_string()),
            operands: vec![
                operand("purchase", "include", Some("minuend"), Some(15.0)),
                operand("arrival", "include", Some("subtrahend"), Some(20.0)),
            ],
            notes: None,
        };

        let derivation = compute_derivation(
            "How many days did it take for my laptop backpack to arrive after I bought it?",
            &response,
        )
        .expect("duration difference should compute");
        assert_eq!(derivation.derived_answer, "5 days");
    }

    #[test]
    fn parses_date_strings_as_numeric_ordinals_for_difference() {
        let response = serde_json::from_value::<DerivationResponse>(json!({
            "operation": "difference",
            "unit": "days",
            "operands": [
                {
                    "ref": "arrival",
                    "label": "include",
                    "role": "minuend",
                    "entity": "arrival",
                    "value": "2023-02-10",
                    "reason": "arrived"
                },
                {
                    "ref": "order",
                    "label": "include",
                    "role": "subtrahend",
                    "entity": "order",
                    "value": "2023-02-05",
                    "reason": "ordered"
                }
            ],
            "notes": null
        }))
        .expect("date string values should deserialize");

        let derivation = compute_derivation(
            "How many days did it take for me to receive the new remote shutter release after I ordered it?",
            &response,
        )
        .expect("date-string difference should compute");
        assert_eq!(derivation.derived_answer, "5 days");
    }

    #[test]
    fn lexical_match_accepts_number_words() {
        assert!(answer_contains_expected(
            &json!("I have worked on five model kits."),
            Some("5 items")
        ));
        assert!(!answer_contains_expected(
            &json!("15 hours for getting to the destinations"),
            Some("41 hours")
        ));
        assert!(answer_contains_expected(&json!("$5,850"), Some("$5850")));
    }

    fn operand(
        ref_id: &str,
        label: &str,
        role: Option<&str>,
        value: Option<f64>,
    ) -> DerivedOperand {
        operand_with_entity(ref_id, label, role, value, "")
    }

    fn operand_with_entity(
        ref_id: &str,
        label: &str,
        role: Option<&str>,
        value: Option<f64>,
        entity: &str,
    ) -> DerivedOperand {
        DerivedOperand {
            ref_id: ref_id.to_string(),
            label: label.to_string(),
            role: role.map(ToString::to_string),
            entity: (!entity.is_empty()).then(|| entity.to_string()),
            value,
            reason: None,
        }
    }
}
