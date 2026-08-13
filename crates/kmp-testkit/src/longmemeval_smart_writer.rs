use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::time::{Duration, Instant};

use kmp_domain::{
    KnownMemoryRelationType, MemoryRelationQuality, MemoryRelationType, RelationSemanticClass,
};
use kmp_mcp::KernelMcpServer;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{LlmProvider, call_llm, normalize_llm_json_response};

const DEFAULT_WRITER_ACTOR: &str = "agent:longmemeval-smart-writer";
const DEFAULT_SOURCE_KIND: &str = "agent";
const MAX_CURRENT_SUMMARY_CHARS: usize = 1600;
const MAX_CURRENT_EVIDENCE_CHARS: usize = 1600;
const MAX_RELATION_EVIDENCE_CHARS: usize = 900;
const MAX_RELATION_WHY_CHARS: usize = 700;
const MAX_PROMPT_TARGET_TEXT_CHARS: usize = 1200;

#[derive(Debug, Clone)]
pub struct LongMemEvalSmartWriterConfig {
    pub llm_endpoint: Option<String>,
    pub llm_model: Option<String>,
    pub llm_provider: Option<LlmProvider>,
    pub api_key: Option<String>,
    pub max_tokens: u32,
    pub temperature: f64,
    pub log_mcp_navigation: bool,
}

impl LongMemEvalSmartWriterConfig {
    pub fn llm_enabled(&self) -> bool {
        self.llm_endpoint
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && self
                .llm_model
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    }
}

#[derive(Debug)]
pub struct LongMemEvalSmartWriter {
    config: LongMemEvalSmartWriterConfig,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Copy)]
