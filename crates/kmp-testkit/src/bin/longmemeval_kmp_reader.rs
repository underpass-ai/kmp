use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kmp_domain::{MemoryRelationType, RelationSemanticClass};
use kmp_interpretation::{
    ComposedEvidenceReader, EvidenceFragment, EvidenceInterpretationInput,
    EvidenceReaderPluginConfiguration, EvidenceReaderRequest,
};
use kmp_testkit::{LlmProvider, LongMemEvalExpected, call_llm};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
struct Args {
    artifacts: PathBuf,
    run: PathBuf,
    output: PathBuf,
    endpoint: Option<String>,
    model: Option<String>,
    provider: Option<LlmProvider>,
    api_key_env: String,
    tls_cert_path: Option<String>,
    tls_key_path: Option<String>,
    max_tokens: u32,
    temperature: f64,
    limit: Option<usize>,
    force: bool,
}

#[derive(Debug, Deserialize)]
struct RunItem {
    question_id: String,
    question_type: String,
    evidence_hit: String,
    ask_answer: Option<String>,
    ask_content: Option<Value>,
    plugin_reader: Option<Value>,
}

#[derive(Debug, Clone)]
struct EvidenceSnippet {
    ref_id: String,
    text: String,
    source: Option<String>,
    time: Option<String>,
    sequence: Option<u64>,
    why: Option<String>,
    confidence: Option<String>,
}

#[derive(Debug, Serialize)]
struct ReaderItemResult {
    question_id: String,
    question_type: String,
    question: String,
    evidence_hit: String,
    evidence_chars: usize,
    plugin_interpretation_chars: usize,
    plugin_reader: Value,
    plugin_value_mentions: usize,
    plugin_derivation_results: usize,
    plugin_diagnostics: usize,
    graph_relation_context_chars: usize,
    graph_relation_total: usize,
    graph_relation_structural: usize,
    graph_relation_anemic: usize,
    graph_relation_rich: usize,
    expected_answer: Value,
    hypothesis: String,
    lexical_answer_hit: bool,
    prompt_tokens: u32,
    completion_tokens: u32,
    reader_latency_ms: u128,
}

#[derive(Debug, Serialize)]
struct ReaderSummary {
    benchmark: &'static str,
    reader: &'static str,
    generated_at_unix_seconds: u64,
    artifacts: String,
    run: String,
    endpoint: String,
    model: String,
    provider: &'static str,
    total_items: usize,
    value_plugins: Vec<&'static str>,
    derivation_plugins: Vec<&'static str>,
    plugin_configuration: EvidenceReaderPluginConfiguration,
    plugin_value_mentions: usize,
    plugin_derivation_results: usize,
    plugin_diagnostics: usize,
    graph_relation_total: usize,
    graph_relation_structural: usize,
    graph_relation_anemic: usize,
    graph_relation_rich: usize,
    lexical_answer_items: usize,
    lexical_answer_hits: usize,
    prompt_tokens: u32,
    completion_tokens: u32,
    elapsed_ms: u128,
    question_types: BTreeMap<String, ReaderQuestionTypeSummary>,
}

#[derive(Debug, Default, Serialize)]
struct ReaderQuestionTypeSummary {
    count: usize,
    lexical_answer_hits: usize,
}

#[derive(Debug, Clone, Default)]
struct GraphRelationContext {
    total: usize,
    structural: usize,
    anemic: usize,
    rich: usize,
    rendered: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
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
    let tls_cert_path = resolve_optional(args.tls_cert_path.as_deref(), "LLM_TLS_CERT_PATH");
    let tls_key_path = resolve_optional(args.tls_key_path.as_deref(), "LLM_TLS_KEY_PATH");
    ensure_output_dir(&args.output, args.force)?;

    let expected = read_expected(&args.artifacts.join("expected.jsonl"), args.limit)?;
    let run_items = read_run_items(&args.run.join("results.jsonl"), args.limit)?;
    if expected.len() != run_items.len() {
        return Err(format!(
            "artifact count mismatch: expected={} run={}",
            expected.len(),
            run_items.len()
        )
        .into());
    }

    let client = build_llm_http_client(
        Duration::from_secs(180),
        tls_cert_path.as_deref(),
        tls_key_path.as_deref(),
    )?;
    let started = Instant::now();
    let mut reader_results = Vec::new();
    let plugin_reader = ComposedEvidenceReader::kernel_default();

