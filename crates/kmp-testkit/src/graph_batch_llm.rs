use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::time::Duration;

use kmp_domain::RelationSemanticClass;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::llm_evaluator::{
    LlmEvaluatorConfig, LlmProvider, LlmSemanticClassifierMode,
    build_http_client_with_connect_timeout, call_llm,
};
use crate::{
    GraphBatch, LlmGraphError, llm_graph_to_projection_events, normalize_llm_json_response,
    parse_llm_graph_batch,
};

const THINKING_TOKEN_BUDGET: u32 = 512;
const MIN_GRAPH_BATCH_MAX_TOKENS: u32 = 2048;
const RETRY_DELAY_SCHEDULE_MS: [u64; 4] = [0, 250, 1_000, 3_000];
const PRIMARY_POLICY_ENV_PREFIX: &str = "LLM_GRAPH_BATCH_PRIMARY";
const REPAIR_POLICY_ENV_PREFIX: &str = "LLM_GRAPH_BATCH_REPAIR";
const SEMANTIC_CLASSIFIER_POLICY_ENV_PREFIX: &str = "LLM_GRAPH_BATCH_SEMANTIC_CLASSIFIER";
const PRIMARY_DEFAULT_MAX_ATTEMPTS: usize = 4;
const PRIMARY_DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 2;
const PRIMARY_DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 45;
const REPAIR_DEFAULT_MAX_ATTEMPTS: usize = 1;
const REPAIR_DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 2;
const REPAIR_DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 180;
const SEMANTIC_CLASSIFIER_DEFAULT_MAX_ATTEMPTS: usize = 1;
const SEMANTIC_CLASSIFIER_DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 2;
const SEMANTIC_CLASSIFIER_DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 20;
const SEMANTIC_CLASSIFIER_MIN_MAX_TOKENS: u32 = 128;
const SEMANTIC_CLASSIFIER_MAX_MAX_TOKENS: u32 = 1024;
const SEMANTIC_CLASS_SCORE_DOCUMENTS: [(RelationSemanticClass, &str); 6] = [
    (
        RelationSemanticClass::Causal,
        "semantic_class=causal. Use when the relation states actual cause and effect. Prefer this when a confirmed finding explains the incident or when caused_by_node_id is present.",
    ),
    (
        RelationSemanticClass::Motivational,
        "semantic_class=motivational. Use when the relation explains why a decision or mitigation was chosen, including tradeoffs or intent.",
    ),
    (
        RelationSemanticClass::Procedural,
        "semantic_class=procedural. Use when the relation mainly communicates ordered steps, methods, runbook actions, or execution sequence.",
    ),
    (
        RelationSemanticClass::Evidential,
        "semantic_class=evidential. Use when the relation mainly contributes logs, metrics, traces, diffs, or inspection evidence supporting a finding or decision.",
    ),
    (
        RelationSemanticClass::Constraint,
        "semantic_class=constraint. Use when the relation mainly communicates a limit, safety rule, dependency, policy, or operational restriction.",
    ),
    (
        RelationSemanticClass::Structural,
        "semantic_class=structural. Use when the relation is mostly neutral association, grouping, containment, or reference without explanatory force.",
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphBatchRetryPolicy {
    pub max_attempts: usize,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
}

impl Default for GraphBatchRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: PRIMARY_DEFAULT_MAX_ATTEMPTS,
            connect_timeout: Duration::from_secs(PRIMARY_DEFAULT_CONNECT_TIMEOUT_SECS),
            request_timeout: Duration::from_secs(PRIMARY_DEFAULT_REQUEST_TIMEOUT_SECS),
        }
    }
}

impl GraphBatchRetryPolicy {
    pub fn from_env() -> Self {
        Self::from_reader(|key| std::env::var(key).ok())
    }

