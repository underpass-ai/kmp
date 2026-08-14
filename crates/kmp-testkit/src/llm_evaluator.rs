//! LLM-in-the-loop evaluator for paper use cases.
//!
//! Sends rehydrated rendered context to an LLM endpoint (OpenAI-compatible,
//! OpenAI new-style, or Anthropic Claude) and evaluates the response against
//! ground truth.
//!
//! Prompts are loaded from `resources/llm_prompts.yaml` (compiled in via
//! `include_str!`) and can be overridden at runtime via `LLM_PROMPTS_PATH`.

use std::error::Error;
use std::sync::LazyLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};

// ── Prompt config ────────────────────────────────────────────────────

/// Prompt templates loaded from YAML.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptConfig {
    pub inference_prompt: String,
    pub judge_prompt: String,
    pub judge_max_tokens: u32,
}

impl PromptConfig {
    /// Load from a YAML file path. Returns the compiled-in default if path is None.
    pub fn load(path: Option<&str>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let yaml = match path {
            Some(p) => std::fs::read_to_string(p)?,
            None => include_str!("../resources/llm_prompts.yaml").to_string(),
        };
        Ok(serde_yaml::from_str(&yaml)?)
    }
}

static PROMPT_CONFIG: LazyLock<PromptConfig> = LazyLock::new(|| {
    let yaml = match std::env::var("LLM_PROMPTS_PATH") {
        Ok(path) => std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read prompts from {path}: {e}")),
        Err(_) => include_str!("../resources/llm_prompts.yaml").to_string(),
    };
    serde_yaml::from_str(&yaml).expect("failed to parse llm_prompts.yaml")
});

/// Supported LLM API providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    /// OpenAI-compatible (vLLM, OpenAI GPT-4.1, etc.)
    OpenAI,
    /// OpenAI models that require max_completion_tokens (GPT-5.x, o3, o4)
    OpenAINew,
    /// Anthropic Claude API
    Anthropic,
}

/// Transport mode for the semantic-class classifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmSemanticClassifierMode {
    /// OpenAI-compatible `chat/completions` JSON classification.
    ChatCompletions,
    /// vLLM pooling `/score` endpoint for rerankers and scorers.
    Score,
}

/// Configuration for the LLM evaluator.
#[derive(Debug, Clone)]
pub struct LlmEvaluatorConfig {
    /// API endpoint.
    pub endpoint: String,
    /// Model name to use.
    pub model: String,
    /// API provider.
    pub provider: LlmProvider,
    /// API key (for OpenAI/Anthropic).
    pub api_key: Option<String>,
    /// Max tokens for the response.
    pub max_tokens: u32,
    /// Temperature (0.0 = deterministic).
    pub temperature: f64,
    /// Optional TLS client cert path for mTLS.
    pub tls_cert_path: Option<String>,
    /// Optional TLS client key path for mTLS.
    pub tls_key_path: Option<String>,
    /// Whether to skip TLS verification.
    pub tls_insecure: bool,
    /// Separate endpoint for the LLM-as-judge. Falls back to main endpoint.
    pub judge_endpoint: Option<String>,
    /// Separate model for the judge. Falls back to main model.
    pub judge_model: Option<String>,
    /// Provider for the judge. Falls back to main provider.
    pub judge_provider: Option<LlmProvider>,
    /// API key for judge. Falls back to main api_key.
    pub judge_api_key: Option<String>,
    /// Separate endpoint for the semantic-class classifier. Falls back to None.
    pub semantic_classifier_endpoint: Option<String>,
    /// Separate model for the semantic-class classifier.
    pub semantic_classifier_model: Option<String>,
    /// Provider for the semantic-class classifier.
    pub semantic_classifier_provider: Option<LlmProvider>,
    /// API key for the semantic-class classifier. Falls back to main api_key.
    pub semantic_classifier_api_key: Option<String>,
    /// Transport mode for the semantic-class classifier.
    pub semantic_classifier_mode: LlmSemanticClassifierMode,
}