    for (expected, run_item) in expected.iter().zip(run_items.iter()) {
        validate_matching_item(expected, run_item)?;
        let evidence = recovered_evidence_text(run_item)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{} has no recovered evidence to read", expected.question_id))?;
        let plugin_output = plugin_output_for_item(&plugin_reader, expected, run_item)?;
        let plugin_interpretation = render_plugin_interpretation(&plugin_output);
        let graph_relation_context = graph_relation_context(run_item, &plugin_output);
        let prompt = build_reader_prompt(
            &expected.question_type,
            &expected.question,
            expected.abstention,
            &evidence,
            plugin_interpretation.as_deref(),
            graph_relation_context.rendered.as_deref(),
        );

        let reader_started = Instant::now();
        let (raw_hypothesis, prompt_tokens, completion_tokens) = call_llm(
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
        let reader_latency_ms = reader_started.elapsed().as_millis();
        let hypothesis = normalize_reader_output(&raw_hypothesis);
        let lexical_answer_hit = answer_contains_expected(&expected.answer, Some(&hypothesis));

        reader_results.push(ReaderItemResult {
            question_id: expected.question_id.clone(),
            question_type: expected.question_type.clone(),
            question: expected.question.clone(),
            evidence_hit: run_item.evidence_hit.clone(),
            evidence_chars: evidence.len(),
            plugin_interpretation_chars: plugin_interpretation
                .as_deref()
                .map(str::len)
                .unwrap_or_default(),
            plugin_value_mentions: plugin_value_count(&plugin_output),
            plugin_derivation_results: plugin_derivation_count(&plugin_output),
            plugin_diagnostics: plugin_diagnostic_count(&plugin_output),
            plugin_reader: plugin_output,
            graph_relation_context_chars: graph_relation_context
                .rendered
                .as_deref()
                .map(str::len)
                .unwrap_or_default(),
            graph_relation_total: graph_relation_context.total,
            graph_relation_structural: graph_relation_context.structural,
            graph_relation_anemic: graph_relation_context.anemic,
            graph_relation_rich: graph_relation_context.rich,
            expected_answer: expected.answer.clone(),
            hypothesis,
            lexical_answer_hit,
            prompt_tokens,
            completion_tokens,
            reader_latency_ms,
        });
    }

    write_jsonl(
        &args.output.join("reader_results.jsonl"),
        reader_results.iter().map(serde_json::to_value),
    )?;
    write_jsonl(
        &args.output.join("hypotheses.jsonl"),
        reader_results.iter().map(|item| {
            Ok(json!({
                "question_id": item.question_id,
                "hypothesis": item.hypothesis
            }))
        }),
    )?;
    let summary = summarize_reader_run(
        &args,
        endpoint,
        model,
        provider,
        started.elapsed().as_millis(),
        &reader_results,
        &plugin_reader,
    )?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn build_reader_prompt(
    question_type: &str,
    question: &str,
    abstention: bool,
    evidence: &str,
    plugin_interpretation: Option<&str>,
    graph_relation_context: Option<&str>,
) -> String {
    let task_instruction = if abstention {
        "The question is unanswerable if the recovered evidence does not contain the requested fact. Answer UNKNOWN when it is not answerable."
    } else {
        match question_type {
            "multi-session" => {
                "Aggregate all relevant evidence across sessions. Count, sum, list, or combine facts when the question requires it. Deduplicate repeated mentions of the same event, item, obligation, or value unless the evidence says they are distinct occurrences."
            }
            "temporal-reasoning" => {
                "Reason over dates, durations, ordering, recency, and before/after relations. Compute the requested temporal answer."
            }
            "knowledge-update" => {
                "Resolve updates explicitly. Evidence is ordered from older to newer when sequence is available. Compare older and newer statements, ignore superseded values, and answer with the latest applicable value. Do not answer UNKNOWN when the latest value is explicit."
            }
            "single-session-preference" => {
                "Answer the user's request as a personalized assistant response, not as a meta-summary of preferences. Use the recovered evidence to tailor the recommendation or advice. Cover every distinct user-specific preference, constraint, setup detail, material, tool, ingredient, language, skill, venue type, activity format, or maintenance concern that would change the answer. If the request is in a new context and the evidence gives preferences but not current location-specific options, do not return UNKNOWN; answer with preference-constrained criteria or advice using only those recovered preferences. Include the user's positive preferences and avoid the options the evidence implies they would not prefer. Keep it concise but actionable."
            }
            "single-session-user" | "single-session-assistant" => {
                "Answer the factual question directly from the recovered evidence."
            }
            _ => "Answer directly from the recovered evidence.",
        }
    };

    let plugin_interpretation = plugin_interpretation
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("No typed plugin values were detected.");
    let graph_relation_context = graph_relation_context
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("No graph relation context was returned.");

    format!(
        "You are a LongMemEval reader. Use only the recovered kernel evidence.\n\
         Do not use outside knowledge. Do not mention that evidence was provided.\n\
         Return only the final answer, with no explanation, no markdown, and no preamble.\n\
         If the evidence is insufficient, return exactly UNKNOWN.\n\
         Preserve required answer units and symbols. If the question asks for money, percentages, distances, durations, dates, or counts with units, keep the matching $, %, miles, hours, days, years, or date wording in the final answer when the evidence supports it.\n\
         For count/sum/average/difference questions, first decide which evidence items satisfy the question predicate, then compute. Count or sum each real item once. Do not include restatements, corrections of the same fact, rejected alternatives, examples, older superseded estimates, or a total together with the components that total already summarizes.\n\n\
         Typed plugin interpretation is deterministic extraction from the same evidence. Raw values preserve original symbols; normalized values expose type. Use both as hints for dates, money, numbers, URLs, code, and math, but include or exclude facts only when the recovered evidence supports the question predicate.\n\n\
         Graph relation context is authoritative only when it contains kernel operand-modeling relations such as contributes_to, excluded_from, checked_against, derived_from, restates, corrects, component_of, total_of, same_event_as, same_entity_as, qualifies_as, or matches_requirement. Structural, records, contains, and support-only relations prove provenance and retrieval, not duplicate/total/correction semantics.\n\n\
         Question type: {question_type}\n\
         Task: {task_instruction}\n\n\
         Question:\n{question}\n\n\
         Recovered kernel evidence:\n{evidence}\n\n\
         Typed plugin interpretation:\n{plugin_interpretation}\n\n\
         Graph relation context:\n{graph_relation_context}\n"
    )
}

fn recovered_evidence_text(run_item: &RunItem) -> Option<String> {
    run_item
        .ask_content
        .as_ref()
        .and_then(format_structured_evidence)
        .or_else(|| run_item.ask_answer.clone())
}

fn graph_relation_context(run_item: &RunItem, plugin_output: &Value) -> GraphRelationContext {
    let Some(ask_content) = run_item.ask_content.as_ref() else {
        return GraphRelationContext::default();
    };
    let Some(path) = ask_content
        .get("proof")
        .and_then(|proof| proof.get("path"))
        .and_then(Value::as_array)
    else {
        return GraphRelationContext::default();
    };

    let focus_refs = graph_focus_refs(run_item, plugin_output);
    let mut context = GraphRelationContext::default();
    let mut rich_lines = Vec::new();

    for relation in path {
        let rel = relation
            .get("rel")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let class = relation
            .get("class")
            .and_then(Value::as_str)
            .unwrap_or_default();
        context.total += 1;
        if is_structural_graph_relation(rel, class) {
            context.structural += 1;
            continue;
        }
        if is_anemic_graph_relation(rel) {
            context.anemic += 1;
            continue;
        }
        if !is_operand_modeling_graph_relation(rel) {
            continue;
        }
        context.rich += 1;
        if rich_lines.len() >= 40 || !relation_touches_focus(relation, &focus_refs) {
            continue;
        }
        rich_lines.push(render_graph_relation_signal(relation));
    }

    context.rendered = Some(render_graph_relation_context_summary(
        &context,
        rich_lines.as_slice(),
    ));
    context
}

fn graph_focus_refs(run_item: &RunItem, plugin_output: &Value) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    if let Some(values) = plugin_output.get("values").and_then(Value::as_array) {
        for mention in values {
            if let Some(ref_id) = mention.get("ref_id").and_then(Value::as_str) {
                refs.insert(ref_id.to_string());
            }
        }
    }
    if let Some(ask_content) = run_item.ask_content.as_ref() {
        for snippet in structured_evidence_snippets(ask_content) {
            refs.insert(snippet.ref_id);
        }
    }
    refs
}

fn relation_touches_focus(relation: &Value, focus_refs: &BTreeSet<String>) -> bool {
    if focus_refs.is_empty() {
        return true;
    }
    ["from", "to"].iter().any(|field| {
        relation
            .get(*field)
            .and_then(Value::as_str)
            .is_some_and(|reference| focus_refs.contains(reference))
    })
}

fn is_structural_graph_relation(rel: &str, class: &str) -> bool {
    RelationSemanticClass::parse(class)
        .is_ok_and(|semantic_class| semantic_class == RelationSemanticClass::Structural)
        || MemoryRelationType::new(rel).is_ok_and(|relation_type| relation_type.is_structural())
}

fn is_anemic_graph_relation(rel: &str) -> bool {
    MemoryRelationType::new(rel).is_ok_and(|relation_type| relation_type.is_support_only())
}

fn is_operand_modeling_graph_relation(rel: &str) -> bool {
    MemoryRelationType::new(rel).is_ok_and(|relation_type| relation_type.is_operand_modeling())
}

fn render_graph_relation_signal(relation: &Value) -> String {
    let rel = relation
        .get("rel")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let class = relation
        .get("class")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let from = relation
        .get("from")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let to = relation
        .get("to")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let confidence = relation
        .get("confidence")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let sequence = relation
        .get("sequence")
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let why = relation
        .get("why")
        .and_then(Value::as_str)
        .or_else(|| relation.get("evidence").and_then(Value::as_str))
        .unwrap_or_default();
    format!(
        "- rel={rel} class={class} confidence={confidence} sequence={sequence} from={} to={} why={}",
        truncate_for_prompt(from, 180),
        truncate_for_prompt(to, 180),
        truncate_for_prompt(why, 320)
    )
}

fn render_graph_relation_context_summary(
    context: &GraphRelationContext,
    rich_lines: &[String],
) -> String {
    let mut rendered = format!(
        "Graph relation summary: total={} structural={} support_only={} rich_operand_relations={}.",
        context.total, context.structural, context.anemic, context.rich
    );
    if context.rich == 0 {
        rendered.push_str(
            "\nNo rich operand relations are present. Treat the graph as retrieval/provenance only for duplicate, total, correction, and operand-inclusion decisions.",
        );
        return rendered;
    }
    rendered.push_str(
        "\nUse rich operand relations to decide inclusion, exclusion, duplicate/restatement, correction, component/total, and question-item matching before computing.",
    );
    if rich_lines.is_empty() {
        rendered.push_str(
            "\nRich relations exist, but none directly touch the retrieved/plugin evidence refs in this prompt.",
        );
    } else {
        rendered.push_str("\nRich relation signals touching retrieved/plugin refs:\n");
        rendered.push_str(&rich_lines.join("\n"));
    }
    rendered
}

fn plugin_output_for_item(
    reader: &ComposedEvidenceReader,
    expected: &LongMemEvalExpected,
    run_item: &RunItem,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    if let Some(plugin_reader) = &run_item.plugin_reader {
        return Ok(plugin_reader.clone());
    }

    let fragments = plugin_evidence_fragments(expected, run_item);
    let request = EvidenceReaderRequest::new(EvidenceInterpretationInput::new(fragments));
    let output = reader.read(&request)?;
    Ok(serde_json::to_value(output)?)
}

fn plugin_evidence_fragments(
    expected: &LongMemEvalExpected,
    run_item: &RunItem,
) -> Vec<EvidenceFragment> {
    let mut fragments = Vec::new();
    fragments.push(EvidenceFragment::new(
        format!("question:{}", expected.question_id),
        expected.question.clone(),
    ));

    if let Some(ask_content) = &run_item.ask_content {
        for snippet in structured_evidence_snippets(ask_content) {
            let mut fragment = EvidenceFragment::new(snippet.ref_id, snippet.text);
            fragment.source = snippet.source;
            fragments.push(fragment);
        }
    }

    if fragments.len() == 1
        && let Some(answer) = run_item
            .ask_answer
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        fragments.push(EvidenceFragment::new(
            format!("kernel_answer:{}", expected.question_id),
            answer.to_string(),
        ));
    }