pub struct LongMemEvalSmartWriterItem<'a> {
    pub item_index: usize,
    pub question_id: &'a str,
    pub question_type: &'a str,
    pub about: &'a str,
    pub arguments: &'a Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongMemEvalSmartWriterResult {
    pub item_index: usize,
    pub question_id: String,
    pub question_type: String,
    pub about: String,
    pub entry_ref: String,
    pub target_ref: String,
    pub entry_kind: String,
    pub relation_strategy: String,
    pub pre_read_calls: Vec<LongMemEvalSmartWriterToolCall>,
    pub write_request: Value,
    pub dry_run_content: Value,
    pub commit_content: Value,
    pub verify_content: Value,
    pub relation_quality: Vec<Value>,
    pub relation_quality_metrics: Value,
    pub llm_used: bool,
    pub llm_output_valid: bool,
    pub llm_raw: Option<String>,
    pub llm_prompt_chars: usize,
    pub llm_prompt_tokens: u32,
    pub llm_completion_tokens: u32,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongMemEvalSmartWriterToolCall {
    pub tool: String,
    pub arguments: Value,
    pub elapsed_ms: u128,
    pub content: Value,
    pub observed_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongMemEvalSmartWriterSummary {
    pub enabled: bool,
    pub relation_writes: usize,
    pub pre_read_calls: usize,
    pub dry_run_calls: usize,
    pub commit_calls: usize,
    pub verify_calls: usize,
    pub llm_calls: usize,
    pub llm_valid_outputs: usize,
    pub llm_invalid_outputs: usize,
    pub deterministic_fallbacks: usize,
    pub relation_total: usize,
    pub relation_rich_count: usize,
    pub relation_anemic_count: usize,
    pub relation_structural_count: usize,
    pub relation_suspect_count: usize,
}

#[derive(Debug, Clone)]
struct RelationWriteInput {
    question_id: String,
    question_type: String,
    about: String,
    entry_ref: String,
    target_ref: String,
    entry_kind: String,
    text: String,
    target_text: Option<String>,
    observed_at: String,
    process_scope: String,
    task_scope: Option<String>,
    sequence: u32,
    original_rel: String,
    original_class: String,
}

#[derive(Debug, Clone, Default)]
struct ReadContextPlan {
    inspected_refs: BTreeSet<String>,
    temporal_refs: BTreeSet<String>,
    calls: Vec<LongMemEvalSmartWriterToolCall>,
}

#[derive(Debug, Clone)]
struct RelationProposal {
    connect_to: Vec<Value>,
    strategy: String,
    llm_raw: Option<String>,
    llm_prompt_chars: usize,
    llm_prompt_tokens: u32,
    llm_completion_tokens: u32,
    llm_used: bool,
    llm_output_valid: bool,
}

#[derive(Debug, Deserialize)]
struct LlmConnectTo {
    r#ref: String,
    rel: String,
    class: String,
    why: String,
    evidence: String,
    #[serde(default)]
    confidence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LlmWriterProposal {
    connect_to: Vec<LlmConnectTo>,
}

impl LongMemEvalSmartWriter {
    pub fn new(config: LongMemEvalSmartWriterConfig) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;
        Ok(Self { config, client })
    }

    pub async fn write_ingest_item(
        &self,
        server: &KernelMcpServer,
        request_id: &mut u64,
        item: LongMemEvalSmartWriterItem<'_>,
    ) -> Result<Vec<LongMemEvalSmartWriterResult>, Box<dyn Error + Send + Sync>> {
        let started = Instant::now();
        let entries = memory_array(item.arguments, "entries")?;
        let relations = memory_array(item.arguments, "relations")?;
        let entries_by_ref = entries_by_ref(entries)?;
        let mut results = Vec::new();

        for relation in relations {
            let Some(mut input) = relation_write_input(item, relation, &entries_by_ref)? else {
                continue;
            };
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "longmemeval_smart_writer.relation.start",
                    "item_index": item.item_index,
                    "question_id": item.question_id,
                    "question_type": item.question_type,
                    "about": item.about,
                    "entry_ref": input.entry_ref.as_str(),
                    "target_ref": input.target_ref.as_str(),
                    "original_rel": input.original_rel.as_str()
                }),
            );

            let read_context = self
                .read_context_for_target(server, request_id, &input)
                .await?;
            attach_target_text(&mut input, &read_context);
            let mut proposal = self.propose_relation(&input, &read_context).await?;
            let mut request = write_memory_request(&input, &proposal, &read_context);

            let dry_run_call = match call_writer_tool_with_record(
                server,
                request_id,
                "kernel_write_memory",
                &request,
            )
            .await
            {
                Ok(call) => call,
                Err(error) if proposal.llm_used => {
                    proposal =
                        fallback_after_rejected_llm_plan(&input, proposal, &error.to_string());
                    request = write_memory_request(&input, &proposal, &read_context);
                    call_writer_tool_with_record(
                        server,
                        request_id,
                        "kernel_write_memory",
                        &request,
                    )
                    .await?
                }
                Err(error) => return Err(error),
            };
            let dry_run_content = dry_run_call.content.clone();

            set_dry_run(&mut request, false)?;
            let commit_call =
                call_writer_tool_with_record(server, request_id, "kernel_write_memory", &request)
                    .await?;
            let commit_content = commit_call.content.clone();

            let verify_arguments = json!({
                "ref": input.entry_ref,
                "include": {
                    "incoming": true,
                    "outgoing": true,
                    "details": true,
                    "raw": false
                }
            });
            let verify_call = call_writer_tool_with_record(
                server,
                request_id,
                "kernel_inspect",
                &verify_arguments,
            )
            .await?;
            let verify_content = verify_call.content.clone();

            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "longmemeval_smart_writer.relation.done",
                    "item_index": item.item_index,
                    "question_id": item.question_id,
                    "about": item.about,
                    "entry_ref": input.entry_ref.as_str(),
                    "target_ref": input.target_ref.as_str(),
                    "relation_strategy": proposal.strategy.as_str(),
                    "relation_quality_metrics": dry_run_content.get("relation_quality_metrics").cloned().unwrap_or_else(|| json!({}))
                }),
            );

            results.push(LongMemEvalSmartWriterResult {
                item_index: item.item_index,
                question_id: item.question_id.to_string(),
                question_type: item.question_type.to_string(),
                about: item.about.to_string(),
                entry_ref: input.entry_ref.clone(),
                target_ref: input.target_ref.clone(),
                entry_kind: input.entry_kind.clone(),
                relation_strategy: proposal.strategy,
                pre_read_calls: read_context.calls,
                write_request: request,
                relation_quality: dry_run_content
                    .get("relation_quality")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                relation_quality_metrics: dry_run_content
                    .get("relation_quality_metrics")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
                dry_run_content,
                commit_content,
                verify_content,
                llm_used: proposal.llm_used,
                llm_output_valid: proposal.llm_output_valid,
                llm_raw: proposal.llm_raw,
                llm_prompt_chars: proposal.llm_prompt_chars,
                llm_prompt_tokens: proposal.llm_prompt_tokens,
                llm_completion_tokens: proposal.llm_completion_tokens,
                elapsed_ms: started.elapsed().as_millis(),
            });
        }

        Ok(results)
    }

    async fn read_context_for_target(
        &self,
        server: &KernelMcpServer,
        request_id: &mut u64,
        input: &RelationWriteInput,
    ) -> Result<ReadContextPlan, Box<dyn Error + Send + Sync>> {
        let mut context = ReadContextPlan::default();
        let near = call_writer_tool_with_record(
            server,
            request_id,
            "kernel_near",
            &json!({
                "about": input.about,
                "around": {"ref": input.target_ref},
                "window": {"before_entries": 8, "after_entries": 0},
                "limit": {"entries": 12, "tokens": 2400},
                "dimensions": {"scope": "current_about", "mode": "all"},
                "include": {"evidence": true, "relations": true, "raw_refs": false},
                "budget": {"tokens": 2400, "depth": 2}
            }),
        )
        .await?;
        for ref_id in collect_memory_refs(&near.content) {
            if is_longmemeval_entry_ref(&ref_id) {
                context.temporal_refs.insert(ref_id);
            }
        }
        context.calls.push(near);

        let inspect = call_writer_tool_with_record(
            server,
            request_id,
            "kernel_inspect",
            &json!({
                "ref": input.target_ref,
                "include": {
                    "incoming": true,
                    "outgoing": true,
                    "details": true,
                    "raw": false
                }
            }),
        )
        .await?;
        context.inspected_refs.insert(input.target_ref.clone());
        context.calls.push(inspect);

        Ok(context)
    }

    async fn propose_relation(
        &self,
        input: &RelationWriteInput,
        read_context: &ReadContextPlan,
    ) -> Result<RelationProposal, Box<dyn Error + Send + Sync>> {
        if self.config.llm_enabled() {
            let prompt = writer_prompt(input, read_context);
            let endpoint = self.config.llm_endpoint.as_deref().unwrap_or_default();
            let model = self.config.llm_model.as_deref().unwrap_or_default();
            let provider = self
                .config
                .llm_provider
                .unwrap_or_else(|| detect_provider_from_model(model));
            let (raw, prompt_tokens, completion_tokens) = call_llm(
                &self.client,
                endpoint,
                model,
                provider,
                self.config.api_key.as_deref(),
                &prompt,
                self.config.max_tokens,
                self.config.temperature,
            )
            .await?;

            if let Ok(connect_to) = parse_llm_connect_to(&raw, input) {
                return Ok(RelationProposal {
                    connect_to,
                    strategy: "llm".to_string(),
                    llm_raw: Some(raw),
                    llm_prompt_chars: prompt.chars().count(),
                    llm_prompt_tokens: prompt_tokens,
                    llm_completion_tokens: completion_tokens,
                    llm_used: true,
                    llm_output_valid: true,
                });
            }

            return Ok(RelationProposal {
                connect_to: fallback_connect_to(input),
                strategy: "deterministic_fallback_after_invalid_llm_output".to_string(),
                llm_raw: Some(raw),
                llm_prompt_chars: prompt.chars().count(),
                llm_prompt_tokens: prompt_tokens,
                llm_completion_tokens: completion_tokens,
                llm_used: true,
                llm_output_valid: false,
            });
        }

        Ok(RelationProposal {
            connect_to: fallback_connect_to(input),
            strategy: "deterministic_fallback".to_string(),
            llm_raw: None,
            llm_prompt_chars: 0,
            llm_prompt_tokens: 0,
            llm_completion_tokens: 0,
            llm_used: false,
            llm_output_valid: false,
        })
    }
}