impl LlmEvaluatorConfig {
    pub fn from_env() -> Self {
        let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "Qwen/Qwen3-8B".to_string());
        let provider = detect_provider(&std::env::var("LLM_PROVIDER").unwrap_or_default(), &model);
        let judge_model = std::env::var("LLM_JUDGE_MODEL").ok();
        let judge_provider = judge_model
            .as_deref()
            .map(|m| detect_provider(&std::env::var("LLM_JUDGE_PROVIDER").unwrap_or_default(), m));
        let semantic_classifier_endpoint = std::env::var("LLM_SEMANTIC_CLASSIFIER_ENDPOINT").ok();
        let semantic_classifier_model = std::env::var("LLM_SEMANTIC_CLASSIFIER_MODEL").ok();
        let semantic_classifier_provider = semantic_classifier_model.as_deref().map(|m| {
            detect_provider(
                &std::env::var("LLM_SEMANTIC_CLASSIFIER_PROVIDER").unwrap_or_default(),
                m,
            )
        });
        let semantic_classifier_mode = detect_semantic_classifier_mode(
            &std::env::var("LLM_SEMANTIC_CLASSIFIER_MODE").unwrap_or_default(),
            semantic_classifier_endpoint.as_deref(),
        );
        Self {
            endpoint: std::env::var("LLM_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:8000/v1/chat/completions".to_string()),
            model,
            provider,
            api_key: std::env::var("LLM_API_KEY").ok(),
            max_tokens: std::env::var("LLM_MAX_TOKENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(200),
            temperature: std::env::var("LLM_TEMPERATURE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.1),
            tls_cert_path: std::env::var("LLM_TLS_CERT_PATH").ok(),
            tls_key_path: std::env::var("LLM_TLS_KEY_PATH").ok(),
            tls_insecure: std::env::var("LLM_TLS_INSECURE")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            judge_endpoint: std::env::var("LLM_JUDGE_ENDPOINT").ok(),
            judge_model,
            judge_provider,
            judge_api_key: std::env::var("LLM_JUDGE_API_KEY").ok(),
            semantic_classifier_endpoint,
            semantic_classifier_model,
            semantic_classifier_provider,
            semantic_classifier_api_key: std::env::var("LLM_SEMANTIC_CLASSIFIER_API_KEY").ok(),
            semantic_classifier_mode,
        }
    }
}

fn detect_provider(explicit: &str, model: &str) -> LlmProvider {
    match explicit {
        "anthropic" => LlmProvider::Anthropic,
        "openai_new" | "openai-new" => LlmProvider::OpenAINew,
        "openai" => LlmProvider::OpenAI,
        _ => {
            if model.starts_with("claude") {
                LlmProvider::Anthropic
            } else if model.starts_with("gpt-5")
                || model.starts_with("o3")
                || model.starts_with("o4")
            {
                LlmProvider::OpenAINew
            } else {
                LlmProvider::OpenAI
            }
        }
    }
}

fn detect_semantic_classifier_mode(
    explicit: &str,
    endpoint: Option<&str>,
) -> LlmSemanticClassifierMode {
    match explicit {
        "score" | "score_api" | "score-api" => LlmSemanticClassifierMode::Score,
        "chat" | "chat_completions" | "chat-completions" => {
            LlmSemanticClassifierMode::ChatCompletions
        }
        _ => endpoint
            .map(|value| value.trim_end_matches('/').ends_with("/score"))
            .filter(|is_score| *is_score)
            .map(|_| LlmSemanticClassifierMode::Score)
            .unwrap_or(LlmSemanticClassifierMode::ChatCompletions),
    }
}

/// Ground truth for evaluating LLM responses.
///
/// Fields should contain **human-readable descriptions** (node kind, title,
/// summary) rather than opaque IDs. The judge evaluates semantically.
#[derive(Debug, Clone)]
pub struct EvaluationGroundTruth {
    /// Description of the failure point (e.g. "incident — System incident requiring diagnosis").
    pub expected_failure_point: Option<String>,
    /// Description of the restart node (e.g. "decision node — the first recovery decision").
    pub expected_restart_node: Option<String>,
    /// The expected dominant reason or rationale (from the main causal chain).
    pub expected_reason: Option<String>,
    /// Distractor rationale from noise/competing branches. Used by the judge
    /// to detect when the agent cites plausible-but-wrong reasoning.
    pub distractor_rationale: Option<String>,
    /// Short domain label for context (e.g. "operations", "software debugging").
    pub domain_context: Option<String>,
}

/// Result of an LLM evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmEvaluationResult {
    pub llm_response: String,
    pub llm_task_success: bool,
    pub llm_restart_accuracy: bool,
    pub llm_restart_exact: bool,
    pub llm_restart_off_by_one: bool,
    pub llm_restart_on_competing: bool,
    pub llm_restart_explained: bool,
    /// Backward-compatible aggregate: true when `reason_correct_main_path` is true.
    pub llm_reason_preserved: bool,
    /// Agent cited rationale from the main causal chain (ground truth path).
    pub llm_reason_correct: bool,
    /// Agent cited rationale from a distractor/noise branch.
    pub llm_reason_distractor: bool,
    /// Self-declared source: "graph_metadata", "inferred", or "not_available".
    pub llm_reason_source: String,
    /// Self-declared confidence: "high", "medium", or "low".
    pub llm_confidence: String,
    /// Fabrication detected: model claims `graph_metadata` but ground truth has no rationale.
    /// Computed deterministically by the evaluator, not by the judge.
    pub llm_reason_fabricated: bool,
    pub llm_latency_ms: f64,
    pub llm_prompt_tokens: u32,
    pub llm_completion_tokens: u32,
    /// Raw judge response for post-hoc analysis. `None` when judge call failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_judge_raw: Option<String>,
}

/// Call an LLM endpoint and return `(content, prompt_tokens, completion_tokens)`.
///
/// Dispatches to the correct wire format based on [`LlmProvider`].
#[allow(clippy::too_many_arguments)]
pub async fn call_llm(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    provider: LlmProvider,
    api_key: Option<&str>,
    prompt: &str,
    max_tokens: u32,
    temperature: f64,
) -> Result<(String, u32, u32), Box<dyn Error + Send + Sync>> {
    match provider {
        LlmProvider::OpenAI | LlmProvider::OpenAINew => {
            call_openai(
                client,
                endpoint,
                model,
                provider,
                api_key,
                prompt,
                max_tokens,
                temperature,
            )
            .await
        }
        LlmProvider::Anthropic => {
            call_anthropic(client, endpoint, model, api_key, prompt, max_tokens).await
        }
    }
}