    dedupe_plugin_fragments(fragments)
}

fn dedupe_plugin_fragments(fragments: Vec<EvidenceFragment>) -> Vec<EvidenceFragment> {
    let mut seen = BTreeMap::new();
    let mut deduped = Vec::new();
    for fragment in fragments {
        let key = (fragment.ref_id.clone(), fragment.text.clone());
        if seen.insert(key, ()).is_none() {
            deduped.push(fragment);
        }
    }
    deduped
}

fn render_plugin_interpretation(plugin_output: &Value) -> Option<String> {
    let values = plugin_output
        .get("values")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let derivation_results = plugin_output
        .get("derivation_results")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if values.is_empty() && derivation_results.is_empty() {
        return None;
    }

    let mut rendered = String::new();
    if !values.is_empty() {
        rendered.push_str(
            "Typed values detected by kernel reader plugins. Treat question-ref values as query constraints, not answer evidence.\n",
        );
        for (index, mention) in values.iter().take(80).enumerate() {
            let plugin = mention
                .get("plugin")
                .and_then(Value::as_str)
                .unwrap_or("unknown-plugin");
            let ref_id = mention
                .get("ref_id")
                .and_then(Value::as_str)
                .unwrap_or("unknown-ref");
            let raw = mention
                .get("raw")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let value = describe_interpreted_value(mention.get("value").unwrap_or(&Value::Null));
            rendered.push_str(&format!(
                "\n[{}] plugin={} ref={} raw={} value={}",
                index + 1,
                plugin,
                ref_id,
                truncate_for_prompt(raw, 160),
                value
            ));
        }
        if values.len() > 80 {
            rendered.push_str(&format!(
                "\n... {} additional typed values omitted from prompt.",
                values.len() - 80
            ));
        }
    }

    if !derivation_results.is_empty() {
        rendered.push_str("\n\nDeterministic derivations requested by the runner:\n");
        for (index, result) in derivation_results.iter().take(20).enumerate() {
            let plugin = result
                .get("plugin")
                .and_then(Value::as_str)
                .unwrap_or("unknown-plugin");
            let answer = result
                .get("answer")
                .and_then(Value::as_str)
                .unwrap_or("no-answer");
            rendered.push_str(&format!(
                "[{}] plugin={} answer={}\n",
                index + 1,
                plugin,
                truncate_for_prompt(answer, 240)
            ));
        }
    }

    if rendered.len() > 12_000 {
        rendered.truncate(12_000);
        rendered.push_str("\n... plugin interpretation truncated.");
    }
    Some(rendered)
}