pub fn summarize_longmemeval_smart_writer(
    enabled: bool,
    results: &[LongMemEvalSmartWriterResult],
) -> LongMemEvalSmartWriterSummary {
    let mut relation_total = 0usize;
    let mut relation_rich_count = 0usize;
    let mut relation_anemic_count = 0usize;
    let mut relation_structural_count = 0usize;
    let mut relation_suspect_count = 0usize;
    for result in results {
        relation_total += metric_usize(&result.relation_quality_metrics, "relation_total");
        relation_rich_count +=
            metric_usize(&result.relation_quality_metrics, "relation_rich_count");
        relation_anemic_count +=
            metric_usize(&result.relation_quality_metrics, "relation_anemic_count");
        relation_structural_count += metric_usize(
            &result.relation_quality_metrics,
            "relation_structural_count",
        );
        relation_suspect_count +=
            metric_usize(&result.relation_quality_metrics, "relation_suspect_count");
    }

    LongMemEvalSmartWriterSummary {
        enabled,
        relation_writes: results.len(),
        pre_read_calls: results
            .iter()
            .map(|result| result.pre_read_calls.len())
            .sum(),
        dry_run_calls: results.len(),
        commit_calls: results.len(),
        verify_calls: results.len(),
        llm_calls: results.iter().filter(|result| result.llm_used).count(),
        llm_valid_outputs: results
            .iter()
            .filter(|result| result.llm_used && result.llm_output_valid)
            .count(),
        llm_invalid_outputs: results
            .iter()
            .filter(|result| result.llm_used && !result.llm_output_valid)
            .count(),
        deterministic_fallbacks: results
            .iter()
            .filter(|result| result.relation_strategy.contains("fallback"))
            .count(),
        relation_total,
        relation_rich_count,
        relation_anemic_count,
        relation_structural_count,
        relation_suspect_count,
    }
}