// ── OpenAI / OpenAI-new wire format ─────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn call_openai(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    provider: LlmProvider,
    api_key: Option<&str>,
    prompt: &str,
    max_tokens: u32,
    temperature: f64,
) -> Result<(String, u32, u32), Box<dyn Error + Send + Sync>> {
    let mut body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": temperature,
    });

    // vLLM + Qwen3 thinking mode.
    //
    // Refs:
    //   - vLLM:  github.com/vllm-project/vllm docs/features/reasoning_outputs.md
    //   - Qwen3: huggingface.co/Qwen/Qwen3-8B
    //
    // Server requires: --reasoning-parser=qwen3 --reasoning-config '{"think_start_str":"<think>","think_end_str":"</think>"}'
    // See k8s/vllm-thinking.yaml.
    //
    // With --reasoning-config, thinking_token_budget is a hard cap enforced by
    // the server, independent of max_tokens. Without it, max_tokens controls
    // the total output and thinking consumes the entire budget.
    //
    // IMPORTANT: do NOT send chat_template_kwargs: {enable_thinking: true}.
    // That overrides the chat template and breaks the reasoning parser.
    //
    // LLM_ENABLE_THINKING env var:
    //   "false"/"0" → explicitly disables thinking (sends enable_thinking: false)
    //   any other value or unset → Qwen3 thinks by default (no override sent)
    if provider == LlmProvider::OpenAI && !is_official_openai_endpoint(endpoint) {
        let disable_thinking = std::env::var("LLM_ENABLE_THINKING")
            .map(|v| v == "false" || v == "0")
            .unwrap_or(false);
        if disable_thinking {
            body["chat_template_kwargs"] = serde_json::json!({"enable_thinking": false});
        } else {
            // Limit reasoning tokens so the model has room for the JSON answer.
            // Requires --reasoning-config on the server (k8s/vllm-qwen3-8b.yaml).
            // vLLM forces </think> when the budget is exhausted.
            body["thinking_token_budget"] = serde_json::json!(512);
        }
    }

    // GPT-5.x / o3 / o4 require `max_completion_tokens` instead of `max_tokens`.
    match provider {
        LlmProvider::OpenAINew => {
            body["max_completion_tokens"] = serde_json::json!(max_tokens);
        }
        _ => {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
    }

    let mut req = client.post(endpoint).json(&body);
    match provider {
        LlmProvider::OpenAINew => {
            // OpenAI-new always requires a bearer token.
            if let Some(key) = api_key {
                req = req.bearer_auth(key);
            }
        }
        _ => {
            if let Some(key) = api_key {
                req = req.bearer_auth(key);
            }
        }
    }

    let response = req.send().await?;
    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        return Err(format!("LLM request failed with {status}: {body_text}").into());
    }

    let chat: OpenAiChatResponse = response.json().await?;
    let msg = chat.choices.first().map(|c| &c.message);
    // When reasoning-parser is active, content has the clean answer and
    // reasoning/reasoning_content has the CoT. Use content directly.
    // When parser is not active, content may contain <think> tags —
    // strip_thinking_tags handles that downstream.
    let content = msg.and_then(|m| m.content.clone()).unwrap_or_default();
    let usage = chat.usage.unwrap_or(OpenAiUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
    });
    Ok((content, usage.prompt_tokens, usage.completion_tokens))
}

fn is_official_openai_endpoint(endpoint: &str) -> bool {
    endpoint.starts_with("https://api.openai.com/")
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
    /// vLLM reasoning parser: CoT thinking separated from answer.
    /// Renamed from `reasoning_content` to `reasoning` in newer vLLM versions.
    #[allow(dead_code)]
    reasoning_content: Option<String>,
    #[allow(dead_code)]
    reasoning: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

// ── Anthropic wire format ───────────────────────────────────────────

async fn call_anthropic(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
    prompt: &str,
    max_tokens: u32,
) -> Result<(String, u32, u32), Box<dyn Error + Send + Sync>> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [{"role": "user", "content": prompt}],
    });

    let mut req = client
        .post(endpoint)
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .json(&body);

    if let Some(key) = api_key {
        req = req.header("x-api-key", key);
    }

    let response = req.send().await?;
    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        return Err(format!("LLM request failed with {status}: {body_text}").into());
    }

    let resp: AnthropicResponse = response.json().await?;
    let content = resp
        .content
        .first()
        .map(|b| b.text.clone())
        .unwrap_or_default();
    let prompt_tokens = resp.usage.input_tokens;
    let completion_tokens = resp.usage.output_tokens;
    Ok((content, prompt_tokens, completion_tokens))
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicBlock>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    text: String,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// ── evaluate_with_llm ───────────────────────────────────────────────