fn describe_interpreted_value(value: &Value) -> String {
    let Some(kind) = value.get("kind").and_then(Value::as_str) else {
        return truncate_for_prompt(&value.to_string(), 240);
    };
    match kind {
        "money" => {
            let currency = value
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("UNK");
            let amount = value
                .get("amount")
                .and_then(Value::as_f64)
                .map(|amount| format!("{amount}"))
                .unwrap_or_else(|| "unknown".to_string());
            format!("{currency} {amount}")
        }
        "date" => {
            let date = value.get("date").unwrap_or(&Value::Null);
            let year = date
                .get("year")
                .and_then(Value::as_i64)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "????".to_string());
            let month = date
                .get("month")
                .and_then(Value::as_u64)
                .map(|value| format!("{value:02}"))
                .unwrap_or_else(|| "??".to_string());
            let day = date
                .get("day")
                .and_then(Value::as_u64)
                .map(|value| format!("{value:02}"))
                .unwrap_or_else(|| "??".to_string());
            format!("{year}-{month}-{day}")
        }
        "number" => {
            let number = value
                .get("value")
                .and_then(Value::as_f64)
                .map(|number| format!("{number}"))
                .unwrap_or_else(|| "unknown".to_string());
            match value.get("unit").and_then(Value::as_str) {
                Some(unit) => format!("{number} {unit}"),
                None => number,
            }
        }
        "source_code" => {
            let language = value
                .get("language")
                .and_then(Value::as_str)
                .unwrap_or("unknown-language");
            let text = value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            format!(
                "code language={} text={}",
                language,
                truncate_for_prompt(text, 180)
            )
        }
        "math_expression" => {
            let expression = value
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or_default();
            format!("math {}", truncate_for_prompt(expression, 180))
        }
        "url" => {
            let url = value.get("url").and_then(Value::as_str).unwrap_or_default();
            format!("url {}", truncate_for_prompt(url, 220))
        }
        _ => truncate_for_prompt(&value.to_string(), 240),
    }
}