fn relation_write_input(
    item: LongMemEvalSmartWriterItem<'_>,
    relation: &Value,
    entries_by_ref: &BTreeMap<String, Value>,
) -> Result<Option<RelationWriteInput>, Box<dyn Error + Send + Sync>> {
    let relation = relation
        .as_object()
        .ok_or("memory.relations[] must contain objects")?;
    let original_rel = required_string(relation, "rel")?.to_string();
    if original_rel != "supports_answer" && original_rel != "supports" {
        return Ok(None);
    }
    let entry_ref = required_string(relation, "from")?.to_string();
    let target_ref = required_string(relation, "to")?.to_string();
    let entry = entries_by_ref
        .get(&entry_ref)
        .ok_or_else(|| format!("relation source entry `{entry_ref}` is not present"))?;
    let entry_object = entry
        .as_object()
        .ok_or("memory.entries[] must contain objects")?;
    let entry_kind = required_string(entry_object, "kind")?.to_string();
    let text = required_string(entry_object, "text")?.to_string();
    let coordinates = entry
        .get("coordinates")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let process_scope = coordinate_scope(coordinates, "benchmark_record")
        .or_else(|| coordinate_scope(coordinates, "conversation"))
        .unwrap_or_else(|| item.about.to_string());
    let task_scope = coordinate_scope(coordinates, "question");
    let observed_at = coordinates
        .iter()
        .find_map(|coordinate| {
            coordinate
                .get("occurred_at")
                .or_else(|| coordinate.get("observed_at"))
                .and_then(Value::as_str)
        })
        .unwrap_or("2026-01-01T00:00:00Z")
        .to_string();
    let sequence = relation
        .get("sequence")
        .and_then(Value::as_u64)
        .or_else(|| {
            coordinates
                .iter()
                .find_map(|coordinate| coordinate.get("sequence").and_then(Value::as_u64))
        })
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| u32::try_from(item.item_index + 1).unwrap_or(1).max(1));

    Ok(Some(RelationWriteInput {
        question_id: item.question_id.to_string(),
        question_type: item.question_type.to_string(),
        about: item.about.to_string(),
        entry_ref,
        target_ref,
        entry_kind,
        text,
        target_text: None,
        observed_at,
        process_scope,
        task_scope,
        sequence,
        original_rel,
        original_class: relation
            .get("class")
            .and_then(Value::as_str)
            .unwrap_or("evidential")
            .to_string(),
    }))
}

fn entries_by_ref(
    entries: &[Value],
) -> Result<BTreeMap<String, Value>, Box<dyn Error + Send + Sync>> {
    let mut by_ref = BTreeMap::new();
    for entry in entries {
        let object = entry
            .as_object()
            .ok_or("memory.entries[] must contain objects")?;
        by_ref.insert(required_string(object, "id")?.to_string(), entry.clone());
    }
    Ok(by_ref)
}