/// Evaluate rendered context against ground truth using an LLM.
pub async fn evaluate_with_llm(
    config: &LlmEvaluatorConfig,
    rendered_context: &str,
    question: &str,
    ground_truth: &EvaluationGroundTruth,
) -> Result<LlmEvaluationResult, Box<dyn Error + Send + Sync>> {
    let prompt = PROMPT_CONFIG
        .inference_prompt
        .replace("{rendered_context}", rendered_context)
        .replace("{question}", question);

    eprintln!(
        "[LLM-EVAL] inference question: {}",
        question
            .replace('\n', " ")
            .chars()
            .take(200)
            .collect::<String>()
    );

    let client = build_http_client(config)?;

    let start = std::time::Instant::now();
    let (content, prompt_tokens, completion_tokens) = call_llm(
        &client,
        &config.endpoint,
        &config.model,
        config.provider,
        config.api_key.as_deref(),
        &prompt,
        config.max_tokens,
        config.temperature,
    )
    .await?;
    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
    let content = normalize_llm_json_response(&content);

    // LLM-as-judge: use separate judge endpoint if configured
    let judge_client = if config.judge_endpoint.is_some() {
        build_http_client(config)?
    } else {
        client
    };
    eprintln!(
        "[LLM-EVAL] inference response: {}",
        content
            .replace('\n', " ")
            .chars()
            .take(500)
            .collect::<String>()
    );
    let (judge_result, judge_raw) = match judge_response(
        config,
        &judge_client,
        &content,
        ground_truth,
    )
    .await
    {
        Ok((verdict, raw)) => {
            eprintln!(
                "[LLM-EVAL] judge verdict: task={} restart={} (exact={} off1={} competing={} explained={}) reason_correct={} reason_distractor={}",
                verdict.task_correct,
                verdict.restart_correct,
                verdict.restart_exact,
                verdict.restart_off_by_one,
                verdict.restart_on_competing_branch,
                verdict.restart_explained,
                verdict.reason_correct_main_path,
                verdict.reason_plausible_but_wrong
            );
            (verdict, Some(raw))
        }
        Err(e) => {
            eprintln!("[LLM-EVAL] judge FAILED: {e}");
            (
                JudgeVerdict {
                    task_correct: false,
                    restart_correct: false,
                    restart_exact: false,
                    restart_off_by_one: false,
                    restart_on_competing_branch: false,
                    restart_explained: false,
                    reason_correct_main_path: false,
                    reason_plausible_but_wrong: false,
                },
                None,
            )
        }
    };

    let (reason_source, confidence) = extract_source_fields(&content);
    let rationale_exists = ground_truth
        .expected_reason
        .as_ref()
        .is_some_and(|r| !r.is_empty() && r != "none");
    let reason_fabricated = reason_source == "graph_metadata" && !rationale_exists;

    Ok(verdict_to_result(
        judge_result,
        EvalContext {
            response: content,
            reason_source,
            confidence,
            reason_fabricated,
            latency_ms,
            prompt_tokens,
            completion_tokens,
            judge_raw,
        },
    ))
}

/// Evaluate with explicit prompt config and LLM config — no global state.
/// Use this to iterate model×prompt cells within a single process.
pub async fn evaluate_with_config(
    prompts: &PromptConfig,
    config: &LlmEvaluatorConfig,
    rendered_context: &str,
    question: &str,
    ground_truth: &EvaluationGroundTruth,
) -> Result<LlmEvaluationResult, Box<dyn Error + Send + Sync>> {
    let prompt = prompts
        .inference_prompt
        .replace("{rendered_context}", rendered_context)
        .replace("{question}", question);

    eprintln!(
        "[LLM-EVAL] inference question: {}",
        question
            .replace('\n', " ")
            .chars()
            .take(200)
            .collect::<String>()
    );

    let client = build_http_client(config)?;

    let start = std::time::Instant::now();
    let (content, prompt_tokens, completion_tokens) = call_llm(
        &client,
        &config.endpoint,
        &config.model,
        config.provider,
        config.api_key.as_deref(),
        &prompt,
        config.max_tokens,
        config.temperature,
    )
    .await?;
    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
    let content = normalize_llm_json_response(&content);

    let judge_client = if config.judge_endpoint.is_some() {
        build_http_client(config)?
    } else {
        client
    };
    eprintln!(
        "[LLM-EVAL] inference response: {}",
        content
            .replace('\n', " ")
            .chars()
            .take(500)
            .collect::<String>()
    );
    let (judge_result, judge_raw) = match judge_response_with_prompts(
        prompts,
        config,
        &judge_client,
        &content,
        ground_truth,
    )
    .await
    {
        Ok((verdict, raw)) => {
            eprintln!(
                "[LLM-EVAL] judge verdict: task={} restart={} (exact={} off1={} competing={} explained={}) reason_correct={} reason_distractor={}",
                verdict.task_correct,
                verdict.restart_correct,
                verdict.restart_exact,
                verdict.restart_off_by_one,
                verdict.restart_on_competing_branch,
                verdict.restart_explained,
                verdict.reason_correct_main_path,
                verdict.reason_plausible_but_wrong
            );
            (verdict, Some(raw))
        }
        Err(e) => {
            eprintln!("[LLM-EVAL] judge FAILED: {e}");
            (
                JudgeVerdict {
                    task_correct: false,
                    restart_correct: false,
                    restart_exact: false,
                    restart_off_by_one: false,
                    restart_on_competing_branch: false,
                    restart_explained: false,
                    reason_correct_main_path: false,
                    reason_plausible_but_wrong: false,
                },
                None,
            )
        }
    };

    let (reason_source, confidence) = extract_source_fields(&content);
    let rationale_exists = ground_truth
        .expected_reason
        .as_ref()
        .is_some_and(|r| !r.is_empty() && r != "none");
    let reason_fabricated = reason_source == "graph_metadata" && !rationale_exists;

    Ok(verdict_to_result(
        judge_result,
        EvalContext {
            response: content,
            reason_source,
            confidence,
            reason_fabricated,
            latency_ms,
            prompt_tokens,
            completion_tokens,
            judge_raw,
        },
    ))
}

