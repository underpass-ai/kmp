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

const DEFAULT_WRITER_ACTOR: &str = "agent:memoryarena-smart-writer";
const DEFAULT_SOURCE_KIND: &str = "agent";
const MAX_CURRENT_SUMMARY_CHARS: usize = 1600;
const MAX_CURRENT_EVIDENCE_CHARS: usize = 1600;
const MAX_RELATION_EVIDENCE_CHARS: usize = 900;
const MAX_RELATION_WHY_CHARS: usize = 700;
const MAX_PROMPT_TARGET_TEXT_CHARS: usize = 1200;

#[derive(Debug, Clone)]
pub struct MemoryArenaSmartWriterConfig {
    pub llm_endpoint: Option<String>,
    pub llm_model: Option<String>,
    pub llm_provider: Option<LlmProvider>,
    pub api_key: Option<String>,
    pub max_tokens: u32,
    pub temperature: f64,
    pub log_mcp_navigation: bool,
}

impl MemoryArenaSmartWriterConfig {
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
pub struct MemoryArenaSmartWriter {
    config: MemoryArenaSmartWriterConfig,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryArenaSmartWriterEvent<'a> {
    pub event_index: usize,
    pub phase: &'a str,
    pub subtask_index: Option<usize>,
    pub about: &'a str,
    pub arguments: &'a Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryArenaSmartWriterResult {
    pub event_index: usize,
    pub phase: String,
    pub subtask_index: Option<usize>,
    pub about: String,
    pub entry_ref: String,
    pub entry_kind: String,
    pub writer_intent: String,
    pub writer_current_kind: String,
    pub relation_strategy: String,
    pub pre_read_calls: Vec<MemoryArenaSmartWriterToolCall>,
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
pub struct MemoryArenaSmartWriterToolCall {
    pub tool: String,
    pub arguments: Value,
    pub elapsed_ms: u128,
    pub content: Value,
    pub observed_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryArenaSmartWriterSummary {
    pub enabled: bool,
    pub entry_writes: usize,
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
struct EntryWriteInput {
    about: String,
    entry_ref: String,
    entry_kind: String,
    text: String,
    observed_at: String,
    task_scope: Option<String>,
    process_scope: String,
    episode_scope: Option<String>,
    sequence: u32,
    candidates: Vec<RelationCandidate>,
}

#[derive(Debug, Clone)]
struct RelationCandidate {
    target_ref: String,
    original_rel: String,
    original_class: String,
    target_text: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ReadContextPlan {
    inspected_refs: BTreeSet<String>,
    temporal_refs: BTreeSet<String>,
    calls: Vec<MemoryArenaSmartWriterToolCall>,
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

impl MemoryArenaSmartWriter {
    pub fn new(config: MemoryArenaSmartWriterConfig) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;
        Ok(Self { config, client })
    }

    pub async fn write_ingest_event(
        &self,
        server: &KernelMcpServer,
        request_id: &mut u64,
        event: MemoryArenaSmartWriterEvent<'_>,
    ) -> Result<Vec<MemoryArenaSmartWriterResult>, Box<dyn Error + Send + Sync>> {
        let mut results = Vec::new();
        let entries = memory_array(event.arguments, "entries")?;
        let relations = memory_array(event.arguments, "relations")?;

        for entry in entries {
            let started = Instant::now();
            let mut input = entry_write_input(event, entry, relations)?;
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.entry.start",
                    "event_index": event.event_index,
                    "phase": event.phase,
                    "subtask_index": event.subtask_index,
                    "about": event.about,
                    "entry_ref": input.entry_ref.as_str(),
                    "entry_kind": input.entry_kind.as_str(),
                    "candidate_refs": input.candidates.iter().map(|candidate| candidate.target_ref.as_str()).collect::<Vec<_>>()
                }),
            );
            let read_context = self
                .read_context_for_candidates(server, request_id, event.about, &input)
                .await?;
            attach_candidate_texts(&mut input, &read_context);
            let mut proposal = self.propose_relations(&input, &read_context).await?;
            let mut request = write_memory_request(event.about, &input, &proposal, &read_context);

            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.write.dry_run.start",
                    "request_id": *request_id,
                    "about": event.about,
                    "entry_ref": input.entry_ref.as_str(),
                    "relation_strategy": proposal.strategy.as_str(),
                    "connect_to": connect_to_summary(&proposal.connect_to)
                }),
            );
            let dry_run_call = match call_writer_tool_with_record(
                server,
                request_id,
                "kmp_write_memory",
                &request,
            )
            .await
            {
                Ok(call) => call,
                Err(error) if proposal.llm_used => {
                    log_writer_navigation(
                        self.config.log_mcp_navigation,
                        json!({
                            "event": "memoryarena_smart_writer.write.dry_run.rejected",
                            "about": event.about,
                            "entry_ref": input.entry_ref.as_str(),
                            "relation_strategy": proposal.strategy.as_str(),
                            "error": error.to_string()
                        }),
                    );
                    proposal =
                        fallback_after_rejected_llm_plan(&input, proposal, &error.to_string());
                    request = write_memory_request(event.about, &input, &proposal, &read_context);
                    log_writer_navigation(
                        self.config.log_mcp_navigation,
                        json!({
                            "event": "memoryarena_smart_writer.write.dry_run.retry_fallback",
                            "request_id": *request_id,
                            "about": event.about,
                            "entry_ref": input.entry_ref.as_str(),
                            "relation_strategy": proposal.strategy.as_str(),
                            "connect_to": connect_to_summary(&proposal.connect_to)
                        }),
                    );
                    call_writer_tool_with_record(server, request_id, "kmp_write_memory", &request)
                        .await?
                }
                Err(error) => return Err(error),
            };
            let dry_run_content = dry_run_call.content.clone();
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.write.dry_run.done",
                    "about": event.about,
                    "entry_ref": input.entry_ref.as_str(),
                    "elapsed_ms": dry_run_call.elapsed_ms,
                    "relation_quality_metrics": dry_run_content.get("relation_quality_metrics").cloned().unwrap_or_else(|| json!({}))
                }),
            );

            set_dry_run(&mut request, false)?;
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.write.commit.start",
                    "request_id": *request_id,
                    "about": event.about,
                    "entry_ref": input.entry_ref.as_str(),
                    "relation_strategy": proposal.strategy.as_str(),
                    "connect_to": connect_to_summary(&proposal.connect_to)
                }),
            );
            let commit_call =
                call_writer_tool_with_record(server, request_id, "kmp_write_memory", &request)
                    .await?;
            let commit_content = commit_call.content.clone();
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.write.commit.done",
                    "about": event.about,
                    "entry_ref": input.entry_ref.as_str(),
                    "elapsed_ms": commit_call.elapsed_ms,
                    "read_after_write_ready": commit_content
                        .pointer("/ingest_result/memory/read_after_write_ready")
                        .cloned()
                        .unwrap_or(Value::Null)
                }),
            );
            let verify_arguments = json!({
                "about": event.about,
                "ref": input.entry_ref,
                "include": {
                    "incoming": true,
                    "outgoing": true,
                    "details": true,
                    "raw": false
                }
            });
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.write.verify.start",
                    "request_id": *request_id,
                    "about": event.about,
                    "entry_ref": input.entry_ref.as_str(),
                    "tool": "kmp_inspect"
                }),
            );
            let verify_call =
                call_writer_tool_with_record(server, request_id, "kmp_inspect", &verify_arguments)
                    .await?;
            let verify_content = verify_call.content.clone();
            let verify_observed_refs = verify_call.observed_refs.clone();
            let verify_observed_entry_refs = observed_entry_refs(&verify_observed_refs);
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.write.verify.done",
                    "about": event.about,
                    "entry_ref": input.entry_ref.as_str(),
                    "elapsed_ms": verify_call.elapsed_ms,
                    "observed_refs": verify_observed_refs,
                    "observed_entry_refs": verify_observed_entry_refs
                }),
            );

            results.push(MemoryArenaSmartWriterResult {
                event_index: event.event_index,
                phase: event.phase.to_string(),
                subtask_index: event.subtask_index,
                about: event.about.to_string(),
                entry_ref: input.entry_ref.clone(),
                entry_kind: input.entry_kind.clone(),
                writer_intent: writer_intent(&input.entry_kind).to_string(),
                writer_current_kind: writer_current_kind(&input.entry_kind).to_string(),
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

    async fn read_context_for_candidates(
        &self,
        server: &KernelMcpServer,
        request_id: &mut u64,
        about: &str,
        input: &EntryWriteInput,
    ) -> Result<ReadContextPlan, Box<dyn Error + Send + Sync>> {
        let mut context = ReadContextPlan::default();
        let mut targets = BTreeSet::new();
        for candidate in &input.candidates {
            targets.insert(candidate.target_ref.clone());
        }

        for target_ref in targets {
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.mcp_read.start",
                    "request_id": *request_id,
                    "about": about,
                    "entry_ref": input.entry_ref.as_str(),
                    "target_ref": target_ref.as_str(),
                    "tool": "kmp_near",
                    "window": {"before_entries": 3, "after_entries": 0}
                }),
            );
            let near = call_writer_tool_with_record(
                server,
                request_id,
                "kmp_near",
                &json!({
                    "about": about,
                    "around": {"ref": target_ref.as_str()},
                    "window": {"before_entries": 3, "after_entries": 0},
                    "limit": {"entries": 8, "tokens": 1800},
                    "dimensions": {"scope": "current_about", "mode": "all"},
                    "include": {"evidence": true, "relations": true, "raw_refs": false},
                    "budget": {"tokens": 1800, "depth": 2}
                }),
            )
            .await?;
            for ref_id in collect_memoryarena_refs(&near.content) {
                if is_memoryarena_entry_ref(&ref_id) {
                    context.temporal_refs.insert(ref_id);
                }
            }
            let near_observed_refs = near.observed_refs.clone();
            let near_observed_entry_refs = observed_entry_refs(&near_observed_refs);
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.mcp_read.done",
                    "about": about,
                    "entry_ref": input.entry_ref.as_str(),
                    "target_ref": target_ref.as_str(),
                    "tool": near.tool.as_str(),
                    "elapsed_ms": near.elapsed_ms,
                    "observed_refs": near_observed_refs,
                    "observed_entry_refs": near_observed_entry_refs
                }),
            );
            context.calls.push(near);

            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.mcp_read.start",
                    "request_id": *request_id,
                    "about": about,
                    "entry_ref": input.entry_ref.as_str(),
                    "target_ref": target_ref.as_str(),
                    "tool": "kmp_inspect"
                }),
            );
            let inspect = call_writer_tool_with_record(
                server,
                request_id,
                "kmp_inspect",
                &json!({
                    "about": about,
                    "ref": target_ref.as_str(),
                    "include": {
                        "incoming": true,
                        "outgoing": true,
                        "details": true,
                        "raw": false
                    }
                }),
            )
            .await?;
            let inspected_ref = target_ref.clone();
            context.inspected_refs.insert(target_ref);
            let inspect_observed_refs = inspect.observed_refs.clone();
            let inspect_observed_entry_refs = observed_entry_refs(&inspect_observed_refs);
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.mcp_read.done",
                    "about": about,
                    "entry_ref": input.entry_ref.as_str(),
                    "target_ref": inspected_ref.as_str(),
                    "tool": inspect.tool.as_str(),
                    "elapsed_ms": inspect.elapsed_ms,
                    "observed_refs": inspect_observed_refs,
                    "observed_entry_refs": inspect_observed_entry_refs
                }),
            );
            context.calls.push(inspect);
        }

        Ok(context)
    }

    async fn propose_relations(
        &self,
        input: &EntryWriteInput,
        read_context: &ReadContextPlan,
    ) -> Result<RelationProposal, Box<dyn Error + Send + Sync>> {
        if self.config.llm_enabled() && !input.candidates.is_empty() {
            let prompt = writer_prompt(input, read_context);
            let endpoint = self.config.llm_endpoint.as_deref().unwrap_or_default();
            let model = self.config.llm_model.as_deref().unwrap_or_default();
            let provider = self
                .config
                .llm_provider
                .unwrap_or_else(|| detect_provider_from_model(model));
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.llm.start",
                    "entry_ref": input.entry_ref.as_str(),
                    "entry_kind": input.entry_kind.as_str(),
                    "model": model,
                    "provider": format!("{provider:?}"),
                    "candidate_refs": input.candidates.iter().map(|candidate| candidate.target_ref.as_str()).collect::<Vec<_>>(),
                    "mcp_pre_read_calls": read_context.calls.len(),
                    "inspected_refs": read_context.inspected_refs.iter().map(String::as_str).collect::<Vec<_>>(),
                    "temporal_refs": read_context.temporal_refs.iter().map(String::as_str).collect::<Vec<_>>(),
                    "prompt_chars": prompt.len()
                }),
            );
            let (raw, prompt_tokens, completion_tokens) = match call_llm(
                &self.client,
                endpoint,
                model,
                provider,
                self.config.api_key.as_deref(),
                &prompt,
                self.config.max_tokens,
                self.config.temperature,
            )
            .await
            {
                Ok(output) => output,
                Err(error) => {
                    log_writer_navigation(
                        self.config.log_mcp_navigation,
                        json!({
                            "event": "memoryarena_smart_writer.llm.error",
                            "entry_ref": input.entry_ref.as_str(),
                            "entry_kind": input.entry_kind.as_str(),
                            "model": model,
                            "provider": format!("{provider:?}"),
                            "candidate_refs": input.candidates.iter().map(|candidate| candidate.target_ref.as_str()).collect::<Vec<_>>(),
                            "mcp_pre_read_calls": read_context.calls.len(),
                            "error": error.to_string()
                        }),
                    );
                    return Err(format!(
                        "LLM relation proposal failed for {}: {error}",
                        input.entry_ref
                    )
                    .into());
                }
            };
            if let Ok(connect_to) = parse_llm_connect_to(&raw, input) {
                log_writer_navigation(
                    self.config.log_mcp_navigation,
                    json!({
                        "event": "memoryarena_smart_writer.llm.done",
                        "entry_ref": input.entry_ref.as_str(),
                        "strategy": "llm",
                        "llm_output_valid": true,
                        "prompt_tokens": prompt_tokens,
                        "completion_tokens": completion_tokens,
                        "connect_to": connect_to_summary(&connect_to)
                    }),
                );
                return Ok(RelationProposal {
                    connect_to,
                    strategy: "llm".to_string(),
                    llm_raw: Some(raw),
                    llm_prompt_chars: prompt.len(),
                    llm_prompt_tokens: prompt_tokens,
                    llm_completion_tokens: completion_tokens,
                    llm_used: true,
                    llm_output_valid: true,
                });
            }
            let fallback_connect_to = fallback_connect_to(input);
            log_writer_navigation(
                self.config.log_mcp_navigation,
                json!({
                    "event": "memoryarena_smart_writer.llm.done",
                    "entry_ref": input.entry_ref.as_str(),
                    "strategy": "deterministic_fallback_after_invalid_llm",
                    "llm_output_valid": false,
                    "prompt_tokens": prompt_tokens,
                    "completion_tokens": completion_tokens,
                    "connect_to": connect_to_summary(&fallback_connect_to)
                }),
            );
            return Ok(RelationProposal {
                connect_to: fallback_connect_to,
                strategy: "deterministic_fallback_after_invalid_llm".to_string(),
                llm_raw: Some(raw),
                llm_prompt_chars: prompt.len(),
                llm_prompt_tokens: prompt_tokens,
                llm_completion_tokens: completion_tokens,
                llm_used: true,
                llm_output_valid: false,
            });
        }

        let fallback_connect_to = fallback_connect_to(input);
        log_writer_navigation(
            self.config.log_mcp_navigation,
            json!({
                "event": "memoryarena_smart_writer.llm.skipped",
                "entry_ref": input.entry_ref.as_str(),
                "strategy": "deterministic_fallback",
                "reason": if input.candidates.is_empty() { "no_candidates" } else { "llm_disabled" },
                "connect_to": connect_to_summary(&fallback_connect_to)
            }),
        );
        Ok(RelationProposal {
            connect_to: fallback_connect_to,
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

pub fn summarize_smart_writer(
    enabled: bool,
    results: &[MemoryArenaSmartWriterResult],
) -> MemoryArenaSmartWriterSummary {
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

    MemoryArenaSmartWriterSummary {
        enabled,
        entry_writes: results.len(),
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

fn entry_write_input(
    event: MemoryArenaSmartWriterEvent<'_>,
    entry: &Value,
    relations: &[Value],
) -> Result<EntryWriteInput, Box<dyn Error + Send + Sync>> {
    let entry_object = entry
        .as_object()
        .ok_or("memory.entries[] must contain objects")?;
    let entry_ref = required_string(entry_object, "id")?.to_string();
    let entry_kind = required_string(entry_object, "kind")?.to_string();
    let text = required_string(entry_object, "text")?.to_string();
    let coordinates = entry
        .get("coordinates")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or("memory entry requires coordinates")?;
    let process_scope = coordinate_scope(coordinates, "agentic_process")
        .ok_or("memory entry requires an agentic_process coordinate")?;
    let task_scope = coordinate_scope(coordinates, "benchmark_task")
        .or_else(|| coordinate_scope(coordinates, "task"));
    let episode_scope = coordinate_scope(coordinates, "agentic_episode");
    let observed_at = coordinates
        .iter()
        .find_map(|coordinate| coordinate.get("observed_at").and_then(Value::as_str))
        .unwrap_or("2026-01-01T00:00:00Z")
        .to_string();
    let sequence = coordinates
        .iter()
        .find_map(|coordinate| coordinate.get("sequence").and_then(Value::as_u64))
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| u32::try_from(event.event_index).unwrap_or(1).max(1));

    Ok(EntryWriteInput {
        about: event.about.to_string(),
        entry_ref: entry_ref.clone(),
        entry_kind,
        text,
        observed_at,
        task_scope,
        process_scope,
        episode_scope,
        sequence,
        candidates: relation_candidates(&entry_ref, relations),
    })
}

fn relation_candidates(entry_ref: &str, relations: &[Value]) -> Vec<RelationCandidate> {
    relations
        .iter()
        .filter_map(Value::as_object)
        .filter(|relation| relation.get("from").and_then(Value::as_str) == Some(entry_ref))
        .filter_map(|relation| {
            Some(RelationCandidate {
                target_ref: relation.get("to")?.as_str()?.to_string(),
                original_rel: relation.get("rel")?.as_str()?.to_string(),
                original_class: relation.get("class")?.as_str()?.to_string(),
                target_text: None,
            })
        })
        .collect()
}

fn write_memory_request(
    about: &str,
    input: &EntryWriteInput,
    proposal: &RelationProposal,
    read_context: &ReadContextPlan,
) -> Value {
    let current_summary = compact_memory_text(&input.text, MAX_CURRENT_SUMMARY_CHARS);
    let current_evidence = compact_memory_text(&input.text, MAX_CURRENT_EVIDENCE_CHARS);
    let mut scope = Map::new();
    if let Some(task_scope) = input.task_scope.as_deref() {
        scope.insert("task".to_string(), json!(task_scope));
    }
    scope.insert("process".to_string(), json!(input.process_scope));
    if let Some(episode_scope) = input.episode_scope.as_deref() {
        scope.insert("episode".to_string(), json!(episode_scope));
    }

    json!({
        "about": about,
        "intent": writer_intent(&input.entry_kind),
        "actor": DEFAULT_WRITER_ACTOR,
        "observed_at": input.observed_at,
        "source_kind": DEFAULT_SOURCE_KIND,
        "scope": Value::Object(scope),
        "current": {
            "ref": input.entry_ref,
            "kind": writer_current_kind(&input.entry_kind),
            "summary": current_summary,
            "evidence": current_evidence
        },
        "connect_to": proposal.connect_to,
        "read_context": read_context_json(read_context),
        "idempotency_key": format!("smart-writer:{}", input.entry_ref),
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

fn fallback_connect_to(input: &EntryWriteInput) -> Vec<Value> {
    if let Some(candidate) = preferred_candidate(input, "answers") {
        return vec![json!({
            "ref": candidate.target_ref,
            "rel": "answers",
            "class": "evidential",
            "why": "This feedback answers the prior question observed in the process.",
            "evidence": compact_memory_text(&input.text, MAX_RELATION_EVIDENCE_CHARS),
            "confidence": "high"
        })];
    }

    if let Some(candidate) = preferred_candidate(input, "follows") {
        return vec![json!({
            "ref": candidate.target_ref,
            "rel": "follows",
            "class": "procedural",
            "why": "The new memory follows this prior process memory in sequence; no richer dependency was justified.",
            "evidence": compact_memory_text(&input.text, MAX_RELATION_EVIDENCE_CHARS),
            "confidence": "high"
        })];
    }

    if let Some(candidate) = preferred_candidate(input, "uses_background") {
        return vec![json!({
            "ref": candidate.target_ref,
            "rel": "uses_background",
            "class": "evidential",
            "why": "The new memory is scoped to background available before this step.",
            "evidence": compact_memory_text(&input.text, MAX_RELATION_EVIDENCE_CHARS),
            "confidence": "high"
        })];
    }

    vec![json!({
        "ref": namespaced_dimension_ref(&input.about, &input.process_scope),
        "rel": "scoped_to",
        "class": "structural",
        "why": "This first memory entry is scoped to the current agentic process before semantic targets exist.",
        "evidence": compact_memory_text(&input.text, MAX_RELATION_EVIDENCE_CHARS),
        "confidence": "high"
    })]
}

fn namespaced_dimension_ref(about: &str, scope_id: &str) -> String {
    format!("about:{about}:dimension:{scope_id}")
}

fn preferred_candidate<'a>(
    input: &'a EntryWriteInput,
    original_rel: &str,
) -> Option<&'a RelationCandidate> {
    input
        .candidates
        .iter()
        .find(|candidate| candidate.original_rel == original_rel)
}

fn writer_prompt(input: &EntryWriteInput, read_context: &ReadContextPlan) -> String {
    let candidates = input
        .candidates
        .iter()
        .map(|candidate| {
            format!(
                "- ref: {}\n  original_rel: {}\n  original_class: {}\n  target_text: {}\n",
                candidate.target_ref,
                candidate.original_rel,
                candidate.original_class,
                candidate
                    .target_text
                    .as_deref()
                    .map(|text| compact_memory_text(text, MAX_PROMPT_TARGET_TEXT_CHARS))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("");
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
    let allowed_fallback_relations =
        writer_relation_names_for_quality(MemoryRelationQuality::Anemic);
    let class_requirements = writer_relation_class_requirements();

    format!(
        "You are an KMP writer. Choose auditable connect_to relations for one new memory entry.\n\
         Return only JSON with this shape: {{\"connect_to\":[{{\"ref\":\"...\",\"rel\":\"...\",\"class\":\"...\",\"why\":\"...\",\"evidence\":\"...\",\"confidence\":\"high|medium|low|unknown\"}}]}}.\n\
         Use only refs from the candidate list. Never invent refs.\n\
         Allowed rich relations: {allowed_rich_relations}.\n\
         Allowed fallback relations: {allowed_fallback_relations}.\n\
         Required relation classes: {class_requirements}.\n\
         Use a rich relation only when the read context makes the semantic dependency specific. If unsure, use the candidate's original fallback relation.\n\
         If the current entry explicitly uses a value, clue, decision, or feedback stored in a candidate target, prefer depends_on with class causal over follows.\n\
         Every non-structural relation must include a concrete why and evidence from the current text or observed target.\n\n\
         Current entry:\nref: {}\nkind: {}\ntext: {}\n\n\
         Candidate target refs:\n{}\n\
         Refs observed before writing:\n- {}\n",
        input.entry_ref, input.entry_kind, input.text, candidates, observed_refs
    )
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

fn parse_llm_connect_to(
    raw: &str,
    input: &EntryWriteInput,
) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
    let normalized = normalize_llm_json_response(raw);
    let proposal = serde_json::from_str::<LlmWriterProposal>(&normalized)?;
    let candidate_refs = input
        .candidates
        .iter()
        .map(|candidate| candidate.target_ref.as_str())
        .collect::<BTreeSet<_>>();
    if proposal.connect_to.is_empty() {
        return Err("LLM writer returned no connect_to relations".into());
    }

    let mut connect_to = Vec::new();
    for (index, relation) in proposal.connect_to.into_iter().enumerate() {
        if !candidate_refs.contains(relation.r#ref.as_str()) {
            return Err(format!(
                "LLM writer relation {index} used non-candidate ref `{}`",
                relation.r#ref
            )
            .into());
        }
        let relation_type = validate_writer_relation_shape(&relation)?;
        let why = compact_memory_text(&relation.why, MAX_RELATION_WHY_CHARS);
        let evidence = compact_memory_text(&relation.evidence, MAX_RELATION_EVIDENCE_CHARS);
        connect_to.push(json!({
            "ref": relation.r#ref,
            "rel": relation_type.as_str(),
            "class": relation.class,
            "why": why,
            "evidence": evidence,
            "confidence": relation.confidence.unwrap_or_else(|| "medium".to_string())
        }));
    }

    Ok(connect_to)
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
    input: &EntryWriteInput,
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
) -> Result<MemoryArenaSmartWriterToolCall, Box<dyn Error + Send + Sync>> {
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
    let observed_refs = collect_memoryarena_refs(&content).into_iter().collect();

    Ok(MemoryArenaSmartWriterToolCall {
        tool: tool.to_string(),
        arguments: arguments.clone(),
        elapsed_ms: started.elapsed().as_millis(),
        content,
        observed_refs,
    })
}

fn log_writer_navigation(enabled: bool, payload: Value) {
    if !enabled {
        return;
    }
    match serde_json::to_string(&payload) {
        Ok(line) => eprintln!("{line}"),
        Err(error) => eprintln!(
            "{{\"event\":\"memoryarena_smart_writer.log_error\",\"error\":{}}}",
            json!(error.to_string())
        ),
    }
}

fn connect_to_summary(connect_to: &[Value]) -> Vec<Value> {
    connect_to
        .iter()
        .filter_map(Value::as_object)
        .map(|relation| {
            json!({
                "ref": relation.get("ref").and_then(Value::as_str).unwrap_or_default(),
                "rel": relation.get("rel").and_then(Value::as_str).unwrap_or_default(),
                "class": relation.get("class").and_then(Value::as_str).unwrap_or_default(),
                "confidence": relation.get("confidence").and_then(Value::as_str).unwrap_or_default()
            })
        })
        .collect()
}

fn observed_entry_refs(refs: &[String]) -> Vec<&str> {
    refs.iter()
        .filter(|reference| is_memoryarena_entry_ref(reference))
        .map(String::as_str)
        .collect()
}

fn set_dry_run(request: &mut Value, dry_run: bool) -> Result<(), Box<dyn Error + Send + Sync>> {
    let options = request
        .get_mut("options")
        .and_then(Value::as_object_mut)
        .ok_or("kmp_write_memory request requires options")?;
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

fn writer_intent(entry_kind: &str) -> &'static str {
    match entry_kind {
        "subtask_answer_feedback" => "record_feedback",
        "background" | "subtask_background" => "record_observation",
        _ => "record_turn",
    }
}

fn writer_current_kind(entry_kind: &str) -> &'static str {
    match entry_kind {
        "subtask_answer_feedback" => "feedback",
        "background" | "subtask_background" => "observation",
        _ => "turn",
    }
}

fn metric_usize(metrics: &Value, key: &str) -> usize {
    metrics
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_default()
}

fn compact_memory_text(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let trimmed = value.trim();
    if char_count(trimmed) <= max_chars {
        return trimmed.to_string();
    }

    if let Some(exact_answer) = exact_answer_line(trimmed) {
        let separator = "\n...\n";
        let exact_answer = if char_count(exact_answer) > max_chars / 2 {
            truncate_chars(exact_answer, max_chars / 2)
        } else {
            exact_answer.to_string()
        };
        let reserved = char_count(separator) + char_count(&exact_answer);
        if max_chars > reserved + 24 {
            let prefix_budget = max_chars - reserved;
            return format!(
                "{}{}{}",
                truncate_chars(trimmed, prefix_budget).trim_end(),
                separator,
                exact_answer
            );
        }
    }

    format!(
        "{}...",
        truncate_chars(trimmed, max_chars.saturating_sub(3)).trim_end()
    )
}

fn exact_answer_line(value: &str) -> Option<&str> {
    value
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with("Exact Answer:"))
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn char_count(value: &str) -> usize {
    value.chars().count()
}

fn attach_candidate_texts(input: &mut EntryWriteInput, read_context: &ReadContextPlan) {
    let texts = target_texts_from_read_context(read_context);
    for candidate in &mut input.candidates {
        if candidate.target_text.is_none() {
            candidate.target_text = texts.get(&candidate.target_ref).cloned();
        }
    }
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

fn collect_memoryarena_refs(value: &Value) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    collect_memoryarena_refs_from_field(value, None, &mut refs);
    refs
}

fn collect_memoryarena_refs_from_field(
    value: &Value,
    field: Option<&str>,
    refs: &mut BTreeSet<String>,
) {
    match value {
        Value::String(value)
            if field_allows_memory_ref(field) && looks_like_memoryarena_ref(value) =>
        {
            refs.insert(value.to_string());
        }
        Value::Array(values) => {
            for value in values {
                collect_memoryarena_refs_from_field(value, field, refs);
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                collect_memoryarena_refs_from_field(value, Some(key), refs);
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

fn looks_like_memoryarena_ref(value: &str) -> bool {
    value.starts_with("memoryarena:") && !value.contains(' ') && value.len() <= 400
}

fn is_memoryarena_entry_ref(value: &str) -> bool {
    value.contains(":subtask:") || value.contains(":background")
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

pub fn parse_provider(value: &str) -> Result<LlmProvider, Box<dyn Error + Send + Sync>> {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn fallback_prefers_answers_over_sequence() {
        let input = EntryWriteInput {
            about: "memoryarena:x".to_string(),
            entry_ref: "memoryarena:x:subtask:1:answer".to_string(),
            entry_kind: "subtask_answer_feedback".to_string(),
            text: "clue-a".to_string(),
            observed_at: "2026-01-01T00:01:00Z".to_string(),
            task_scope: None,
            process_scope: "memoryarena:process:x".to_string(),
            episode_scope: None,
            sequence: 1,
            candidates: vec![
                RelationCandidate {
                    target_ref: "memoryarena:x:subtask:1:question".to_string(),
                    original_rel: "answers".to_string(),
                    original_class: "evidential".to_string(),
                    target_text: None,
                },
                RelationCandidate {
                    target_ref: "memoryarena:x:background:global".to_string(),
                    original_rel: "uses_background".to_string(),
                    original_class: "evidential".to_string(),
                    target_text: None,
                },
            ],
        };

        let connect_to = fallback_connect_to(&input);

        assert_eq!(connect_to[0]["rel"], "answers");
        assert_eq!(connect_to[0]["ref"], "memoryarena:x:subtask:1:question");
    }

    #[test]
    fn first_entry_falls_back_to_structural_process_scope() {
        let input = EntryWriteInput {
            about: "memoryarena:x".to_string(),
            entry_ref: "memoryarena:x:background:global".to_string(),
            entry_kind: "background".to_string(),
            text: "background".to_string(),
            observed_at: "2026-01-01T00:01:00Z".to_string(),
            task_scope: None,
            process_scope: "memoryarena:process:x".to_string(),
            episode_scope: None,
            sequence: 1,
            candidates: Vec::new(),
        };

        let connect_to = fallback_connect_to(&input);

        assert_eq!(connect_to[0]["rel"], "scoped_to");
        assert_eq!(connect_to[0]["class"], "structural");
        assert_eq!(
            connect_to[0]["ref"],
            "about:memoryarena:x:dimension:memoryarena:process:x"
        );
        assert_eq!(connect_to[0]["evidence"], "background");
    }

    #[test]
    fn parses_llm_connect_to_only_for_candidate_refs() {
        let input = EntryWriteInput {
            about: "memoryarena:x".to_string(),
            entry_ref: "memoryarena:x:subtask:2:question".to_string(),
            entry_kind: "subtask_question".to_string(),
            text: "Use clue-a to choose.".to_string(),
            observed_at: "2026-01-01T00:02:00Z".to_string(),
            task_scope: None,
            process_scope: "memoryarena:process:x".to_string(),
            episode_scope: None,
            sequence: 2,
            candidates: vec![RelationCandidate {
                target_ref: "memoryarena:x:subtask:1:answer".to_string(),
                original_rel: "follows".to_string(),
                original_class: "procedural".to_string(),
                target_text: None,
            }],
        };

        let connect_to = parse_llm_connect_to(
            r#"{"connect_to":[{"ref":"memoryarena:x:subtask:1:answer","rel":"depends_on","class":"causal","why":"The current question uses clue-a from the previous answer.","evidence":"Use clue-a to choose."}]}"#,
            &input,
        )
        .expect("candidate relation should parse");

        assert_eq!(connect_to[0]["rel"], "depends_on");
        assert_eq!(connect_to[0]["class"], "causal");
    }

    #[test]
    fn rejects_llm_connect_to_for_unknown_ref() {
        let input = EntryWriteInput {
            about: "memoryarena:x".to_string(),
            entry_ref: "memoryarena:x:subtask:2:question".to_string(),
            entry_kind: "subtask_question".to_string(),
            text: "Use clue-a to choose.".to_string(),
            observed_at: "2026-01-01T00:02:00Z".to_string(),
            task_scope: None,
            process_scope: "memoryarena:process:x".to_string(),
            episode_scope: None,
            sequence: 2,
            candidates: Vec::new(),
        };

        let error = parse_llm_connect_to(
            r#"{"connect_to":[{"ref":"memoryarena:x:missing","rel":"depends_on","class":"causal","why":"x","evidence":"x"}]}"#,
            &input,
        )
        .expect_err("non-candidate ref must fail");

        assert!(error.to_string().contains("non-candidate ref"));
    }

    #[test]
    fn rejects_llm_relation_with_wrong_class() {
        let input = EntryWriteInput {
            about: "memoryarena:x".to_string(),
            entry_ref: "memoryarena:x:subtask:2:question".to_string(),
            entry_kind: "subtask_question".to_string(),
            text: "Use clue-a to choose.".to_string(),
            observed_at: "2026-01-01T00:02:00Z".to_string(),
            task_scope: None,
            process_scope: "memoryarena:process:x".to_string(),
            episode_scope: None,
            sequence: 2,
            candidates: vec![RelationCandidate {
                target_ref: "memoryarena:x:subtask:1:answer".to_string(),
                original_rel: "follows".to_string(),
                original_class: "procedural".to_string(),
                target_text: None,
            }],
        };

        let error = parse_llm_connect_to(
            r#"{"connect_to":[{"ref":"memoryarena:x:subtask:1:answer","rel":"depends_on","class":"procedural","why":"The current question uses the prior clue.","evidence":"Use clue-a to choose."}]}"#,
            &input,
        )
        .expect_err("relation/class mismatch must fail before dry-run");

        assert!(error.to_string().contains("cannot use class"));
    }

    #[test]
    fn rejects_confirms_selection_with_causal_class() {
        let input = EntryWriteInput {
            about: "memoryarena:x".to_string(),
            entry_ref: "memoryarena:x:subtask:2:answer".to_string(),
            entry_kind: "subtask_answer_feedback".to_string(),
            text: "The selected item is confirmed.".to_string(),
            observed_at: "2026-01-01T00:03:00Z".to_string(),
            task_scope: None,
            process_scope: "memoryarena:process:x".to_string(),
            episode_scope: None,
            sequence: 3,
            candidates: vec![RelationCandidate {
                target_ref: "memoryarena:x:subtask:2:question".to_string(),
                original_rel: "answers".to_string(),
                original_class: "evidential".to_string(),
                target_text: None,
            }],
        };

        let error = parse_llm_connect_to(
            r#"{"connect_to":[{"ref":"memoryarena:x:subtask:2:question","rel":"confirms_selection","class":"causal","why":"The feedback confirms the selected item.","evidence":"The selected item is confirmed."}]}"#,
            &input,
        )
        .expect_err("confirms_selection must be evidential or motivational");

        assert!(error.to_string().contains("cannot use class"));
    }

    #[test]
    fn entry_write_input_extracts_scope_and_candidates() {
        let arguments = json!({
            "memory": {
                "entries": [{
                    "id": "memoryarena:x:subtask:2:question",
                    "kind": "subtask_question",
                    "text": "Use clue-a.",
                    "coordinates": [
                        {"dimension": "agentic_process", "scope_id": "memoryarena:process:x", "sequence": 4, "observed_at": "2026-01-01T00:04:00Z"},
                        {"dimension": "benchmark_task", "scope_id": "memoryarena:task:x", "sequence": 4}
                    ]
                }],
                "relations": [{
                    "from": "memoryarena:x:subtask:2:question",
                    "to": "memoryarena:x:subtask:1:answer",
                    "rel": "follows",
                    "class": "procedural"
                }]
            }
        });
        let event = MemoryArenaSmartWriterEvent {
            event_index: 4,
            phase: "pre_subtask",
            subtask_index: Some(2),
            about: "memoryarena:x",
            arguments: &arguments,
        };
        let entries = memory_array(&arguments, "entries").expect("entries");
        let relations = memory_array(&arguments, "relations").expect("relations");

        let input = entry_write_input(event, &entries[0], relations).expect("input");

        assert_eq!(input.entry_ref, "memoryarena:x:subtask:2:question");
        assert_eq!(input.process_scope, "memoryarena:process:x");
        assert_eq!(input.task_scope.as_deref(), Some("memoryarena:task:x"));
        assert_eq!(input.sequence, 4);
        assert_eq!(input.candidates.len(), 1);
    }

    #[test]
    fn attach_candidate_texts_uses_inspect_content() {
        let mut input = EntryWriteInput {
            about: "memoryarena:x".to_string(),
            entry_ref: "memoryarena:x:subtask:2:question".to_string(),
            entry_kind: "subtask_question".to_string(),
            text: "Use clue-a.".to_string(),
            observed_at: "2026-01-01T00:02:00Z".to_string(),
            task_scope: None,
            process_scope: "memoryarena:process:x".to_string(),
            episode_scope: None,
            sequence: 2,
            candidates: vec![RelationCandidate {
                target_ref: "memoryarena:x:subtask:1:answer".to_string(),
                original_rel: "follows".to_string(),
                original_class: "procedural".to_string(),
                target_text: None,
            }],
        };
        let read_context = ReadContextPlan {
            calls: vec![MemoryArenaSmartWriterToolCall {
                tool: "kmp_inspect".to_string(),
                arguments: json!({}),
                elapsed_ms: 1,
                content: json!({
                    "object": {
                        "ref": "memoryarena:x:subtask:1:answer",
                        "kind": "feedback",
                        "text": "clue-a"
                    }
                }),
                observed_refs: Vec::new(),
            }],
            ..ReadContextPlan::default()
        };

        attach_candidate_texts(&mut input, &read_context);

        assert_eq!(input.candidates[0].target_text.as_deref(), Some("clue-a"));
    }

    #[test]
    fn compact_memory_text_preserves_exact_answer_line() {
        let text = format!(
            "{}\n\nExact Answer: Ihuoma Sonia Uche",
            "long evidence paragraph ".repeat(200)
        );

        let compact = compact_memory_text(&text, 180);

        assert!(char_count(&compact) <= 180);
        assert!(compact.contains("Exact Answer: Ihuoma Sonia Uche"));
        assert!(compact.contains("..."));
    }

    #[test]
    fn write_memory_request_compacts_large_current_text() {
        let text = format!(
            "{}\n\nExact Answer: Ihuoma Sonia Uche",
            "large answer feedback ".repeat(300)
        );
        let input = EntryWriteInput {
            about: "memoryarena:x".to_string(),
            entry_ref: "memoryarena:x:subtask:9:answer".to_string(),
            entry_kind: "subtask_answer_feedback".to_string(),
            text,
            observed_at: "2026-01-01T00:27:00Z".to_string(),
            task_scope: Some("memoryarena:task:x".to_string()),
            process_scope: "memoryarena:process:x".to_string(),
            episode_scope: Some("memoryarena:episode:x:9".to_string()),
            sequence: 18,
            candidates: vec![RelationCandidate {
                target_ref: "memoryarena:x:subtask:9:question".to_string(),
                original_rel: "answers".to_string(),
                original_class: "evidential".to_string(),
                target_text: None,
            }],
        };
        let proposal = RelationProposal {
            connect_to: fallback_connect_to(&input),
            strategy: "deterministic_fallback".to_string(),
            llm_raw: None,
            llm_prompt_chars: 0,
            llm_prompt_tokens: 0,
            llm_completion_tokens: 0,
            llm_used: false,
            llm_output_valid: false,
        };

        let request = write_memory_request(
            "memoryarena:x",
            &input,
            &proposal,
            &ReadContextPlan::default(),
        );

        let summary = request["current"]["summary"].as_str().expect("summary");
        let evidence = request["current"]["evidence"].as_str().expect("evidence");
        let relation_evidence = request["connect_to"][0]["evidence"]
            .as_str()
            .expect("relation evidence");
        assert!(char_count(summary) <= MAX_CURRENT_SUMMARY_CHARS);
        assert!(char_count(evidence) <= MAX_CURRENT_EVIDENCE_CHARS);
        assert!(char_count(relation_evidence) <= MAX_RELATION_EVIDENCE_CHARS);
        assert!(summary.contains("Exact Answer: Ihuoma Sonia Uche"));
        assert!(evidence.contains("Exact Answer: Ihuoma Sonia Uche"));
        assert!(relation_evidence.contains("Exact Answer: Ihuoma Sonia Uche"));
    }
}