fn write_memory_request(
    input: &RelationWriteInput,
    proposal: &RelationProposal,
    read_context: &ReadContextPlan,
) -> Value {
    let mut scope = Map::new();
    if let Some(task_scope) = input.task_scope.as_deref() {
        scope.insert("task".to_string(), json!(task_scope));
    }
    scope.insert("process".to_string(), json!(input.process_scope));

    json!({
        "about": input.about,
        "intent": "record_turn",
        "actor": DEFAULT_WRITER_ACTOR,
        "observed_at": input.observed_at,
        "source_kind": DEFAULT_SOURCE_KIND,
        "scope": Value::Object(scope),
        "current": {
            "ref": input.entry_ref,
            "kind": "turn",
            "summary": compact_memory_text(&input.text, MAX_CURRENT_SUMMARY_CHARS),
            "evidence": compact_memory_text(&input.text, MAX_CURRENT_EVIDENCE_CHARS)
        },
        "connect_to": proposal.connect_to,
        "read_context": read_context_json(read_context),
        "idempotency_key": format!("longmemeval-smart-writer:{}:{}", input.entry_ref, input.target_ref),
        "options": {
            "dry_run": true,
            "strict": true,
            "sequence": input.sequence
        }
    })
}

fn read_context_json(read_context: &ReadContextPlan) -> Value {
    json!({
        "inspected_refs": read_context.inspected_refs.iter().cloned().collect::<Vec<_>>(),
        "temporal_refs": read_context.temporal_refs.iter().cloned().collect::<Vec<_>>()
    })
}

fn fallback_connect_to(input: &RelationWriteInput) -> Vec<Value> {
    vec![json!({
        "ref": input.target_ref,
        "rel": "supports",
        "class": "evidential",
        "why": "This LongMemEval turn provides direct evidence relevant to the benchmark question.",
        "evidence": compact_memory_text(&input.text, MAX_RELATION_EVIDENCE_CHARS),
        "confidence": "high"
    })]
}

fn writer_prompt(input: &RelationWriteInput, read_context: &ReadContextPlan) -> String {
    let observed_refs = read_context
        .inspected_refs
        .iter()
        .chain(read_context.temporal_refs.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join("\n- ");
    let allowed_rich_relations = writer_relation_names_for_quality(MemoryRelationQuality::Rich);
    let class_requirements = writer_relation_class_requirements();
    let target_text = input
        .target_text
        .as_deref()
        .map(|text| compact_memory_text(text, MAX_PROMPT_TARGET_TEXT_CHARS))
        .unwrap_or_default();

    format!(
        "You are an KMP writer for LongMemEval conversational memory.\n\
         Choose exactly one auditable connect_to relation from the current turn to the benchmark question target.\n\
         Return only JSON: {{\"connect_to\":[{{\"ref\":\"...\",\"rel\":\"...\",\"class\":\"...\",\"why\":\"...\",\"evidence\":\"...\",\"confidence\":\"high|medium|low|unknown\"}}]}}.\n\
         Use only this target ref: {}. Never invent refs.\n\
         Allowed rich relations: {allowed_rich_relations}.\n\
         Required relation classes: {class_requirements}.\n\
         Prefer matches_requirement when the turn satisfies the question predicate, component_of when it is one item/value in an aggregate answer, same_entity_as/restates/corrects for duplicate or updated facts, and supports only when no more specific relation is justified.\n\
         Do not invent aggregate totals. Use a rich relation only when the current turn text and MCP-read context justify it.\n\n\
         Question id: {}\nQuestion type: {}\nTarget question ref: {}\nTarget question text: {}\n\n\
         Current turn ref: {}\nOriginal relation: {} / {}\nCurrent turn text: {}\n\n\
         Refs observed before writing:\n- {}\n",
        input.target_ref,
        input.question_id,
        input.question_type,
        input.target_ref,
        target_text,
        input.entry_ref,
        input.original_rel,
        input.original_class,
        input.text,
        observed_refs
    )
}

fn parse_llm_connect_to(
    raw: &str,
    input: &RelationWriteInput,
) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
    let normalized = normalize_llm_json_response(raw);
    let proposal = serde_json::from_str::<LlmWriterProposal>(&normalized)?;
    if proposal.connect_to.len() != 1 {
        return Err("LLM writer must return exactly one connect_to relation".into());
    }

    let relation = proposal
        .connect_to
        .into_iter()
        .next()
        .ok_or("LLM writer returned no relation")?;
    if relation.r#ref != input.target_ref {
        return Err(format!(
            "LLM writer used non-target ref `{}` instead of `{}`",
            relation.r#ref, input.target_ref
        )
        .into());
    }
    let relation_type = validate_writer_relation_shape(&relation)?;
    let why = compact_memory_text(&relation.why, MAX_RELATION_WHY_CHARS);
    let evidence = compact_memory_text(&relation.evidence, MAX_RELATION_EVIDENCE_CHARS);
    Ok(vec![json!({
        "ref": relation.r#ref,
        "rel": relation_type.as_str(),
        "class": relation.class,
        "why": why,
        "evidence": evidence,
        "confidence": relation.confidence.unwrap_or_else(|| "medium".to_string())
    })])
}