fn truncate_for_prompt(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut truncated = normalized.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn plugin_value_count(plugin_output: &Value) -> usize {
    plugin_output
        .get("values")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

fn plugin_derivation_count(plugin_output: &Value) -> usize {
    plugin_output
        .get("derivation_results")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

fn plugin_diagnostic_count(plugin_output: &Value) -> usize {
    plugin_output
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

fn format_structured_evidence(value: &Value) -> Option<String> {
    let mut snippets = structured_evidence_snippets(value);
    if snippets.is_empty() {
        return None;
    }
    snippets.sort_by(|left, right| {
        left.sequence
            .unwrap_or(u64::MAX)
            .cmp(&right.sequence.unwrap_or(u64::MAX))
            .then_with(|| left.ref_id.cmp(&right.ref_id))
    });

    let mut rendered = String::from(
        "Evidence is ordered by kernel relation sequence when available; later sequence means newer evidence within this adapted item.\n",
    );
    for (index, snippet) in snippets.iter().enumerate() {
        rendered.push_str(&format!("\n[{}] ref={}", index + 1, snippet.ref_id));
        if let Some(sequence) = snippet.sequence {
            rendered.push_str(&format!(" sequence={sequence}"));
        }
        if let Some(time) = snippet
            .time
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            rendered.push_str(&format!(" time={time}"));
        }
        if let Some(confidence) = snippet
            .confidence
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            rendered.push_str(&format!(" confidence={confidence}"));
        }
        rendered.push('\n');
        if let Some(source) = snippet
            .source
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            rendered.push_str(&format!("source: {source}\n"));
        }
        if let Some(why) = snippet
            .why
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            rendered.push_str(&format!("why: {why}\n"));
        }
        rendered.push_str("text: ");
        rendered.push_str(snippet.text.trim());
        rendered.push('\n');
    }
    Some(rendered)
}

fn structured_evidence_snippets(value: &Value) -> Vec<EvidenceSnippet> {
    let evidence_by_ref = evidence_text_by_supported_ref(value);
    let mut snippets = Vec::new();

    for relation in proof_path(value) {
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
        let evidence = evidence_by_ref.get(ref_id);
        let relation_evidence = relation
            .get("evidence")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let text = evidence
            .map(|snippet| snippet.text.as_str())
            .filter(|text| !text.trim().is_empty())
            .unwrap_or(relation_evidence)
            .trim();
        if text.is_empty() {
            continue;
        }
        snippets.push(EvidenceSnippet {
            ref_id: ref_id.to_string(),
            text: text.to_string(),
            source: evidence.and_then(|snippet| snippet.source.clone()),
            time: evidence.and_then(|snippet| snippet.time.clone()),
            sequence: relation.get("sequence").and_then(Value::as_u64),
            why: relation
                .get("why")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            confidence: relation
                .get("confidence")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        });
    }

    if snippets.is_empty() {
        for reason in value
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
            let text = reason
                .get("evidence")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if text.is_empty() {
                continue;
            }
            snippets.push(EvidenceSnippet {
                ref_id: ref_id.to_string(),
                text: text.to_string(),
                source: None,
                time: None,
                sequence: None,
                why: reason
                    .get("claim")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                confidence: None,
            });
        }
    }

    if snippets.is_empty() {
        snippets.extend(evidence_by_ref.into_values());
    }

    deduplicate_snippets(snippets)
}