struct EvalContext {
    response: String,
    reason_source: String,
    confidence: String,
    reason_fabricated: bool,
    latency_ms: f64,
    prompt_tokens: u32,
    completion_tokens: u32,
    judge_raw: Option<String>,
}

fn verdict_to_result(v: JudgeVerdict, ctx: EvalContext) -> LlmEvaluationResult {
    LlmEvaluationResult {
        llm_response: ctx.response,
        llm_task_success: v.task_correct,
        llm_restart_accuracy: v.restart_correct,
        llm_restart_exact: v.restart_exact,
        llm_restart_off_by_one: v.restart_off_by_one,
        llm_restart_on_competing: v.restart_on_competing_branch,
        llm_restart_explained: v.restart_explained,
        llm_reason_preserved: v.reason_correct_main_path,
        llm_reason_correct: v.reason_correct_main_path,
        llm_reason_distractor: v.reason_plausible_but_wrong,
        llm_reason_source: ctx.reason_source,
        llm_confidence: ctx.confidence,
        llm_reason_fabricated: ctx.reason_fabricated,
        llm_latency_ms: ctx.latency_ms,
        llm_prompt_tokens: ctx.prompt_tokens,
        llm_completion_tokens: ctx.completion_tokens,
        llm_judge_raw: ctx.judge_raw,
    }
}

/// Extract `reason_source` and `confidence` from the inference response JSON.
/// Returns defaults ("unknown", "unknown") if parsing fails.
fn extract_source_fields(response: &str) -> (String, String) {
    let reason_source = serde_json::from_str::<serde_json::Value>(response)
        .ok()
        .and_then(|v| v["reason_source"].as_str().map(String::from))
        .unwrap_or_else(|| "unknown".to_string());
    let confidence = serde_json::from_str::<serde_json::Value>(response)
        .ok()
        .and_then(|v| v["confidence"].as_str().map(String::from))
        .unwrap_or_else(|| "unknown".to_string());
    (reason_source, confidence)
}

// ── judge ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JudgeVerdict {
    task_correct: bool,
    restart_correct: bool,
    #[serde(default)]
    restart_exact: bool,
    #[serde(default)]
    restart_off_by_one: bool,
    #[serde(default)]
    restart_on_competing_branch: bool,
    #[serde(default)]
    restart_explained: bool,
    reason_correct_main_path: bool,
    #[serde(default)]
    reason_plausible_but_wrong: bool,
}

async fn judge_response(
    config: &LlmEvaluatorConfig,
    client: &reqwest::Client,
    llm_response: &str,
    ground_truth: &EvaluationGroundTruth,
) -> Result<(JudgeVerdict, String), Box<dyn Error + Send + Sync>> {
    judge_response_with_prompts(&PROMPT_CONFIG, config, client, llm_response, ground_truth).await
}

async fn judge_response_with_prompts(
    prompts: &PromptConfig,
    config: &LlmEvaluatorConfig,
    client: &reqwest::Client,
    llm_response: &str,
    ground_truth: &EvaluationGroundTruth,
) -> Result<(JudgeVerdict, String), Box<dyn Error + Send + Sync>> {
    let expected_failure = ground_truth
        .expected_failure_point
        .as_deref()
        .unwrap_or("not specified");
    let expected_restart = ground_truth
        .expected_restart_node
        .as_deref()
        .unwrap_or("not specified");
    let expected_reason = ground_truth
        .expected_reason
        .as_deref()
        .unwrap_or("not specified");
    let domain_ctx = ground_truth
        .domain_context
        .as_deref()
        .unwrap_or("operational");

    let distractor = ground_truth
        .distractor_rationale
        .as_deref()
        .unwrap_or("none");

    let judge_prompt = prompts
        .judge_prompt
        .replace("{domain_context}", domain_ctx)
        .replace("{llm_response}", llm_response)
        .replace("{expected_failure}", expected_failure)
        .replace("{expected_restart}", expected_restart)
        .replace("{expected_reason}", expected_reason)
        .replace("{distractor_rationale}", distractor);

    let judge_endpoint = config.judge_endpoint.as_deref().unwrap_or(&config.endpoint);
    let judge_model = config.judge_model.as_deref().unwrap_or(&config.model);
    let judge_provider = config.judge_provider.unwrap_or(config.provider);
    let judge_api_key = config
        .judge_api_key
        .as_deref()
        .or(config.api_key.as_deref());

    eprintln!(
        "[LLM-EVAL] judge prompt ground truth: failure={} restart={} reason={} distractor={}",
        expected_failure.chars().take(80).collect::<String>(),
        expected_restart.chars().take(80).collect::<String>(),
        expected_reason.chars().take(80).collect::<String>(),
        distractor.chars().take(80).collect::<String>()
    );

    let (judge_content, _, _) = call_llm(
        client,
        judge_endpoint,
        judge_model,
        judge_provider,
        judge_api_key,
        &judge_prompt,
        prompts.judge_max_tokens,
        0.0,
    )
    .await?;

    let json_str = strip_markdown_fences(&judge_content);

    let verdict: JudgeVerdict = serde_json::from_str(&json_str)
        .map_err(|e| format!("failed to parse judge response '{judge_content}': {e}"))?;
    Ok((verdict, judge_content))
}