fn validate_writer_relation_shape(relation: &LlmConnectTo) -> Result<MemoryRelationType, String> {
    let relation_type = MemoryRelationType::new(&relation.rel)
        .map_err(|error| format!("unsupported writer relation `{}`: {error}", relation.rel))?;
    let spec = relation_type
        .writer_spec()
        .ok_or_else(|| format!("unsupported writer relation `{}`", relation.rel))?;
    let semantic_class = RelationSemanticClass::parse(&relation.class)
        .map_err(|_| format!("unsupported writer class `{}`", relation.class))?;
    if !spec.allows_class(&semantic_class) {
        return Err(format!(
            "writer relation `{}` cannot use class `{}`",
            relation.rel, relation.class
        ));
    }
    if relation.why.trim().is_empty() || relation.evidence.trim().is_empty() {
        return Err("writer relation requires why and evidence".to_string());
    }
    Ok(relation_type)
}

fn fallback_after_rejected_llm_plan(
    input: &RelationWriteInput,
    rejected: RelationProposal,
    reason: &str,
) -> RelationProposal {
    RelationProposal {
        connect_to: fallback_connect_to(input),
        strategy: format!("deterministic_fallback_after_rejected_llm_plan:{reason}"),
        llm_raw: rejected.llm_raw,
        llm_prompt_chars: rejected.llm_prompt_chars,
        llm_prompt_tokens: rejected.llm_prompt_tokens,
        llm_completion_tokens: rejected.llm_completion_tokens,
        llm_used: rejected.llm_used,
        llm_output_valid: false,
    }
}

async fn call_writer_tool_with_record(
    server: &KernelMcpServer,
    request_id: &mut u64,
    tool: &str,
    arguments: &Value,
) -> Result<LongMemEvalSmartWriterToolCall, Box<dyn Error + Send + Sync>> {
    let id = *request_id;
    *request_id = request_id.checked_add(1).ok_or("request id overflow")?;
    let started = Instant::now();
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": tool,
            "arguments": arguments
        }
    });
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .ok_or_else(|| format!("MCP tool `{tool}` returned no JSON-RPC response"))?;
    let value = serde_json::from_str::<Value>(&response)?;
    if let Some(error) = value.get("error") {
        return Err(format!("MCP tool `{tool}` returned JSON-RPC error: {error}").into());
    }
    let result = value
        .get("result")
        .ok_or_else(|| format!("MCP tool `{tool}` returned no result"))?;
    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!("MCP tool `{tool}` failed: {result}").into());
    }
    let content = result
        .get("structuredContent")
        .ok_or_else(|| format!("MCP tool `{tool}` returned no structuredContent"))?
        .clone();
    let observed_refs = collect_memory_refs(&content).into_iter().collect();

    Ok(LongMemEvalSmartWriterToolCall {
        tool: tool.to_string(),
        arguments: arguments.clone(),
        elapsed_ms: started.elapsed().as_millis(),
        content,
        observed_refs,
    })
}

fn set_dry_run(request: &mut Value, dry_run: bool) -> Result<(), Box<dyn Error + Send + Sync>> {
    let options = request
        .get_mut("options")
        .and_then(Value::as_object_mut)
        .ok_or("kernel_write_memory request requires options")?;
    options.insert("dry_run".to_string(), json!(dry_run));
    Ok(())
}