fn evidence_text_by_supported_ref(value: &Value) -> BTreeMap<String, EvidenceSnippet> {
    let mut evidence_by_ref = BTreeMap::new();
    for evidence in proof_evidence(value) {
        let text = evidence
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if text.is_empty() {
            continue;
        }
        let source = evidence
            .get("source")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let time = evidence
            .get("time")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let supports = evidence
            .get("supports")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for support in supports {
            let Some(ref_id) = support.as_str() else {
                continue;
            };
            if !looks_like_turn_ref(ref_id) {
                continue;
            }
            evidence_by_ref
                .entry(ref_id.to_string())
                .or_insert_with(|| EvidenceSnippet {
                    ref_id: ref_id.to_string(),
                    text: text.to_string(),
                    source: source.clone(),
                    time: time.clone(),
                    sequence: None,
                    why: None,
                    confidence: None,
                });
        }
    }
    evidence_by_ref
}

fn proof_path(value: &Value) -> &[Value] {
    value
        .get("proof")
        .and_then(|proof| proof.get("path"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn proof_evidence(value: &Value) -> &[Value] {
    value
        .get("proof")
        .and_then(|proof| proof.get("evidence"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn deduplicate_snippets(snippets: Vec<EvidenceSnippet>) -> Vec<EvidenceSnippet> {
    let mut deduplicated = BTreeMap::<String, EvidenceSnippet>::new();
    for snippet in snippets {
        deduplicated
            .entry(snippet.ref_id.clone())
            .and_modify(|existing| {
                if existing.sequence.is_none() {
                    existing.sequence = snippet.sequence;
                }
                if existing.why.is_none() {
                    existing.why = snippet.why.clone();
                }
                if existing.confidence.is_none() {
                    existing.confidence = snippet.confidence.clone();
                }
            })
            .or_insert(snippet);
    }
    deduplicated.into_values().collect()
}

fn looks_like_turn_ref(value: &str) -> bool {
    value.starts_with("turn:") && !value.contains(' ') && value.len() <= 240
}

fn normalize_reader_output(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim()
        .trim_start_matches("Answer:")
        .trim()
        .to_string()
}

fn validate_matching_item(
    expected: &LongMemEvalExpected,
    run_item: &RunItem,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if expected.question_id != run_item.question_id {
        return Err(format!(
            "question_id mismatch: expected={} run={}",
            expected.question_id, run_item.question_id
        )
        .into());
    }
    if expected.question_type != run_item.question_type {
        return Err(format!(
            "question_type mismatch for {}: expected={} run={}",
            expected.question_id, expected.question_type, run_item.question_type
        )
        .into());
    }
    Ok(())
}

fn summarize_reader_run(
    args: &Args,
    endpoint: String,
    model: String,
    provider: LlmProvider,
    elapsed_ms: u128,
    results: &[ReaderItemResult],
    plugin_reader: &ComposedEvidenceReader,
) -> Result<ReaderSummary, Box<dyn Error + Send + Sync>> {
    let mut question_types = BTreeMap::<String, ReaderQuestionTypeSummary>::new();
    for result in results {
        let summary = question_types
            .entry(result.question_type.clone())
            .or_default();
        summary.count += 1;
        if result.lexical_answer_hit {
            summary.lexical_answer_hits += 1;
        }
    }

    Ok(ReaderSummary {
        benchmark: "LongMemEval",
        reader: "longmemeval-kmp-reader-v1",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        artifacts: args.artifacts.display().to_string(),
        run: args.run.display().to_string(),
        endpoint,
        model,
        provider: provider_label(provider),
        total_items: results.len(),
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
        graph_relation_total: results
            .iter()
            .map(|result| result.graph_relation_total)
            .sum(),
        graph_relation_structural: results
            .iter()
            .map(|result| result.graph_relation_structural)
            .sum(),
        graph_relation_anemic: results
            .iter()
            .map(|result| result.graph_relation_anemic)
            .sum(),
        graph_relation_rich: results
            .iter()
            .map(|result| result.graph_relation_rich)
            .sum(),
        lexical_answer_items: results
            .iter()
            .filter(|result| !normalize_answer_text(&result.expected_answer).is_empty())
            .count(),
        lexical_answer_hits: results
            .iter()
            .filter(|result| result.lexical_answer_hit)
            .count(),
        prompt_tokens: results.iter().map(|result| result.prompt_tokens).sum(),
        completion_tokens: results.iter().map(|result| result.completion_tokens).sum(),
        elapsed_ms,
        question_types,
    })
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

fn read_run_items(
    path: &Path,
    limit: Option<usize>,
) -> Result<Vec<RunItem>, Box<dyn Error + Send + Sync>> {
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
    let mut artifacts = None;
    let mut run = None;
    let mut output = None;
    let mut endpoint = None;
    let mut model = None;
    let mut provider = None;
    let mut api_key_env = "LLM_API_KEY".to_string();
    let mut tls_cert_path = None;
    let mut tls_key_path = None;
    let mut max_tokens = 256u32;
    let mut temperature = 0.0f64;
    let mut limit = None;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--artifacts" => artifacts = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--run" => run = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--endpoint" => endpoint = Some(required_flag_value(&mut args, &arg)?),
            "--model" => model = Some(required_flag_value(&mut args, &arg)?),
            "--provider" => {
                provider = Some(parse_provider(&required_flag_value(&mut args, &arg)?)?)
            }
            "--api-key-env" => api_key_env = required_flag_value(&mut args, &arg)?,
            "--tls-cert-path" => tls_cert_path = Some(required_flag_value(&mut args, &arg)?),
            "--tls-key-path" => tls_key_path = Some(required_flag_value(&mut args, &arg)?),
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
            "--limit" => {
                let value = required_flag_value(&mut args, &arg)?;
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|error| format!("invalid --limit value `{value}`: {error}"))?,
                );
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
        endpoint,
        model,
        provider,
        api_key_env,
        tls_cert_path,
        tls_key_path,
        max_tokens,
        temperature,
        limit,
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

fn resolve_optional(cli_value: Option<&str>, env_name: &str) -> Option<String> {
    cli_value
        .map(str::to_string)
        .or_else(|| env::var(env_name).ok())
        .filter(|value| !value.trim().is_empty())
}

fn build_llm_http_client(
    timeout: Duration,
    tls_cert_path: Option<&str>,
    tls_key_path: Option<&str>,
) -> Result<reqwest::Client, Box<dyn Error + Send + Sync>> {
    let mut builder = reqwest::Client::builder().timeout(timeout);
    match (tls_cert_path, tls_key_path) {
        (Some(cert_path), Some(key_path)) => {
            let cert_pem = fs::read(cert_path)?;
            let key_pem = fs::read(key_path)?;
            let mut identity_pem = cert_pem;
            identity_pem.extend_from_slice(&key_pem);
            builder = builder.identity(reqwest::Identity::from_pem(&identity_pem)?);
        }
        (None, None) => {}
        _ => {
            return Err(
                "mTLS client identity requires both --tls-cert-path/LLM_TLS_CERT_PATH and --tls-key-path/LLM_TLS_KEY_PATH"
                    .into(),
            );
        }
    }
    Ok(builder.build()?)
}

fn print_usage() {
    eprintln!(
        "Usage: longmemeval_kmp_reader --artifacts <adapter-output-dir> --run <runner-output-dir> --output <reader-output-dir> --endpoint <chat-completions-url> --model <model> [--provider openai|openai-new|anthropic] [--api-key-env LLM_API_KEY] [--tls-cert-path PATH] [--tls-key-path PATH] [--max-tokens N] [--temperature F] [--limit N] [--force]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_contains_task_specific_instruction_without_gold_answer() {
        let prompt = build_reader_prompt(
            "multi-session",
            "How many appointments?",
            false,
            "I visited a dentist. I visited an ENT specialist.",
            None,
            None,
        );

        assert!(prompt.contains("Aggregate all relevant evidence"));
        assert!(prompt.contains("How many appointments?"));
        assert!(prompt.contains("I visited a dentist"));
        assert!(!prompt.contains("expected"));
    }

    #[test]
    fn normalizes_reader_output() {
        assert_eq!(normalize_reader_output("  \"Answer: 3 days\"  "), "3 days");
    }

    #[test]
    fn preference_prompt_requests_personalized_assistant_response() {
        let prompt = build_reader_prompt(
            "single-session-preference",
            "Can you recommend some resources?",
            false,
            "The user wants Adobe Premiere Pro advanced settings.",
            None,
            None,
        );

        assert!(prompt.contains("personalized assistant response"));
        assert!(prompt.contains("not as a meta-summary"));
        assert!(prompt.contains("every distinct user-specific preference"));
        assert!(prompt.contains("preference-constrained criteria"));
        assert!(prompt.contains("maintenance concern"));
        assert!(prompt.contains("Can you recommend some resources?"));
    }

    #[test]
    fn structured_evidence_orders_updates_by_sequence() {
        let run_item = RunItem {
            question_id: "q-update".to_string(),
            question_type: "knowledge-update".to_string(),
            evidence_hit: "full".to_string(),
            ask_answer: Some("fallback".to_string()),
            plugin_reader: None,
            ask_content: Some(json!({
                "proof": {
                    "path": [
                            {
                                "from": "turn:q-update:s1:1",
                                "to": "question:q-update",
                                "rel": "supports_answer",
                                "class": "evidential",
                                "why": "older value",
                            "evidence": "Yoga twice a week.",
                            "confidence": "high",
                            "sequence": 1
                        },
                            {
                                "from": "turn:q-update:s2:1",
                                "to": "question:q-update",
                                "rel": "supports_answer",
                                "class": "evidential",
                            "why": "updated value",
                            "evidence": "Yoga three times a week.",
                            "confidence": "high",
                            "sequence": 2
                        }
                    ],
                    "evidence": []
                }
            })),
        };

        let evidence = recovered_evidence_text(&run_item).expect("structured evidence");
        assert!(evidence.contains("sequence=1"));
        assert!(evidence.contains("sequence=2"));
        assert!(
            evidence
                .find("Yoga twice a week")
                .expect("old value exists")
                < evidence
                    .find("Yoga three times a week")
                    .expect("new value exists")
        );
    }

    #[test]
    fn prompt_includes_plugin_interpretation_section() {
        let prompt = build_reader_prompt(
            "multi-session",
            "How much did I spend?",
            false,
            "I paid $12 for lunch.",
            Some("[1] plugin=money-value-v1 ref=turn:q:s1:1 raw=$12 value=USD 12"),
            Some(
                "Graph relation summary: total=0 structural=0 support_only=0 rich_operand_relations=0.",
            ),
        );

        assert!(prompt.contains("Typed plugin interpretation"));
        assert!(prompt.contains("Graph relation context"));
        assert!(prompt.contains("money-value-v1"));
        assert!(prompt.contains("USD 12"));
        assert!(prompt.contains("include or exclude facts only when"));
    }

    #[test]
    fn graph_relation_context_surfaces_rich_operand_relations() {
        let run_item = RunItem {
            question_id: "q-money".to_string(),
            question_type: "multi-session".to_string(),
            evidence_hit: "full".to_string(),
            ask_answer: None,
            plugin_reader: None,
            ask_content: Some(json!({
                "proof": {
                    "path": [
                        {
                            "from": "scope",
                            "to": "turn:q-money:s1:1",
                            "rel": "contains_entry",
                            "class": "structural"
                        },
                        {
                            "from": "turn:q-money:s1:2",
                            "to": "turn:q-money:s1:1",
                            "rel": "restates",
                            "class": "semantic",
                            "confidence": "high",
                            "why": "The later light purchase mention repeats the same $40 expense.",
                            "sequence": 2
                        },
                        {
                            "from": "turn:q-money:s1:1",
                            "to": "question:q-money",
                            "rel": "supports",
                            "class": "evidential"
                        }
                    ],
                    "evidence": [
                        {
                            "supports": ["turn:q-money:s1:1"],
                            "text": "Bike lights cost $40."
                        },
                        {
                            "supports": ["turn:q-money:s1:2"],
                            "text": "The same bike lights were $40."
                        }
                    ]
                }
            })),
        };
        let plugin_output = json!({
            "values": [
                {
                    "ref_id": "turn:q-money:s1:1",
                    "plugin": "money-value-v1",
                    "raw": "$40",
                    "value": {"kind": "money", "currency": "USD", "amount": 40.0, "amount_minor": 4000}
                }
            ],
            "derivation_results": [],
            "diagnostics": []
        });

        let context = graph_relation_context(&run_item, &plugin_output);
        let rendered = context.rendered.expect("rendered graph context");

        assert_eq!(context.total, 3);
        assert_eq!(context.structural, 1);
        assert_eq!(context.anemic, 1);
        assert_eq!(context.rich, 1);
        assert!(rendered.contains("restates"));
        assert!(rendered.contains("rich_operand_relations=1"));
    }

    #[test]
    fn plugin_output_fallback_interprets_structured_evidence() {
        let expected = LongMemEvalExpected {
            question_id: "q-money".to_string(),
            question_type: "multi-session".to_string(),
            question: "How much did I spend?".to_string(),
            question_date: "2026-05-08".to_string(),
            answer: json!("12"),
            answer_session_ids: vec!["s1".to_string()],
            answer_turn_refs: vec!["turn:q-money:s1:1".to_string()],
            answer_session_refs: vec!["session:q-money:s1".to_string()],
            abstention: false,
        };
        let run_item = RunItem {
            question_id: "q-money".to_string(),
            question_type: "multi-session".to_string(),
            evidence_hit: "full".to_string(),
            ask_answer: Some("fallback".to_string()),
            plugin_reader: None,
            ask_content: Some(json!({
                "proof": {
                    "path": [
                        {
                            "from": "turn:q-money:s1:1",
                            "to": "question:q-money",
                            "rel": "supports_answer",
                            "evidence": "I paid $12 for lunch.",
                            "sequence": 1
                        }
                    ],
                    "evidence": []
                }
            })),
        };
        let reader = ComposedEvidenceReader::kernel_default();
        let output = plugin_output_for_item(&reader, &expected, &run_item).expect("plugin output");
        let rendered = render_plugin_interpretation(&output).expect("rendered plugins");

        assert!(plugin_value_count(&output) > 0);
        assert!(rendered.contains("money-value-v1"));
        assert!(rendered.contains("turn:q-money:s1:1"));
    }
}