/// Strips markdown code fences from LLM responses.
///
/// Many models (especially Claude) wrap JSON in ` ```json ... ``` `.
/// This normalizes the response to plain text before parsing or passing
/// to the judge.
/// Strips `<think>...</think>` blocks from reasoning model output.
/// If the result is empty after stripping, extracts JSON from inside the thinking block.
fn strip_thinking_tags(s: &str) -> String {
    let input = s.to_string();

    // Try to extract content AFTER </think>
    if let Some(end_idx) = input.find("</think>") {
        let after = input[end_idx + "</think>".len()..].trim();
        if !after.is_empty() {
            return after.to_string();
        }
    }

    // No content after </think> — look for JSON inside the thinking block
    if let Some(start) = input.find("<think>") {
        let inside = if let Some(end) = input.find("</think>") {
            &input[start + "<think>".len()..end]
        } else {
            &input[start + "<think>".len()..]
        };
        // Find first { ... last } inside thinking
        if let (Some(json_start), Some(json_end)) = (inside.find('{'), inside.rfind('}')) {
            return inside[json_start..=json_end].to_string();
        }
    }

    // No thinking tags — pass through. If input still has <think>, return empty.
    let trimmed = input.trim();
    if trimmed.contains("<think>") {
        String::new()
    } else {
        trimmed.to_string()
    }
}