    fn from_reader<F>(read: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let defaults = Self::default();
        Self {
            max_attempts: read_usize_with(
                &read,
                env_key(PRIMARY_POLICY_ENV_PREFIX, "MAX_ATTEMPTS"),
                defaults.max_attempts,
            ),
            connect_timeout: Duration::from_secs(read_u64_with(
                &read,
                env_key(PRIMARY_POLICY_ENV_PREFIX, "CONNECT_TIMEOUT_SECS"),
                defaults.connect_timeout.as_secs(),
            )),
            request_timeout: Duration::from_secs(read_u64_with(
                &read,
                env_key(PRIMARY_POLICY_ENV_PREFIX, "REQUEST_TIMEOUT_SECS"),
                defaults.request_timeout.as_secs(),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphBatchRequestOutcome {
    pub batch: GraphBatch,
    pub raw_content: String,
    pub normalized_content: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub attempts: usize,
    pub primary_attempts: usize,
    pub repair_attempts: usize,
    pub repaired_by_judge: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphBatchRepairJudgePolicy {
    pub max_attempts: usize,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
}

impl Default for GraphBatchRepairJudgePolicy {
    fn default() -> Self {
        Self {
            max_attempts: REPAIR_DEFAULT_MAX_ATTEMPTS,
            connect_timeout: Duration::from_secs(REPAIR_DEFAULT_CONNECT_TIMEOUT_SECS),
            request_timeout: Duration::from_secs(REPAIR_DEFAULT_REQUEST_TIMEOUT_SECS),
        }
    }
}

impl GraphBatchRepairJudgePolicy {
    pub fn from_env() -> Self {
        Self::from_reader(|key| std::env::var(key).ok())
    }

    fn from_reader<F>(read: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let defaults = Self::default();
        Self {
            max_attempts: read_usize_with(
                &read,
                env_key(REPAIR_POLICY_ENV_PREFIX, "MAX_ATTEMPTS"),
                defaults.max_attempts,
            ),
            connect_timeout: Duration::from_secs(read_u64_with(
                &read,
                env_key(REPAIR_POLICY_ENV_PREFIX, "CONNECT_TIMEOUT_SECS"),
                defaults.connect_timeout.as_secs(),
            )),
            request_timeout: Duration::from_secs(read_u64_with(
                &read,
                env_key(REPAIR_POLICY_ENV_PREFIX, "REQUEST_TIMEOUT_SECS"),
                defaults.request_timeout.as_secs(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphBatchSemanticClassifierPolicy {
    pub max_attempts: usize,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
}

impl Default for GraphBatchSemanticClassifierPolicy {
    fn default() -> Self {
        Self {
            max_attempts: SEMANTIC_CLASSIFIER_DEFAULT_MAX_ATTEMPTS,
            connect_timeout: Duration::from_secs(SEMANTIC_CLASSIFIER_DEFAULT_CONNECT_TIMEOUT_SECS),
            request_timeout: Duration::from_secs(SEMANTIC_CLASSIFIER_DEFAULT_REQUEST_TIMEOUT_SECS),
        }
    }
}

impl GraphBatchSemanticClassifierPolicy {
    pub fn from_env() -> Self {
        Self::from_reader(|key| std::env::var(key).ok())
    }

    fn from_reader<F>(read: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let defaults = Self::default();
        Self {
            max_attempts: read_usize_with(
                &read,
                env_key(SEMANTIC_CLASSIFIER_POLICY_ENV_PREFIX, "MAX_ATTEMPTS"),
                defaults.max_attempts,
            ),
            connect_timeout: Duration::from_secs(read_u64_with(
                &read,
                env_key(
                    SEMANTIC_CLASSIFIER_POLICY_ENV_PREFIX,
                    "CONNECT_TIMEOUT_SECS",
                ),
                defaults.connect_timeout.as_secs(),
            )),
            request_timeout: Duration::from_secs(read_u64_with(
                &read,
                env_key(
                    SEMANTIC_CLASSIFIER_POLICY_ENV_PREFIX,
                    "REQUEST_TIMEOUT_SECS",
                ),
                defaults.request_timeout.as_secs(),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphBatchSemanticClassifierOutcome {
    pub batch: GraphBatch,
    pub raw_content: String,
    pub normalized_content: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub attempts: usize,
    pub changed_relations: usize,
}

#[derive(Debug)]
pub enum GraphBatchRequestError {
    UnsupportedProvider(LlmProvider),
    InvalidRequestFixture(serde_json::Error),
    InvalidRequestShape(String),
    RepairJudgeUnavailable(String),
    SemanticClassifierUnavailable(String),
    RepairJudgeFailed {
        primary_attempts: usize,
        primary_error: String,
        repair_error: String,
    },
    Transport {
        attempts: usize,
        message: String,
    },
    InvalidBatch {
        attempts: usize,
        error: String,
        raw_content: String,
        normalized_content: String,
    },
    InvalidSemanticClassifierResponse {
        attempts: usize,
        error: String,
        raw_content: String,
        normalized_content: String,
    },
    SemanticClassifierTransport {
        attempts: usize,
        message: String,
    },
}

impl fmt::Display for GraphBatchRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProvider(provider) => write!(
                f,
                "GraphBatch LLM request requires an OpenAI-compatible provider, got {provider:?}"
            ),
            Self::InvalidRequestFixture(error) => {
                write!(f, "invalid GraphBatch request fixture: {error}")
            }
            Self::InvalidRequestShape(message) => f.write_str(message),
            Self::RepairJudgeUnavailable(message) => f.write_str(message),
            Self::SemanticClassifierUnavailable(message) => f.write_str(message),
            Self::RepairJudgeFailed {
                primary_attempts,
                primary_error,
                repair_error,
            } => write!(
                f,
                "GraphBatch repair judge failed after primary invalid output ({primary_attempts} primary attempt(s)): primary={primary_error}; repair={repair_error}"
            ),
            Self::Transport { attempts, message } => write!(
                f,
                "GraphBatch request failed after {attempts} attempt(s): {message}"
            ),
            Self::InvalidBatch {
                attempts,
                error,
                raw_content,
                normalized_content,
            } => write!(
                f,
                "GraphBatch response stayed invalid after {attempts} attempt(s): {error}\nraw={raw_content}\nnormalized={normalized_content}"
            ),
            Self::InvalidSemanticClassifierResponse {
                attempts,
                error,
                raw_content,
                normalized_content,
            } => write!(
                f,
                "semantic-class classifier response stayed invalid after {attempts} attempt(s): {error}\nraw={raw_content}\nnormalized={normalized_content}"
            ),
            Self::SemanticClassifierTransport { attempts, message } => write!(
                f,
                "semantic-class classifier failed after {attempts} attempt(s): {message}"
            ),
        }
    }
}

impl Error for GraphBatchRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidRequestFixture(error) => Some(error),
            _ => None,
        }
    }
}

pub fn build_graph_batch_request_body(
    config: &LlmEvaluatorConfig,
    request_fixture: &str,
) -> Result<Value, GraphBatchRequestError> {
    let mut body: Value = serde_json::from_str(request_fixture)
        .map_err(GraphBatchRequestError::InvalidRequestFixture)?;

    let Some(object) = body.as_object_mut() else {
        return Err(GraphBatchRequestError::InvalidRequestShape(
            "GraphBatch request fixture must be a JSON object".to_string(),
        ));
    };

    object.insert("model".to_string(), json!(config.model));
    object.insert("temperature".to_string(), json!(config.temperature));
    let fixture_max_tokens = object
        .get("max_tokens")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default();
    object.insert(
        "max_tokens".to_string(),
        json!(
            config
                .max_tokens
                .max(fixture_max_tokens)
                .max(MIN_GRAPH_BATCH_MAX_TOKENS)
        ),
    );

    let disable_thinking = std::env::var("LLM_ENABLE_THINKING")
        .map(|value| value == "false" || value == "0")
        .unwrap_or(false);

    if disable_thinking {
        object.insert(
            "chat_template_kwargs".to_string(),
            json!({"enable_thinking": false}),
        );
        object.remove("thinking_token_budget");
    } else {
        object.insert(
            "thinking_token_budget".to_string(),
            json!(THINKING_TOKEN_BUDGET),
        );
        object.remove("chat_template_kwargs");
    }

    Ok(body)
}

pub async fn request_graph_batch_with_retry(
    config: &LlmEvaluatorConfig,
    request_body: Value,
    subject_prefix: &str,
    run_id: &str,
) -> Result<GraphBatchRequestOutcome, GraphBatchRequestError> {
    request_graph_batch_with_policy(
        config,
        request_body,
        subject_prefix,
        run_id,
        GraphBatchRetryPolicy::default(),
    )
    .await
}

pub async fn request_graph_batch_with_repair_judge(
    config: &LlmEvaluatorConfig,
    request_body: Value,
    subject_prefix: &str,
    run_id: &str,
    primary_policy: GraphBatchRetryPolicy,
    repair_policy: GraphBatchRepairJudgePolicy,
) -> Result<GraphBatchRequestOutcome, GraphBatchRequestError> {
    let original_request_body = request_body.clone();

    match request_graph_batch_with_policy(
        config,
        request_body,
        subject_prefix,
        run_id,
        primary_policy,
    )
    .await
    {
        Ok(outcome) => Ok(outcome),
        Err(GraphBatchRequestError::InvalidBatch {
            attempts,
            error,
            raw_content,
            normalized_content,
        }) if repair_policy.max_attempts > 0 => {
            let repair_config = build_repair_judge_config(config)?;
            let repair_request_body = build_repair_judge_request_body(
                &repair_config,
                &original_request_body,
                &raw_content,
                &normalized_content,
                &error,
            )?;

            match request_graph_batch_with_policy(
                &repair_config,
                repair_request_body,
                subject_prefix,
                run_id,
                GraphBatchRetryPolicy {
                    max_attempts: repair_policy.max_attempts,
                    connect_timeout: repair_policy.connect_timeout,
                    request_timeout: repair_policy.request_timeout,
                },
            )
            .await
            {
                Ok(mut outcome) => {
                    outcome.repaired_by_judge = true;
                    outcome.primary_attempts = attempts;
                    outcome.repair_attempts = outcome.attempts;
                    outcome.attempts += attempts;
                    Ok(outcome)
                }
                Err(repair_error) => Err(GraphBatchRequestError::RepairJudgeFailed {
                    primary_attempts: attempts,
                    primary_error: error,
                    repair_error: repair_error.to_string(),
                }),
            }
        }
        Err(error) => Err(error),
    }
}

pub async fn classify_graph_batch_semantic_classes_with_policy(
    config: &LlmEvaluatorConfig,
    batch: &GraphBatch,
    subject_prefix: &str,
    run_id: &str,
    policy: GraphBatchSemanticClassifierPolicy,
) -> Result<GraphBatchSemanticClassifierOutcome, GraphBatchRequestError> {
    if batch.relations.is_empty() {
        return Ok(GraphBatchSemanticClassifierOutcome {
            batch: batch.clone(),
            raw_content: String::new(),
            normalized_content: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            attempts: 0,
            changed_relations: 0,
        });
    }

    match config.semantic_classifier_mode {
        LlmSemanticClassifierMode::ChatCompletions => {
            classify_graph_batch_semantic_classes_with_chat_api(
                config,
                batch,
                subject_prefix,
                run_id,
                policy,
            )
            .await
        }
        LlmSemanticClassifierMode::Score => {
            classify_graph_batch_semantic_classes_with_score_api(
                config,
                batch,
                subject_prefix,
                run_id,
                policy,
            )
            .await
        }
    }
}

async fn classify_graph_batch_semantic_classes_with_chat_api(
    config: &LlmEvaluatorConfig,
    batch: &GraphBatch,
    subject_prefix: &str,
    run_id: &str,
    policy: GraphBatchSemanticClassifierPolicy,
) -> Result<GraphBatchSemanticClassifierOutcome, GraphBatchRequestError> {
    let classifier_config = build_semantic_classifier_config(config)?;
    let client =
        build_http_client_with_connect_timeout(&classifier_config, Some(policy.connect_timeout))
            .map_err(
                |error| GraphBatchRequestError::SemanticClassifierTransport {
                    attempts: 0,
                    message: error.to_string(),
                },
            )?;
    let prompt = build_semantic_classifier_prompt(batch);
    let max_tokens = semantic_classifier_max_tokens(batch);
    let max_attempts = policy.max_attempts.max(1);

    for attempt in 1..=max_attempts {
        let response = tokio::time::timeout(
            policy.request_timeout,
            call_llm(
                &client,
                &classifier_config.endpoint,
                &classifier_config.model,
                classifier_config.provider,
                classifier_config.api_key.as_deref(),
                &prompt,
                max_tokens,
                classifier_config.temperature,
            ),
        )
        .await;

        match response {
            Ok(Ok((raw_content, prompt_tokens, completion_tokens))) => {
                let normalized_content = normalize_llm_json_response(&raw_content);
                match apply_semantic_classifications(
                    batch,
                    &normalized_content,
                    subject_prefix,
                    run_id,
                ) {
                    Ok((classified_batch, changed_relations)) => {
                        return Ok(GraphBatchSemanticClassifierOutcome {
                            batch: classified_batch,
                            raw_content,
                            normalized_content,
                            prompt_tokens,
                            completion_tokens,
                            attempts: attempt,
                            changed_relations,
                        });
                    }
                    Err(_error) if attempt < max_attempts => {
                        sleep(retry_delay_for_attempt(attempt + 1)).await;
                    }
                    Err(error) => {
                        return Err(GraphBatchRequestError::InvalidSemanticClassifierResponse {
                            attempts: attempt,
                            error,
                            raw_content,
                            normalized_content,
                        });
                    }
                }
            }
            Ok(Err(error)) if attempt < max_attempts => {
                let _ = error;
                sleep(retry_delay_for_attempt(attempt + 1)).await;
            }
            Ok(Err(error)) => {
                return Err(GraphBatchRequestError::SemanticClassifierTransport {
                    attempts: attempt,
                    message: error.to_string(),
                });
            }
            Err(_) if attempt < max_attempts => {
                sleep(retry_delay_for_attempt(attempt + 1)).await;
            }
            Err(_) => {
                return Err(GraphBatchRequestError::SemanticClassifierTransport {
                    attempts: attempt,
                    message: format!(
                        "semantic-class classifier timed out after {:?}",
                        policy.request_timeout
                    ),
                });
            }
        }
    }

    unreachable!("max_attempts is clamped to at least one")
}

async fn classify_graph_batch_semantic_classes_with_score_api(
    config: &LlmEvaluatorConfig,
    batch: &GraphBatch,
    subject_prefix: &str,
    run_id: &str,
    policy: GraphBatchSemanticClassifierPolicy,
) -> Result<GraphBatchSemanticClassifierOutcome, GraphBatchRequestError> {
    let classifier_config = build_semantic_classifier_config(config)?;
    let client =
        build_http_client_with_connect_timeout(&classifier_config, Some(policy.connect_timeout))
            .map_err(
                |error| GraphBatchRequestError::SemanticClassifierTransport {
                    attempts: 0,
                    message: error.to_string(),
                },
            )?;
    let max_attempts = policy.max_attempts.max(1);

    for attempt in 1..=max_attempts {
        let mut prompt_tokens = 0_u32;
        let mut completion_tokens = 0_u32;
        let mut raw_responses = Vec::with_capacity(batch.relations.len());
        let mut classifications = Vec::with_capacity(batch.relations.len());
        let mut failure = None;

        for relation_index in 0..batch.relations.len() {
            let query = build_semantic_classifier_score_query(batch, relation_index);
            let response = tokio::time::timeout(
                policy.request_timeout,
                send_semantic_classifier_score_request(
                    &client,
                    &classifier_config,
                    &query,
                    policy.request_timeout,
                ),
            )
            .await;

            match response {
                Ok(Ok((raw_content, parsed))) => {
                    prompt_tokens = prompt_tokens.saturating_add(parsed.usage.prompt_tokens);
                    completion_tokens =
                        completion_tokens.saturating_add(parsed.usage.completion_tokens);
                    raw_responses.push(
                        serde_json::from_str::<Value>(&raw_content)
                            .unwrap_or_else(|_| json!(raw_content)),
                    );
                    match best_semantic_class_from_score_response(&parsed, relation_index) {
                        Ok(semantic_class) => classifications.push(SemanticClassifierRelation {
                            index: relation_index,
                            semantic_class,
                        }),
                        Err(error) => {
                            failure =
                                Some(GraphBatchRequestError::InvalidSemanticClassifierResponse {
                                    attempts: attempt,
                                    error,
                                    raw_content: serde_json::to_string(&raw_responses)
                                        .unwrap_or_else(|_| "[]".to_string()),
                                    normalized_content: String::new(),
                                });
                            break;
                        }
                    }
                }
                Ok(Err(SendError::Retryable(message))) if attempt < max_attempts => {
                    failure = Some(GraphBatchRequestError::SemanticClassifierTransport {
                        attempts: attempt,
                        message,
                    });
                    break;
                }
                Ok(Err(SendError::Retryable(message))) | Ok(Err(SendError::Fatal(message))) => {
                    return Err(GraphBatchRequestError::SemanticClassifierTransport {
                        attempts: attempt,
                        message,
                    });
                }
                Err(_) if attempt < max_attempts => {
                    failure = Some(GraphBatchRequestError::SemanticClassifierTransport {
                        attempts: attempt,
                        message: format!(
                            "semantic-class classifier timed out after {:?}",
                            policy.request_timeout
                        ),
                    });
                    break;
                }
                Err(_) => {
                    return Err(GraphBatchRequestError::SemanticClassifierTransport {
                        attempts: attempt,
                        message: format!(
                            "semantic-class classifier timed out after {:?}",
                            policy.request_timeout
                        ),
                    });
                }
            }
        }

        if let Some(error) = failure {
            if attempt < max_attempts {
                sleep(retry_delay_for_attempt(attempt + 1)).await;
                continue;
            }
            return Err(error);
        }

        let normalized_content = serde_json::to_string(&SemanticClassifierResponse {
            relations: classifications,
        })
        .unwrap_or_else(|_| "{\"relations\":[]}".to_string());
        let raw_content =
            serde_json::to_string(&raw_responses).unwrap_or_else(|_| "[]".to_string());
        let (classified_batch, changed_relations) =
            apply_semantic_classifications(batch, &normalized_content, subject_prefix, run_id)
                .map_err(
                    |error| GraphBatchRequestError::InvalidSemanticClassifierResponse {
                        attempts: attempt,
                        error,
                        raw_content: raw_content.clone(),
                        normalized_content: normalized_content.clone(),
                    },
                )?;

        return Ok(GraphBatchSemanticClassifierOutcome {
            batch: classified_batch,
            raw_content,
            normalized_content,
            prompt_tokens,
            completion_tokens,
            attempts: attempt,
            changed_relations,
        });
    }

    unreachable!("max_attempts is clamped to at least one")
}

pub async fn request_graph_batch_with_policy(
    config: &LlmEvaluatorConfig,
    request_body: Value,
    subject_prefix: &str,
    run_id: &str,
    policy: GraphBatchRetryPolicy,
) -> Result<GraphBatchRequestOutcome, GraphBatchRequestError> {
    if !matches!(
        config.provider,
        LlmProvider::OpenAI | LlmProvider::OpenAINew
    ) {
        return Err(GraphBatchRequestError::UnsupportedProvider(config.provider));
    }

    let client = build_http_client_with_connect_timeout(config, Some(policy.connect_timeout))
        .map_err(|error| GraphBatchRequestError::Transport {
            attempts: 0,
            message: error.to_string(),
        })?;

    let mut request_body = request_body;
    let max_attempts = policy.max_attempts.max(1);

    for attempt in 1..=max_attempts {
        match send_openai_graph_batch_request(
            &client,
            config,
            &request_body,
            policy.request_timeout,
        )
        .await
        {
            Ok((raw_content, prompt_tokens, completion_tokens)) => {
                let normalized_content = normalize_llm_json_response(&raw_content);
                match validate_graph_batch_response(&normalized_content, subject_prefix, run_id) {
                    Ok(batch) => {
                        return Ok(GraphBatchRequestOutcome {
                            batch,
                            raw_content,
                            normalized_content,
                            prompt_tokens,
                            completion_tokens,
                            attempts: attempt,
                            primary_attempts: attempt,
                            repair_attempts: 0,
                            repaired_by_judge: false,
                        });
                    }
                    Err(error) if attempt < max_attempts => {
                        append_validation_feedback(&mut request_body, &error)?;
                        sleep(retry_delay_for_attempt(attempt + 1)).await;
                    }
                    Err(error) => {
                        return Err(GraphBatchRequestError::InvalidBatch {
                            attempts: attempt,
                            error,
                            raw_content,
                            normalized_content,
                        });
                    }
                }
            }
            Err(SendError::Retryable(_message)) if attempt < max_attempts => {
                sleep(retry_delay_for_attempt(attempt + 1)).await;
                continue;
            }
            Err(SendError::Retryable(message)) | Err(SendError::Fatal(message)) => {
                return Err(GraphBatchRequestError::Transport {
                    attempts: attempt,
                    message,
                });
            }
        }
    }

    unreachable!("max_attempts is clamped to at least one")
}

fn validate_graph_batch_response(
    normalized_content: &str,
    subject_prefix: &str,
    run_id: &str,
) -> Result<GraphBatch, String> {
    let batch = parse_llm_graph_batch(normalized_content)
        .map_err(|error| format_graph_batch_error("parse", &error))?;
    llm_graph_to_projection_events(&batch, subject_prefix, run_id)
        .map_err(|error| format_graph_batch_error("translation", &error))?;
    Ok(batch)
}

fn format_graph_batch_error(stage: &str, error: &LlmGraphError) -> String {
    format!("{stage} failed: {error}")
}

fn build_repair_judge_config(
    config: &LlmEvaluatorConfig,
) -> Result<LlmEvaluatorConfig, GraphBatchRequestError> {
    let judge_endpoint = config.judge_endpoint.clone().ok_or_else(|| {
        GraphBatchRequestError::RepairJudgeUnavailable(
            "repair judge requires LLM_JUDGE_ENDPOINT".to_string(),
        )
    })?;
    let judge_model = config.judge_model.clone().ok_or_else(|| {
        GraphBatchRequestError::RepairJudgeUnavailable(
            "repair judge requires LLM_JUDGE_MODEL".to_string(),
        )
    })?;
    let judge_provider = config.judge_provider.ok_or_else(|| {
        GraphBatchRequestError::RepairJudgeUnavailable(
            "repair judge requires LLM_JUDGE_PROVIDER".to_string(),
        )
    })?;

    Ok(LlmEvaluatorConfig {
        endpoint: judge_endpoint,
        model: judge_model,
        provider: judge_provider,
        api_key: config
            .judge_api_key
            .clone()
            .or_else(|| config.api_key.clone()),
        max_tokens: config.max_tokens.max(MIN_GRAPH_BATCH_MAX_TOKENS),
        temperature: 0.0,
        tls_cert_path: config.tls_cert_path.clone(),
        tls_key_path: config.tls_key_path.clone(),
        tls_insecure: config.tls_insecure,
        judge_endpoint: None,
        judge_model: None,
        judge_provider: None,
        judge_api_key: None,
        semantic_classifier_endpoint: None,
        semantic_classifier_model: None,
        semantic_classifier_provider: None,
        semantic_classifier_api_key: None,
        semantic_classifier_mode: config.semantic_classifier_mode,
    })
}

fn build_semantic_classifier_config(
    config: &LlmEvaluatorConfig,
) -> Result<LlmEvaluatorConfig, GraphBatchRequestError> {
    let endpoint = config.semantic_classifier_endpoint.clone().ok_or_else(|| {
        GraphBatchRequestError::SemanticClassifierUnavailable(
            "semantic-class classifier requires LLM_SEMANTIC_CLASSIFIER_ENDPOINT".to_string(),
        )
    })?;
    let model = config.semantic_classifier_model.clone().ok_or_else(|| {
        GraphBatchRequestError::SemanticClassifierUnavailable(
            "semantic-class classifier requires LLM_SEMANTIC_CLASSIFIER_MODEL".to_string(),
        )
    })?;
    let provider = config.semantic_classifier_provider.ok_or_else(|| {
        GraphBatchRequestError::SemanticClassifierUnavailable(
            "semantic-class classifier requires LLM_SEMANTIC_CLASSIFIER_PROVIDER".to_string(),
        )
    })?;

    Ok(LlmEvaluatorConfig {
        endpoint,
        model,
        provider,
        api_key: config
            .semantic_classifier_api_key
            .clone()
            .or_else(|| config.api_key.clone()),
        max_tokens: SEMANTIC_CLASSIFIER_MAX_MAX_TOKENS,
        temperature: 0.0,
        tls_cert_path: config.tls_cert_path.clone(),
        tls_key_path: config.tls_key_path.clone(),
        tls_insecure: config.tls_insecure,
        judge_endpoint: None,
        judge_model: None,
        judge_provider: None,
        judge_api_key: None,
        semantic_classifier_endpoint: None,
        semantic_classifier_model: None,
        semantic_classifier_provider: None,
        semantic_classifier_api_key: None,
        semantic_classifier_mode: config.semantic_classifier_mode,
    })
}

fn build_semantic_classifier_prompt(batch: &GraphBatch) -> String {
    let nodes = batch
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let details = batch
        .node_details
        .iter()
        .map(|detail| (detail.node_id.as_str(), detail.detail.as_str()))
        .collect::<BTreeMap<_, _>>();
    let relations = batch
        .relations
        .iter()
        .enumerate()
        .map(|(index, relation)| {
            let source = nodes.get(relation.source_node_id.as_str());
            let target = nodes.get(relation.target_node_id.as_str());
            json!({
                "index": index,
                "source_node_id": relation.source_node_id,
                "source_node_kind": source.map(|node| node.node_kind.as_str()).unwrap_or(""),
                "source_title": source.map(|node| node.title.as_str()).unwrap_or(""),
                "source_summary": source.map(|node| node.summary.as_str()).unwrap_or(""),
                "target_node_id": relation.target_node_id,
                "target_node_kind": target.map(|node| node.node_kind.as_str()).unwrap_or(""),
                "target_title": target.map(|node| node.title.as_str()).unwrap_or(""),
                "target_summary": target.map(|node| node.summary.as_str()).unwrap_or(""),
                "target_detail_excerpt": details
                    .get(relation.target_node_id.as_str())
                    .map(|detail| semantic_classifier_excerpt(detail, 320)),
                "relation_type": relation.relation_type,
                "rationale": relation.rationale,
                "motivation": relation.motivation,
                "method": relation.method,
                "decision_id": relation.decision_id,
                "caused_by_node_id": relation.caused_by_node_id,
                "evidence": relation.evidence,
                "confidence": relation.confidence,
                "current_semantic_class": relation.semantic_class.as_str(),
            })
        })
        .collect::<Vec<_>>();
    let payload = json!({
        "root_node_id": batch.root_node_id,
        "relations": relations,
    });

    format!(
        "You are a semantic-class catalog agent for Rehydration Kernel graph edges.\n\
Return JSON only with this exact shape:\n\
{{\"relations\":[{{\"index\":0,\"semantic_class\":\"causal\"}}]}}\n\
Return exactly one entry per input relation index. Do not add or remove indices.\n\
Do not rewrite node ids, relation_type, rationale, motivation, method, evidence, or confidence.\n\
Only decide `semantic_class`.\n\n\
Rubric:\n\
- `causal`: actual cause/effect, confirmed finding explains the incident, wording like caused/triggered/led to/because, or `caused_by_node_id` is present.\n\
- `motivational`: explains why a decision or mitigation was selected, including tradeoffs.\n\
- `procedural`: step, method, runbook, ordered execution, or how-to sequence.\n\
- `evidential`: logs, metrics, traces, diffs, or inspection support a finding or decision.\n\
- `constraint`: rule, dependency, safety boundary, limit, or operational restriction.\n\
- `structural`: reference, containment, grouping, or neutral association without explanatory force.\n\n\
Prefer the strongest explanatory class over `structural`.\n\
If both `method` and `motivation` exist, classify by what the relation primarily communicates.\n\
If a confirmed finding explains the incident, prefer `causal` over `procedural`.\n\n\
Classify these relations:\n{}\n",
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
    )
}

fn build_semantic_classifier_score_query(batch: &GraphBatch, relation_index: usize) -> String {
    let relation = &batch.relations[relation_index];
    let nodes = batch
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let details = batch
        .node_details
        .iter()
        .map(|detail| (detail.node_id.as_str(), detail.detail.as_str()))
        .collect::<BTreeMap<_, _>>();
    let source = nodes.get(relation.source_node_id.as_str());
    let target = nodes.get(relation.target_node_id.as_str());

    format!(
        "Classify this Rehydration Kernel relation into the best semantic_class.\n\
source_kind={}\n\
source_title={}\n\
source_summary={}\n\
target_kind={}\n\
target_title={}\n\
target_summary={}\n\
target_detail_excerpt={}\n\
relation_type={}\n\
rationale={}\n\
motivation={}\n\
method={}\n\
evidence={}\n\
decision_id={}\n\
caused_by_node_id={}\n\
confidence={}\n\
current_semantic_class={}",
        source.map(|node| node.node_kind.as_str()).unwrap_or(""),
        source.map(|node| node.title.as_str()).unwrap_or(""),
        source.map(|node| node.summary.as_str()).unwrap_or(""),
        target.map(|node| node.node_kind.as_str()).unwrap_or(""),
        target.map(|node| node.title.as_str()).unwrap_or(""),
        target.map(|node| node.summary.as_str()).unwrap_or(""),
        details
            .get(relation.target_node_id.as_str())
            .map(|detail| semantic_classifier_excerpt(detail, 320))
            .unwrap_or_default(),
        relation.relation_type,
        relation.rationale.as_deref().unwrap_or(""),
        relation.motivation.as_deref().unwrap_or(""),
        relation.method.as_deref().unwrap_or(""),
        relation.evidence.as_deref().unwrap_or(""),
        relation.decision_id.as_deref().unwrap_or(""),
        relation.caused_by_node_id.as_deref().unwrap_or(""),
        relation.confidence.as_deref().unwrap_or(""),
        relation.semantic_class.as_str(),
    )
}

fn semantic_classifier_score_documents() -> Vec<&'static str> {
    SEMANTIC_CLASS_SCORE_DOCUMENTS
        .iter()
        .map(|(_, document)| *document)
        .collect()
}

fn best_semantic_class_from_score_response(
    response: &SemanticClassifierScoreResponse,
    relation_index: usize,
) -> Result<RelationSemanticClass, String> {
    if response.data.len() != SEMANTIC_CLASS_SCORE_DOCUMENTS.len() {
        return Err(format!(
            "semantic-class score response for relation index {} returned {} score(s), expected {}",
            relation_index,
            response.data.len(),
            SEMANTIC_CLASS_SCORE_DOCUMENTS.len()
        ));
    }

    let mut seen = [false; SEMANTIC_CLASS_SCORE_DOCUMENTS.len()];
    for score in &response.data {
        if score.index >= SEMANTIC_CLASS_SCORE_DOCUMENTS.len() {
            return Err(format!(
                "semantic-class score response for relation index {} returned out-of-range document index {}",
                relation_index, score.index
            ));
        }
        if std::mem::replace(&mut seen[score.index], true) {
            return Err(format!(
                "semantic-class score response for relation index {} returned duplicate document index {}",
                relation_index, score.index
            ));
        }
    }

    let best = response
        .data
        .iter()
        .max_by(|left, right| {
            left.score
                .total_cmp(&right.score)
                .then_with(|| right.index.cmp(&left.index))
        })
        .ok_or_else(|| {
            format!(
                "semantic-class score response for relation index {} was empty",
                relation_index
            )
        })?;

    Ok(SEMANTIC_CLASS_SCORE_DOCUMENTS[best.index].0)
}

fn semantic_classifier_max_tokens(batch: &GraphBatch) -> u32 {
    let estimated = 64_u32.saturating_add((batch.relations.len() as u32).saturating_mul(24));
    estimated.clamp(
        SEMANTIC_CLASSIFIER_MIN_MAX_TOKENS,
        SEMANTIC_CLASSIFIER_MAX_MAX_TOKENS,
    )
}

fn semantic_classifier_excerpt(value: &str, max_chars: usize) -> String {
    let total = value.chars().count();
    if total <= max_chars {
        return value.to_string();
    }

    let head = value.chars().take(max_chars).collect::<String>();
    format!("{head}...")
}

fn apply_semantic_classifications(
    batch: &GraphBatch,
    normalized_content: &str,
    subject_prefix: &str,
    run_id: &str,
) -> Result<(GraphBatch, usize), String> {
    let response: SemanticClassifierResponse =
        serde_json::from_str(normalized_content).map_err(|error| {
            format!("parse failed: invalid semantic-class classifier JSON: {error}")
        })?;

    if response.relations.len() != batch.relations.len() {
        return Err(format!(
            "semantic-class classifier returned {} relation classification(s), expected {}",
            response.relations.len(),
            batch.relations.len()
        ));
    }

    let mut rewritten = batch.clone();
    let mut seen = BTreeSet::new();
    let mut changed_relations = 0;

    for relation in response.relations {
        if relation.index >= rewritten.relations.len() {
            return Err(format!(
                "semantic-class classifier returned out-of-range relation index {}",
                relation.index
            ));
        }
        if !seen.insert(relation.index) {
            return Err(format!(
                "semantic-class classifier returned duplicate classification for relation index {}",
                relation.index
            ));
        }

        let current = &mut rewritten.relations[relation.index];
        if current.semantic_class != relation.semantic_class {
            current.semantic_class = relation.semantic_class;
            changed_relations += 1;
        }
    }

    llm_graph_to_projection_events(&rewritten, subject_prefix, run_id)
        .map_err(|error| format_graph_batch_error("semantic classification translation", &error))?;

    Ok((rewritten, changed_relations))
}

#[derive(Debug, Deserialize, Serialize)]
struct SemanticClassifierResponse {
    relations: Vec<SemanticClassifierRelation>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SemanticClassifierRelation {
    index: usize,
    semantic_class: kmp_domain::RelationSemanticClass,
}

#[derive(Debug, Deserialize)]
struct SemanticClassifierScoreResponse {
    data: Vec<SemanticClassifierScore>,
    usage: SemanticClassifierScoreUsage,
}

#[derive(Debug, Deserialize)]
struct SemanticClassifierScore {
    index: usize,
    score: f64,
}

#[derive(Debug, Deserialize)]
struct SemanticClassifierScoreUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

fn build_repair_judge_request_body(
    repair_config: &LlmEvaluatorConfig,
    original_request_body: &Value,
    raw_content: &str,
    normalized_content: &str,
    error: &str,
) -> Result<Value, GraphBatchRequestError> {
    let mut request_body = original_request_body.clone();
    let Some(object) = request_body.as_object_mut() else {
        return Err(GraphBatchRequestError::InvalidRequestShape(
            "GraphBatch repair request body must be a JSON object".to_string(),
        ));
    };

    object.insert("model".to_string(), json!(repair_config.model));
    object.insert("temperature".to_string(), json!(repair_config.temperature));
    object.insert(
        "max_tokens".to_string(),
        json!(repair_config.max_tokens.max(MIN_GRAPH_BATCH_MAX_TOKENS)),
    );

    let invalid_payload = if normalized_content.trim().is_empty() {
        raw_content
    } else {
        normalized_content
    };

    append_validation_feedback(
        &mut request_body,
        &format!(
            "{error}\nPrevious invalid response:\n{invalid_payload}\nYou are acting as a repair judge. Correct only the GraphBatch contract violations. Do not invent new facts or widen the graph beyond the original request."
        ),
    )?;

    Ok(request_body)
}

fn append_validation_feedback(
    request_body: &mut Value,
    error: &str,
) -> Result<(), GraphBatchRequestError> {
    let Some(messages) = request_body
        .get_mut("messages")
        .and_then(Value::as_array_mut)
    else {
        return Err(GraphBatchRequestError::InvalidRequestShape(
            "GraphBatch request body must contain a `messages` array".to_string(),
        ));
    };

    messages.push(json!({
        "role": "user",
        "content": format!(
            "Your previous response did not satisfy the GraphBatch contract.\nValidation error: {error}\nReturn corrected JSON only.\nKeep the same root_node_id and preserve any fields that were already valid."
        )
    }));

    Ok(())
}

fn retry_delay_for_attempt(attempt: usize) -> Duration {
    let index = attempt
        .saturating_sub(1)
        .min(RETRY_DELAY_SCHEDULE_MS.len().saturating_sub(1));
    Duration::from_millis(RETRY_DELAY_SCHEDULE_MS[index])
}

fn env_key(prefix: &str, field: &str) -> String {
    format!("{prefix}_{field}")
}

fn read_usize_with<F>(read: &F, key: String, default: usize) -> usize
where
    F: Fn(&str) -> Option<String>,
{
    read(&key)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn read_u64_with<F>(read: &F, key: String, default: u64) -> u64
where
    F: Fn(&str) -> Option<String>,
{
    read(&key)
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

async fn send_openai_graph_batch_request(
    client: &reqwest::Client,
    config: &LlmEvaluatorConfig,
    body: &Value,
    request_timeout: Duration,
) -> Result<(String, u32, u32), SendError> {
    let mut request = client
        .post(&config.endpoint)
        .timeout(request_timeout)
        .json(body);

    if let Some(key) = config.api_key.as_deref() {
        request = request.bearer_auth(key);
    }

    let response = request.send().await.map_err(classify_reqwest_error)?;
    let status = response.status();
    let response_text = response.text().await.map_err(classify_reqwest_error)?;

    if !status.is_success() {
        let message = format!("LLM request failed with {status}: {response_text}");
        return Err(if is_retryable_http_status(status) {
            SendError::Retryable(message)
        } else {
            SendError::Fatal(message)
        });
    }

    let chat: OpenAiChatResponse = serde_json::from_str(&response_text).map_err(|error| {
        SendError::Fatal(format!(
            "failed to parse LLM chat response: {error}; body={response_text}"
        ))
    })?;
    let content = chat
        .choices
        .first()
        .and_then(|choice| choice.message.content.clone())
        .unwrap_or_default();
    let usage = chat.usage.unwrap_or(OpenAiUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
    });

    Ok((content, usage.prompt_tokens, usage.completion_tokens))
}

async fn send_semantic_classifier_score_request(
    client: &reqwest::Client,
    config: &LlmEvaluatorConfig,
    query: &str,
    request_timeout: Duration,
) -> Result<(String, SemanticClassifierScoreResponse), SendError> {
    let body = json!({
        "model": config.model,
        "queries": query,
        "documents": semantic_classifier_score_documents(),
    });

    let mut request = client
        .post(&config.endpoint)
        .timeout(request_timeout)
        .json(&body);

    if let Some(key) = config.api_key.as_deref() {
        request = request.bearer_auth(key);
    }

    let response = request.send().await.map_err(classify_reqwest_error)?;
    let status = response.status();
    let response_text = response.text().await.map_err(classify_reqwest_error)?;

    if !status.is_success() {
        let message = format!("semantic-class score request failed with {status}: {response_text}");
        return Err(if is_retryable_http_status(status) {
            SendError::Retryable(message)
        } else {
            SendError::Fatal(message)
        });
    }

    let parsed: SemanticClassifierScoreResponse =
        serde_json::from_str(&response_text).map_err(|error| {
            SendError::Fatal(format!(
                "failed to parse semantic-class score response: {error}; body={response_text}"
            ))
        })?;

    Ok((response_text, parsed))
}

fn is_retryable_http_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT
    )
}

fn classify_reqwest_error(error: reqwest::Error) -> SendError {
    let message = error.to_string();
    if error.is_timeout() || error.is_connect() {
        SendError::Retryable(message)
    } else {
        SendError::Fatal(message)
    }
}

#[derive(Debug)]
enum SendError {
    Retryable(String),
    Fatal(String),
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn sample_config() -> LlmEvaluatorConfig {
        LlmEvaluatorConfig {
            endpoint: "https://llm.example.test/v1/chat/completions".to_string(),
            model: "Qwen/Qwen3.5-9B".to_string(),
            provider: LlmProvider::OpenAI,
            api_key: None,
            max_tokens: 1024,
            temperature: 0.0,
            tls_cert_path: None,
            tls_key_path: None,
            tls_insecure: false,
            judge_endpoint: None,
            judge_model: None,
            judge_provider: None,
            judge_api_key: None,
            semantic_classifier_endpoint: None,
            semantic_classifier_model: None,
            semantic_classifier_provider: None,
            semantic_classifier_api_key: None,
            semantic_classifier_mode: LlmSemanticClassifierMode::ChatCompletions,
        }
    }

    #[test]
    fn retry_policy_defaults_match_documented_values() {
        let policy = GraphBatchRetryPolicy::default();
        assert_eq!(policy.max_attempts, PRIMARY_DEFAULT_MAX_ATTEMPTS);
        assert_eq!(
            policy.connect_timeout,
            Duration::from_secs(PRIMARY_DEFAULT_CONNECT_TIMEOUT_SECS)
        );
        assert_eq!(
            policy.request_timeout,
            Duration::from_secs(PRIMARY_DEFAULT_REQUEST_TIMEOUT_SECS)
        );
        assert_eq!(retry_delay_for_attempt(1), Duration::from_millis(0));
        assert_eq!(retry_delay_for_attempt(2), Duration::from_millis(250));
        assert_eq!(retry_delay_for_attempt(3), Duration::from_secs(1));
        assert_eq!(retry_delay_for_attempt(4), Duration::from_secs(3));
        assert_eq!(retry_delay_for_attempt(8), Duration::from_secs(3));

        let repair_policy = GraphBatchRepairJudgePolicy::default();
        assert_eq!(repair_policy.max_attempts, REPAIR_DEFAULT_MAX_ATTEMPTS);
        assert_eq!(
            repair_policy.connect_timeout,
            Duration::from_secs(REPAIR_DEFAULT_CONNECT_TIMEOUT_SECS)
        );
        assert_eq!(
            repair_policy.request_timeout,
            Duration::from_secs(REPAIR_DEFAULT_REQUEST_TIMEOUT_SECS)
        );

        let semantic_policy = GraphBatchSemanticClassifierPolicy::default();
        assert_eq!(
            semantic_policy.max_attempts,
            SEMANTIC_CLASSIFIER_DEFAULT_MAX_ATTEMPTS
        );
        assert_eq!(
            semantic_policy.connect_timeout,
            Duration::from_secs(SEMANTIC_CLASSIFIER_DEFAULT_CONNECT_TIMEOUT_SECS)
        );
        assert_eq!(
            semantic_policy.request_timeout,
            Duration::from_secs(SEMANTIC_CLASSIFIER_DEFAULT_REQUEST_TIMEOUT_SECS)
        );
    }

    #[test]
    fn retry_policies_can_be_overridden_independently_via_env() {
        let overrides = std::collections::BTreeMap::from([
            (
                "LLM_GRAPH_BATCH_PRIMARY_MAX_ATTEMPTS".to_string(),
                "2".to_string(),
            ),
            (
                "LLM_GRAPH_BATCH_PRIMARY_CONNECT_TIMEOUT_SECS".to_string(),
                "5".to_string(),
            ),
            (
                "LLM_GRAPH_BATCH_PRIMARY_REQUEST_TIMEOUT_SECS".to_string(),
                "75".to_string(),
            ),
            (
                "LLM_GRAPH_BATCH_REPAIR_MAX_ATTEMPTS".to_string(),
                "3".to_string(),
            ),
            (
                "LLM_GRAPH_BATCH_REPAIR_CONNECT_TIMEOUT_SECS".to_string(),
                "7".to_string(),
            ),
            (
                "LLM_GRAPH_BATCH_REPAIR_REQUEST_TIMEOUT_SECS".to_string(),
                "240".to_string(),
            ),
            (
                "LLM_GRAPH_BATCH_SEMANTIC_CLASSIFIER_MAX_ATTEMPTS".to_string(),
                "2".to_string(),
            ),
            (
                "LLM_GRAPH_BATCH_SEMANTIC_CLASSIFIER_CONNECT_TIMEOUT_SECS".to_string(),
                "1".to_string(),
            ),
            (
                "LLM_GRAPH_BATCH_SEMANTIC_CLASSIFIER_REQUEST_TIMEOUT_SECS".to_string(),
                "12".to_string(),
            ),
        ]);

        let primary = GraphBatchRetryPolicy::from_reader(|key| overrides.get(key).cloned());
        let repair = GraphBatchRepairJudgePolicy::from_reader(|key| overrides.get(key).cloned());
        let semantic =
            GraphBatchSemanticClassifierPolicy::from_reader(|key| overrides.get(key).cloned());

        assert_eq!(primary.max_attempts, 2);
        assert_eq!(primary.connect_timeout, Duration::from_secs(5));
        assert_eq!(primary.request_timeout, Duration::from_secs(75));

        assert_eq!(repair.max_attempts, 3);
        assert_eq!(repair.connect_timeout, Duration::from_secs(7));
        assert_eq!(repair.request_timeout, Duration::from_secs(240));

        assert_eq!(semantic.max_attempts, 2);
        assert_eq!(semantic.connect_timeout, Duration::from_secs(1));
        assert_eq!(semantic.request_timeout, Duration::from_secs(12));
    }

    #[test]
    fn build_request_body_applies_runtime_model_settings() {
        let config = sample_config();
        let request = build_graph_batch_request_body(
            &config,
            r#"{"model":"old","temperature":0.7,"max_tokens":64,"messages":[]}"#,
        )
        .expect("request body should build");

        assert_eq!(request["model"], json!("Qwen/Qwen3.5-9B"));
        assert_eq!(request["temperature"], json!(0.0));
        assert_eq!(request["max_tokens"], json!(2048));
        assert_eq!(request["thinking_token_budget"], json!(512));
        assert!(request.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn build_request_body_preserves_large_fixture_max_tokens() {
        let config = sample_config();
        let request = build_graph_batch_request_body(
            &config,
            r#"{"model":"old","temperature":0.7,"max_tokens":8192,"messages":[]}"#,
        )
        .expect("request body should build");

        assert_eq!(request["max_tokens"], json!(8192));
    }

    #[test]
    fn append_validation_feedback_adds_repair_message() {
        let mut request = json!({
            "messages": [
                {"role": "system", "content": "Return JSON only."},
                {"role": "user", "content": "Extract a graph."}
            ]
        });

        append_validation_feedback(&mut request, "parse failed: missing root node")
            .expect("feedback should append");

        let messages = request["messages"]
            .as_array()
            .expect("messages should remain an array");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2]["role"], json!("user"));
        let repair_text = messages[2]["content"]
            .as_str()
            .expect("repair message should be text");
        assert!(repair_text.contains("Validation error: parse failed: missing root node"));
        assert!(repair_text.contains("Return corrected JSON only."));
    }

    #[test]
    fn anthropic_provider_is_rejected_for_openai_graph_batch_request() {
        let mut config = sample_config();
        config.provider = LlmProvider::Anthropic;

        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime should build");
        let error = runtime
            .block_on(request_graph_batch_with_policy(
                &config,
                json!({"messages": []}),
                "rehydration",
                "test",
                GraphBatchRetryPolicy {
                    max_attempts: 1,
                    ..GraphBatchRetryPolicy::default()
                },
            ))
            .expect_err("anthropic should be rejected");

        assert!(matches!(
            error,
            GraphBatchRequestError::UnsupportedProvider(LlmProvider::Anthropic)
        ));
    }

    #[test]
    fn build_repair_judge_request_body_appends_invalid_payload_and_uses_judge_model() {
        let mut config = sample_config();
        config.judge_endpoint = Some("https://judge.example.test/v1/chat/completions".to_string());
        config.judge_model = Some("gpt-5.4".to_string());
        config.judge_provider = Some(LlmProvider::OpenAINew);

        let repair_config = build_repair_judge_config(&config).expect("judge config should build");
        let request_body = json!({
            "model": "Qwen/Qwen3.5-9B",
            "messages": [
                {"role": "system", "content": "Return JSON only."},
                {"role": "user", "content": "Extract a graph."}
            ]
        });

        let repaired = build_repair_judge_request_body(
            &repair_config,
            &request_body,
            "{\"bad\":true}",
            "{\"bad\":true}",
            "translation failed: nodes must contain at least the root node",
        )
        .expect("repair request should build");

        assert_eq!(repaired["model"], json!("gpt-5.4"));
        let messages = repaired["messages"]
            .as_array()
            .expect("repair request should keep messages");
        let repair_text = messages
            .last()
            .and_then(|message| message["content"].as_str())
            .expect("repair message should be present");
        assert!(repair_text.contains("You are acting as a repair judge."));
        assert!(repair_text.contains("Previous invalid response:"));
        assert!(repair_text.contains("{\"bad\":true}"));
    }

    #[test]
    fn repair_judge_requires_explicit_judge_config() {
        let error = build_repair_judge_config(&sample_config())
            .expect_err("repair judge should require explicit judge config");
        assert!(matches!(
            error,
            GraphBatchRequestError::RepairJudgeUnavailable(_)
        ));
    }

    #[test]
    fn semantic_classifier_requires_explicit_config() {
        let error = build_semantic_classifier_config(&sample_config())
            .expect_err("semantic classifier should require explicit config");
        assert!(matches!(
            error,
            GraphBatchRequestError::SemanticClassifierUnavailable(_)
        ));
    }

    #[tokio::test]
    async fn semantic_classifier_can_rewrite_relation_classes() {
        let classifier_response = openai_chat_response(
            r#"{
              "relations":[
                {"index":0,"semantic_class":"causal"},
                {"index":1,"semantic_class":"motivational"}
              ]
            }"#,
        );
        let classifier_endpoint = spawn_single_response_server(classifier_response).await;
        let mut config = sample_config();
        config.semantic_classifier_endpoint = Some(classifier_endpoint);
        config.semantic_classifier_model = Some("Qwen/Qwen3-0.6B".to_string());
        config.semantic_classifier_provider = Some(LlmProvider::OpenAI);
        let batch = parse_llm_graph_batch(
            r#"{
              "root_node_id":"incident-1",
              "nodes":[
                {
                  "node_id":"incident-1",
                  "node_kind":"incident",
                  "title":"Latency spike",
                  "summary":"Payments latency increased after rollout.",
                  "status":"INVESTIGATING",
                  "labels":["incident"],
                  "properties":{}
                },
                {
                  "node_id":"finding-1",
                  "node_kind":"finding",
                  "title":"DB pool reduced",
                  "summary":"maxConnections changed from 50 to 5.",
                  "status":"CONFIRMED",
                  "labels":["finding"],
                  "properties":{"service":"payments-api"}
                },
                {
                  "node_id":"decision-1",
                  "node_kind":"decision",
                  "title":"Reroute traffic",
                  "summary":"Shift 80% of traffic while rollback completes.",
                  "status":"APPROVED",
                  "labels":["decision"],
                  "properties":{"owner":"oncall"}
                }
              ],
              "relations":[
                {
                  "source_node_id":"incident-1",
                  "target_node_id":"finding-1",
                  "relation_type":"related_to",
                  "semantic_class":"procedural",
                  "rationale":"The DB pool change caused the latency spike.",
                  "caused_by_node_id":"finding-1",
                  "confidence":"high"
                },
                {
                  "source_node_id":"incident-1",
                  "target_node_id":"decision-1",
                  "relation_type":"related_to",
                  "semantic_class":"structural",
                  "motivation":"The secondary region was chosen to buy time for rollback.",
                  "confidence":"high"
                }
              ],
              "node_details":[
                {
                  "node_id":"finding-1",
                  "detail":"Config diff shows maxConnections changed from 50 to 5.",
                  "revision":1
                }
              ]
            }"#,
        )
        .expect("batch should parse");

        let outcome = classify_graph_batch_semantic_classes_with_policy(
            &config,
            &batch,
            "rehydration",
            "semantic-class-test",
            GraphBatchSemanticClassifierPolicy::default(),
        )
        .await
        .expect("semantic classifier should classify the relations");

        assert_eq!(outcome.attempts, 1);
        assert_eq!(outcome.changed_relations, 2);
        assert_eq!(
            outcome.batch.relations[0].semantic_class,
            kmp_domain::RelationSemanticClass::Causal
        );
        assert_eq!(
            outcome.batch.relations[1].semantic_class,
            kmp_domain::RelationSemanticClass::Motivational
        );
    }

    #[tokio::test]
    async fn semantic_classifier_score_api_can_rewrite_relation_classes() {
        let classifier_endpoint = spawn_response_server(vec![
            semantic_classifier_score_response(0),
            semantic_classifier_score_response(1),
        ])
        .await;
        let mut config = sample_config();
        config.semantic_classifier_endpoint = Some(classifier_endpoint);
        config.semantic_classifier_model = Some("Qwen/Qwen3-Reranker-0.6B".to_string());
        config.semantic_classifier_provider = Some(LlmProvider::OpenAI);
        config.semantic_classifier_mode = LlmSemanticClassifierMode::Score;
        let batch = parse_llm_graph_batch(
            r#"{
              "root_node_id":"incident-1",
              "nodes":[
                {
                  "node_id":"incident-1",
                  "node_kind":"incident",
                  "title":"Latency spike",
                  "summary":"Payments latency increased after rollout.",
                  "status":"INVESTIGATING",
                  "labels":["incident"],
                  "properties":{}
                },
                {
                  "node_id":"finding-1",
                  "node_kind":"finding",
                  "title":"DB pool reduced",
                  "summary":"maxConnections changed from 50 to 5.",
                  "status":"CONFIRMED",
                  "labels":["finding"],
                  "properties":{"service":"payments-api"}
                },
                {
                  "node_id":"decision-1",
                  "node_kind":"decision",
                  "title":"Reroute traffic",
                  "summary":"Shift 80% of traffic while rollback completes.",
                  "status":"APPROVED",
                  "labels":["decision"],
                  "properties":{"owner":"oncall"}
                }
              ],
              "relations":[
                {
                  "source_node_id":"incident-1",
                  "target_node_id":"finding-1",
                  "relation_type":"related_to",
                  "semantic_class":"procedural",
                  "rationale":"The DB pool change caused the latency spike.",
                  "caused_by_node_id":"finding-1",
                  "confidence":"high"
                },
                {
                  "source_node_id":"incident-1",
                  "target_node_id":"decision-1",
                  "relation_type":"related_to",
                  "semantic_class":"structural",
                  "motivation":"The secondary region was chosen to buy time for rollback.",
                  "confidence":"high"
                }
              ],
              "node_details":[
                {
                  "node_id":"finding-1",
                  "detail":"Config diff shows maxConnections changed from 50 to 5.",
                  "revision":1
                }
              ]
            }"#,
        )
        .expect("batch should parse");

        let outcome = classify_graph_batch_semantic_classes_with_policy(
            &config,
            &batch,
            "rehydration",
            "semantic-class-score-test",
            GraphBatchSemanticClassifierPolicy::default(),
        )
        .await
        .expect("semantic classifier score API should classify the relations");

        assert_eq!(outcome.attempts, 1);
        assert_eq!(outcome.changed_relations, 2);
        assert_eq!(
            outcome.batch.relations[0].semantic_class,
            kmp_domain::RelationSemanticClass::Causal
        );
        assert_eq!(
            outcome.batch.relations[1].semantic_class,
            kmp_domain::RelationSemanticClass::Motivational
        );
    }

    #[tokio::test]
    async fn repair_judge_can_salvage_an_invalid_primary_response() {
        let primary_response = openai_chat_response(
            r#"{"root_node_id":"incident-1","nodes":[],"relations":[],"node_details":[]}"#,
        );
        let repaired_response = openai_chat_response(
            r#"{
              "root_node_id":"incident-1",
              "nodes":[
                {
                  "node_id":"incident-1",
                  "node_kind":"incident",
                  "title":"Latency spike",
                  "summary":"Payments latency increased after rollout.",
                  "status":"INVESTIGATING",
                  "labels":["incident"],
                  "properties":{}
                },
                {
                  "node_id":"finding-1",
                  "node_kind":"finding",
                  "title":"DB pool reduced",
                  "summary":"maxConnections changed from 50 to 5.",
                  "status":"CONFIRMED",
                  "labels":["finding"],
                  "properties":{"service":"payments-api"}
                }
              ],
              "relations":[
                {
                  "source_node_id":"incident-1",
                  "target_node_id":"finding-1",
                  "relation_type":"CAUSED_BY",
                  "semantic_class":"causal",
                  "rationale":"The rollout reduced DB capacity.",
                  "confidence":"high"
                }
              ],
              "node_details":[
                {
                  "node_id":"finding-1",
                  "detail":"Config diff shows maxConnections changed from 50 to 5.",
                  "revision":1
                }
              ]
            }"#,
        );

        let primary_endpoint = spawn_single_response_server(primary_response).await;
        let judge_endpoint = spawn_single_response_server(repaired_response).await;

        let mut config = sample_config();
        config.endpoint = primary_endpoint;
        config.judge_endpoint = Some(judge_endpoint);
        config.judge_model = Some("gpt-5.4".to_string());
        config.judge_provider = Some(LlmProvider::OpenAINew);

        let outcome = request_graph_batch_with_repair_judge(
            &config,
            json!({
                "model": "Qwen/Qwen3.5-9B",
                "messages": [
                    {"role": "system", "content": "Return JSON only."},
                    {"role": "user", "content": "Extract a graph."}
                ]
            }),
            "rehydration",
            "repair-judge-test",
            GraphBatchRetryPolicy {
                max_attempts: 1,
                ..GraphBatchRetryPolicy::default()
            },
            GraphBatchRepairJudgePolicy {
                max_attempts: 1,
                ..GraphBatchRepairJudgePolicy::default()
            },
        )
        .await
        .expect("repair judge should salvage the invalid primary output");

        assert!(outcome.repaired_by_judge);
        assert_eq!(outcome.primary_attempts, 1);
        assert_eq!(outcome.repair_attempts, 1);
        assert_eq!(outcome.batch.root_node_id, "incident-1");
        assert_eq!(outcome.batch.nodes.len(), 2);
    }

    fn openai_chat_response(content: &str) -> String {
        json!({
            "choices": [{"message": {"content": content}}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 18}
        })
        .to_string()
    }

    async fn spawn_single_response_server(response_body: String) -> String {
        spawn_response_server(vec![response_body]).await
    }

    fn semantic_classifier_score_response(winner_index: usize) -> String {
        let data = (0..SEMANTIC_CLASS_SCORE_DOCUMENTS.len())
            .map(|index| {
                json!({
                    "index": index,
                    "object": "score",
                    "score": if index == winner_index {
                        0.99
                    } else {
                        0.01 * (index as f64 + 1.0)
                    }
                })
            })
            .collect::<Vec<_>>();

        json!({
            "data": data,
            "usage": {"prompt_tokens": 24, "completion_tokens": 0}
        })
        .to_string()
    }

    async fn spawn_response_server(response_bodies: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("listener should have address");

        tokio::spawn(async move {
            for response_body in response_bodies {
                let (mut stream, _) = listener.accept().await.expect("connection should arrive");
                let mut buffer = vec![0_u8; 8192];
                let _ = stream.read(&mut buffer).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("response should write");
            }
        });

        format!("http://{address}/v1/chat/completions")
    }
}