fn memory_array<'a>(
    arguments: &'a Value,
    field: &str,
) -> Result<&'a [Value], Box<dyn Error + Send + Sync>> {
    Ok(arguments
        .get("memory")
        .and_then(|memory| memory.get(field))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]))
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, Box<dyn Error + Send + Sync>> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing required string `{key}`").into())
}

fn coordinate_scope(coordinates: &[Value], dimension: &str) -> Option<String> {
    coordinates.iter().find_map(|coordinate| {
        let object = coordinate.as_object()?;
        if object.get("dimension").and_then(Value::as_str) == Some(dimension) {
            object
                .get("scope_id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(ToString::to_string)
        } else {
            None
        }
    })
}

fn attach_target_text(input: &mut RelationWriteInput, read_context: &ReadContextPlan) {
    let texts = target_texts_from_read_context(read_context);
    input.target_text = texts.get(&input.target_ref).cloned();
}

fn target_texts_from_read_context(read_context: &ReadContextPlan) -> BTreeMap<String, String> {
    let mut texts = BTreeMap::new();
    for call in &read_context.calls {
        if let Some(object) = call.content.get("object").and_then(Value::as_object)
            && let (Some(ref_id), Some(text)) = (
                object.get("ref").and_then(Value::as_str),
                object.get("text").and_then(Value::as_str),
            )
            && !text.trim().is_empty()
        {
            texts.insert(ref_id.to_string(), text.to_string());
        }
        for entry in call
            .content
            .get("entries")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            if let (Some(ref_id), Some(text)) = (
                entry.get("ref").and_then(Value::as_str),
                entry.get("text").and_then(Value::as_str),
            ) && !text.trim().is_empty()
            {
                texts
                    .entry(ref_id.to_string())
                    .or_insert_with(|| text.to_string());
            }
        }
    }
    texts
}

fn collect_memory_refs(value: &Value) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    collect_memory_refs_from_field(value, None, &mut refs);
    refs
}

fn collect_memory_refs_from_field(value: &Value, field: Option<&str>, refs: &mut BTreeSet<String>) {
    match value {
        Value::String(value) if field_allows_memory_ref(field) && looks_like_memory_ref(value) => {
            refs.insert(value.to_string());
        }
        Value::Array(values) => {
            for value in values {
                collect_memory_refs_from_field(value, field, refs);
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                collect_memory_refs_from_field(value, Some(key), refs);
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

fn looks_like_memory_ref(value: &str) -> bool {
    !value.contains(' ')
        && value.len() <= 500
        && (value.starts_with("longmemeval:")
            || value.starts_with("turn:")
            || value.starts_with("question:")
            || value.starts_with("evidence:")
            || value.starts_with("about:"))
}

fn is_longmemeval_entry_ref(value: &str) -> bool {
    value.starts_with("turn:") || value.starts_with("question:")
}

fn compact_memory_text(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    format!(
        "{}...",
        trimmed
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect::<String>()
            .trim_end()
    )
}

fn metric_usize(metrics: &Value, key: &str) -> usize {
    metrics
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_default()
}

fn writer_relation_names_for_quality(quality: MemoryRelationQuality) -> String {
    KnownMemoryRelationType::writer_relation_types()
        .iter()
        .filter_map(|relation_type| relation_type.writer_spec())
        .filter(|spec| spec.quality() == quality)
        .map(|spec| spec.relation_type().as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn writer_relation_class_requirements() -> String {
    KnownMemoryRelationType::writer_relation_types()
        .iter()
        .filter_map(|relation_type| relation_type.writer_spec())
        .filter(|spec| spec.quality() != MemoryRelationQuality::Structural)
        .map(|spec| {
            let classes = spec
                .allowed_classes()
                .iter()
                .map(RelationSemanticClass::as_str)
                .collect::<Vec<_>>()
                .join(" or ");
            format!("{}={classes}", spec.relation_type().as_str())
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn log_writer_navigation(enabled: bool, payload: Value) {
    if !enabled {
        return;
    }
    match serde_json::to_string(&payload) {
        Ok(line) => eprintln!("{line}"),
        Err(error) => eprintln!(
            "{{\"event\":\"longmemeval_smart_writer.log_error\",\"error\":{}}}",
            json!(error.to_string())
        ),
    }
}

pub fn detect_provider_from_model(model: &str) -> LlmProvider {
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