fn strip_markdown_fences(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.starts_with("```") {
        let without_opening = if let Some(after_lang) = trimmed.strip_prefix("```json") {
            after_lang
        } else if let Some(after_lang) = trimmed.strip_prefix("```JSON") {
            after_lang
        } else if let Some(after_tick) = trimmed.strip_prefix("```") {
            after_tick
        } else {
            trimmed
        };
        without_opening.trim_end_matches("```").trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Normalizes LLM JSON-ish output before schema parsing.
///
/// Removes provider-specific reasoning wrappers and markdown fences while
/// preserving the inner JSON payload.
pub fn normalize_llm_json_response(s: &str) -> String {
    strip_markdown_fences(&strip_thinking_tags(s))
}

// ── calibration ─────────────────────────────────────────────────────

/// Result of a single calibration case.
#[derive(Debug)]
pub struct CalibrationCase {
    pub name: &'static str,
    pub passed: bool,
    pub expected: &'static str,
    pub got: String,
}

/// Run judge calibration with known-good and known-bad synthetic responses.
///
/// Returns a list of calibration cases with pass/fail. Call this before
/// the full benchmark to verify the judge model + prompt combination is
/// producing sane verdicts. Avoids wasting money on a miscalibrated run.
pub async fn calibrate_judge(
    prompts: &PromptConfig,
    config: &LlmEvaluatorConfig,
) -> Result<Vec<CalibrationCase>, Box<dyn Error + Send + Sync>> {
    let client = build_http_client(config)?;
    let judge_client = if config.judge_endpoint.is_some() {
        build_http_client(config)?
    } else {
        client
    };

    let ground_truth = EvaluationGroundTruth {
        expected_failure_point: Some("evidence 3 (cal:chain-3) — operational step 3".to_string()),
        expected_restart_node: Some("artifact 2 (cal:chain-2) — causal predecessor".to_string()),
        expected_reason: Some(
            "operational response required at depth 0; operational response required at depth 1"
                .to_string(),
        ),
        distractor_rationale: Some("alternative path (competing branch)".to_string()),
        domain_context: Some("operations".to_string()),
    };

    // Known-good: response that correctly identifies failure, restart, and rationale
    let good_response = r#"{"failure_point": "evidence 3 — the deepest node in the operational causal chain, representing the final step of operational response", "restart_node": "artifact 2 — causal predecessor of evidence 3, the correct recovery point because it is the last successful step before the failure", "reason": "The rationale connecting the chain is 'operational response required at depth 0' triggering the chain through depth 1, as stated in the relationship metadata."}"#;

    // Known-bad: response that gets everything wrong
    let bad_response = r#"{"failure_point": "the system root node", "restart_node": "some unrelated node", "reason": "I think something went wrong somewhere in the system."}"#;

    let mut cases = Vec::new();

    // Case 1: good response should get task=true, reason_correct=true
    match judge_response_with_prompts(prompts, config, &judge_client, good_response, &ground_truth)
        .await
    {
        Ok((v, raw)) => {
            let task_ok = v.task_correct;
            let reason_ok = v.reason_correct_main_path;
            cases.push(CalibrationCase {
                name: "known-good: task_correct",
                passed: task_ok,
                expected: "true",
                got: format!("{task_ok} (raw: {raw})"),
            });
            cases.push(CalibrationCase {
                name: "known-good: reason_correct_main_path",
                passed: reason_ok,
                expected: "true",
                got: format!("{reason_ok}"),
            });
        }
        Err(e) => {
            cases.push(CalibrationCase {
                name: "known-good: judge call",
                passed: false,
                expected: "success",
                got: format!("ERROR: {e}"),
            });
        }
    }

    // Case 2: bad response should get task=false, reason_correct=false
    match judge_response_with_prompts(prompts, config, &judge_client, bad_response, &ground_truth)
        .await
    {
        Ok((v, raw)) => {
            let task_fail = !v.task_correct;
            let reason_fail = !v.reason_correct_main_path;
            cases.push(CalibrationCase {
                name: "known-bad: task_correct",
                passed: task_fail,
                expected: "false",
                got: format!("{} (raw: {raw})", v.task_correct),
            });
            cases.push(CalibrationCase {
                name: "known-bad: reason_correct_main_path",
                passed: reason_fail,
                expected: "false",
                got: format!("{}", v.reason_correct_main_path),
            });
        }
        Err(e) => {
            cases.push(CalibrationCase {
                name: "known-bad: judge call",
                passed: false,
                expected: "success",
                got: format!("ERROR: {e}"),
            });
        }
    }

    Ok(cases)
}

/// Calibrate the inference agent before running the full benchmark.
///
/// Sends a minimal prompt and checks that the agent returns non-empty JSON
/// containing the required fields (failure_point, restart_node, reason,
/// reason_source, confidence). Catches misconfigured thinking mode, empty
/// responses, and malformed JSON before wasting eval budget.
pub async fn calibrate_agent(
    config: &LlmEvaluatorConfig,
) -> Result<Vec<CalibrationCase>, Box<dyn Error + Send + Sync>> {
    let client = build_http_client(config)?;

    let prompt = "Given this rehydrated context from an ops graph:\n\
        Objective: Incident Alpha — System outage\n\
        [node] root: incident \"Incident Alpha\" (ACTIVE)\n\
        [node] chain-0: decision \"Decision 0\" (ACTIVE)\n\
        [causal] root → chain-0: TRIGGERS (rationale: failure triggered recovery)\n\n\
        1. What is the deepest failure point in the causal chain?\n\
        2. Which node should the system restart from to recover?\n\
        3. What rationale connects the nodes in the causal chain?\n\n\
        Respond with JSON: {\"failure_point\": \"...\", \"restart_node\": \"...\", \
        \"reason\": \"...\", \"reason_source\": \"graph_metadata|inferred|not_available\", \
        \"confidence\": \"high|medium|low\"}";

    let mut cases = Vec::new();

    match call_llm(
        &client,
        &config.endpoint,
        &config.model,
        config.provider,
        config.api_key.as_deref(),
        prompt,
        config.max_tokens,
        config.temperature,
    )
    .await
    {
        Ok((raw_content, _prompt_tokens, completion_tokens)) => {
            let content = normalize_llm_json_response(&raw_content);

            // Check 1: non-empty response
            let non_empty = !content.trim().is_empty();
            cases.push(CalibrationCase {
                name: "agent: non-empty response",
                passed: non_empty,
                expected: "non-empty content after strip_thinking_tags",
                got: if non_empty {
                    format!("{} chars, {} completion tokens", content.len(), completion_tokens)
                } else {
                    format!(
                        "EMPTY (raw={} chars, {} completion tokens — thinking may have consumed all tokens)",
                        raw_content.len(),
                        completion_tokens
                    )
                },
            });

            if non_empty {
                // Check 2: valid JSON
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(&content);
                let json_ok = parsed.is_ok();
                cases.push(CalibrationCase {
                    name: "agent: valid JSON",
                    passed: json_ok,
                    expected: "parseable JSON object",
                    got: if json_ok {
                        "OK".to_string()
                    } else {
                        format!(
                            "PARSE ERROR: {}",
                            content.chars().take(100).collect::<String>()
                        )
                    },
                });

                // Check 3: required fields present
                if let Ok(ref obj) = parsed {
                    for field in &[
                        "failure_point",
                        "restart_node",
                        "reason",
                        "reason_source",
                        "confidence",
                    ] {
                        let present = obj.get(field).is_some_and(|v| !v.is_null());
                        cases.push(CalibrationCase {
                            name: Box::leak(format!("agent: field '{field}'").into_boxed_str()),
                            passed: present,
                            expected: "present and non-null",
                            got: if present {
                                obj[field].as_str().unwrap_or("(non-string)").to_string()
                            } else {
                                "MISSING".to_string()
                            },
                        });
                    }
                }
            }
        }
        Err(e) => {
            cases.push(CalibrationCase {
                name: "agent: inference call",
                passed: false,
                expected: "successful HTTP response",
                got: format!("ERROR: {e}"),
            });
        }
    }

    Ok(cases)
}

pub(crate) fn build_http_client(
    config: &LlmEvaluatorConfig,
) -> Result<reqwest::Client, Box<dyn Error + Send + Sync>> {
    build_http_client_with_connect_timeout(config, None)
}

pub(crate) fn build_http_client_with_connect_timeout(
    config: &LlmEvaluatorConfig,
    connect_timeout: Option<Duration>,
) -> Result<reqwest::Client, Box<dyn Error + Send + Sync>> {
    let mut builder = reqwest::Client::builder();

    if config.tls_insecure {
        return Err(
            "LLM_TLS_INSECURE is no longer supported; require valid TLS certificates".into(),
        );
    }

    if let Some(connect_timeout) = connect_timeout {
        builder = builder.connect_timeout(connect_timeout);
    }

    if let (Some(cert_path), Some(key_path)) = (&config.tls_cert_path, &config.tls_key_path) {
        let cert_pem = std::fs::read(cert_path)?;
        let key_pem = std::fs::read(key_path)?;
        let mut combined = cert_pem;
        combined.extend_from_slice(&key_pem);
        let identity = reqwest::Identity::from_pem(&combined)?;
        builder = builder.identity(identity);
    }

    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_from_env_uses_defaults() {
        let config = LlmEvaluatorConfig::from_env();
        assert!(config.endpoint.contains("chat/completions"));
        assert!(!config.model.is_empty());
        assert!(config.max_tokens > 0);
        // Default provider for Qwen model is OpenAI-compatible
        assert_eq!(config.provider, LlmProvider::OpenAI);
        // No API key set by default
        assert!(config.api_key.is_none());
        // Judge fields fall back to None
        assert!(config.judge_provider.is_none());
        assert!(config.judge_api_key.is_none());
        assert!(config.semantic_classifier_provider.is_none());
        assert!(config.semantic_classifier_api_key.is_none());
        assert_eq!(
            config.semantic_classifier_mode,
            LlmSemanticClassifierMode::ChatCompletions
        );
    }

    #[test]
    fn semantic_classifier_mode_defaults_to_score_for_score_endpoint() {
        let mode = detect_semantic_classifier_mode("", Some("http://vllm:8000/score"));
        assert_eq!(mode, LlmSemanticClassifierMode::Score);
    }

    #[test]
    fn evaluation_result_serializes_to_json() {
        let result = LlmEvaluationResult {
            llm_response: "test".to_string(),
            llm_task_success: true,
            llm_restart_accuracy: true,
            llm_restart_exact: true,
            llm_restart_off_by_one: false,
            llm_restart_on_competing: false,
            llm_restart_explained: true,
            llm_reason_preserved: false,
            llm_reason_correct: false,
            llm_reason_distractor: false,
            llm_reason_source: "graph_metadata".to_string(),
            llm_confidence: "high".to_string(),
            llm_reason_fabricated: false,
            llm_latency_ms: 123.4,
            llm_prompt_tokens: 50,
            llm_completion_tokens: 30,
            llm_judge_raw: Some("{\"task_correct\":true}".to_string()),
        };
        let json = serde_json::to_string(&result).expect("should serialize");
        assert!(json.contains("llm_task_success"));
        assert!(json.contains("llm_judge_raw"));
        assert!(json.contains("llm_reason_correct"));
        assert!(json.contains("llm_restart_exact"));
        assert!(json.contains("llm_restart_explained"));
    }

    #[test]
    fn strip_thinking_tags_removes_complete_block() {
        let input = r#"<think>
Let me analyze this carefully...
The failure point is the root incident.
</think>
{"failure_point": "root", "restart_node": "chain-0", "reason": "test"}"#;
        let result = strip_thinking_tags(input);
        assert!(result.starts_with('{'));
        assert!(!result.contains("<think>"));
        assert!(result.contains("failure_point"));
    }

    #[test]
    fn strip_thinking_tags_handles_no_thinking() {
        let input = r#"{"failure_point": "root", "restart_node": "chain-0", "reason": "test"}"#;
        let result = strip_thinking_tags(input);
        assert_eq!(result, input);
    }

    #[test]
    fn strip_thinking_tags_handles_unclosed_tag() {
        let input = r#"<think>
I'm still thinking about this...
"#;
        let result = strip_thinking_tags(input);
        assert!(!result.contains("<think>"));
    }

    #[test]
    fn strip_thinking_extracts_json_from_inside_thinking() {
        let input = r#"<think>
Let me analyze the graph structure carefully.
The root node is an incident...

{"failure_point": "root", "restart_node": "chain-0", "reason": "test", "reason_source": "graph_metadata", "confidence": "high"}
</think>"#;
        let result = strip_thinking_tags(input);
        assert!(result.starts_with('{'), "got: {result}");
        assert!(result.contains("failure_point"));
    }

    #[test]
    fn strip_thinking_then_fences_works() {
        let input = r#"<think>
Reasoning about the graph...
</think>
```json
{"failure_point": "root", "reason": "test", "restart_node": "n1"}
```"#;
        let result = strip_markdown_fences(&strip_thinking_tags(input));
        assert!(result.starts_with('{'), "got: {result}");
        assert!(result.contains("failure_point"));
    }
}
