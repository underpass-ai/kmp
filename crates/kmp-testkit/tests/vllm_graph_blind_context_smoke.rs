use std::env;
use std::error::Error;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    GraphBatchRetryPolicy, GraphBatchSemanticClassifierPolicy, LlmEvaluatorConfig, LlmProvider,
    build_graph_batch_request_body, call_llm, classify_graph_batch_semantic_classes_with_policy,
    namespace_graph_batch, request_graph_batch_with_policy,
};
use serde_json::{Value, json};

const RUN_ENV: &str = "RUN_VLLM_BLIND_CONTEXT_CONSUMPTION_SMOKE";
const REQUEST_FIXTURE: &str = include_str!(
    "../../../api/examples/inference-prompts/vllm-graph-materialization.blind.request.json"
);
const KERNEL_CONTEXT_CONSUMPTION_PROMPT: &str =
    include_str!("../../../api/examples/inference-prompts/kernel-context-consumption.txt");

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[tokio::test]
async fn vllm_blind_context_consumption_smoke_uses_rehydrated_context() -> TestResult<()> {
    if env::var(RUN_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping blind vLLM context consumption smoke: set {RUN_ENV}=1 plus LLM_*, LLM_SEMANTIC_CLASSIFIER_*, and PIR_GRAPH_BATCH_* variables"
        );
        return Ok(());
    }

    let config = LlmEvaluatorConfig::from_env();
    assert_eq!(
        config.provider,
        LlmProvider::OpenAI,
        "blind context smoke expects LLM_PROVIDER=openai"
    );

    let run_id = env::var("VLLM_BLIND_CONTEXT_RUN_ID")
        .unwrap_or_else(|_| format!("vllm-blind-context-{}", unix_timestamp_secs()));
    let request_body = build_graph_batch_request_body(&config, REQUEST_FIXTURE)?;
    let primary = request_graph_batch_with_policy(
        &config,
        request_body,
        "rehydration",
        &run_id,
        GraphBatchRetryPolicy::from_env(),
    )
    .await?;
    let classified = classify_graph_batch_semantic_classes_with_policy(
        &config,
        &primary.batch,
        "rehydration",
        &run_id,
        GraphBatchSemanticClassifierPolicy::from_env(),
    )
    .await?;

    assert!(
        classified.attempts >= 1,
        "blind context smoke requires the semantic reranker to run"
    );

    let mut batch = classified.batch;
    namespace_graph_batch(&mut batch, &run_id);
    let batch_payload = serde_json::to_string(&batch)?;
    let summary =
        run_roundtrip_with_options(&batch_payload, &run_id, true, Some("reason_preserving"))?;
    let rendered_context = summary
        .get("rendered_content")
        .and_then(Value::as_str)
        .ok_or("roundtrip summary should include rendered_content")?;
    assert!(
        !rendered_context.trim().is_empty(),
        "kernel must return rendered context for the blind graph"
    );

    let answer = answer_from_rehydrated_context(
        &config,
        rendered_context,
        "Answer in one concise sentence: which operational change most likely explains the latency spike, and what action reduced user impact while recovery progressed?",
    )
    .await?;

    assert!(
        !answer.to_ascii_lowercase().contains("not_found"),
        "blind context answer should use kernel context instead of returning NOT_FOUND; answer={answer}"
    );
    assert_answer_mentions_any(
        &answer,
        &[
            "maxconnections",
            "max connections",
            "db_max_connections",
            "50 to 5",
            "config map",
        ],
        "likely operational cause",
    )?;
    assert_answer_mentions_any(
        &answer,
        &[
            "rollback",
            "roll back",
            "secondary region",
            "traffic shift",
            "shifted most traffic",
            "shifted traffic",
        ],
        "mitigation action",
    )?;

    eprintln!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "run_id": run_id,
            "primary_attempts": primary.primary_attempts,
            "primary_prompt_tokens": primary.prompt_tokens,
            "primary_completion_tokens": primary.completion_tokens,
            "semantic_classifier_attempts": classified.attempts,
            "semantic_classifier_changed_relations": classified.changed_relations,
            "rendered_chars": rendered_context.chars().count(),
            "rendered_excerpt": excerpt(rendered_context, 320),
            "answer": answer.replace('\n', " "),
            "roundtrip_summary": summary,
        }))?
    );

    Ok(())
}

fn run_roundtrip_with_options(
    batch_payload: &str,
    run_id: &str,
    include_rendered_content: bool,
    rehydration_mode: Option<&str>,
) -> TestResult<Value> {
    let nats_url = required_env("PIR_GRAPH_BATCH_NATS_URL")?;
    let grpc_endpoint = required_env("PIR_GRAPH_BATCH_GRPC_ENDPOINT")?;
    let role =
        env::var("PIR_GRAPH_BATCH_ROLE").unwrap_or_else(|_| "incident-commander".to_string());
    let scopes = env::var("PIR_GRAPH_BATCH_SCOPES").unwrap_or_else(|_| "graph,details".to_string());

    let mut command = Command::new(graph_batch_roundtrip_bin());
    command
        .arg("--input")
        .arg("-")
        .arg("--nats-url")
        .arg(nats_url)
        .arg("--grpc-endpoint")
        .arg(grpc_endpoint)
        .arg("--run-id")
        .arg(run_id)
        .arg("--role")
        .arg(role)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for scope in scopes
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        command.arg("--requested-scope").arg(scope);
    }

    add_optional_flag(
        &mut command,
        "PIR_GRAPH_BATCH_GRPC_TLS_CA_PATH",
        "--grpc-tls-ca-path",
    );
    add_optional_flag(
        &mut command,
        "PIR_GRAPH_BATCH_GRPC_TLS_CERT_PATH",
        "--grpc-tls-cert-path",
    );
    add_optional_flag(
        &mut command,
        "PIR_GRAPH_BATCH_GRPC_TLS_KEY_PATH",
        "--grpc-tls-key-path",
    );
    add_optional_flag(
        &mut command,
        "PIR_GRAPH_BATCH_GRPC_TLS_DOMAIN_NAME",
        "--grpc-tls-domain-name",
    );
    add_optional_flag(
        &mut command,
        "PIR_GRAPH_BATCH_NATS_TLS_CA_PATH",
        "--nats-tls-ca-path",
    );
    add_optional_flag(
        &mut command,
        "PIR_GRAPH_BATCH_NATS_TLS_CERT_PATH",
        "--nats-tls-cert-path",
    );
    add_optional_flag(
        &mut command,
        "PIR_GRAPH_BATCH_NATS_TLS_KEY_PATH",
        "--nats-tls-key-path",
    );

    if env::var("PIR_GRAPH_BATCH_NATS_TLS_FIRST").as_deref() == Ok("true") {
        command.arg("--nats-tls-first");
    }
    if include_rendered_content {
        command.arg("--include-rendered-content");
    }
    if let Some(rehydration_mode) = rehydration_mode {
        command.arg("--rehydration-mode").arg(rehydration_mode);
    }

    let mut child = command.spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or("roundtrip stdin should be available")?
        .write_all(batch_payload.as_bytes())?;
    let output = child.wait_with_output()?;

    if !output.status.success() {
        return Err(format!(
            "graph_batch_roundtrip failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}

async fn answer_from_rehydrated_context(
    config: &LlmEvaluatorConfig,
    rendered_context: &str,
    question: &str,
) -> TestResult<String> {
    let client = reqwest::Client::builder().build()?;
    let prompt = KERNEL_CONTEXT_CONSUMPTION_PROMPT
        .replace("{question}", question)
        .replace("{rendered_context}", rendered_context);
    let (answer, _prompt_tokens, _completion_tokens) = call_llm(
        &client,
        &config.endpoint,
        &config.model,
        config.provider,
        config.api_key.as_deref(),
        &prompt,
        config.max_tokens.clamp(128, 512),
        0.0,
    )
    .await?;
    Ok(answer)
}

fn assert_answer_mentions_any(answer: &str, needles: &[&str], label: &str) -> TestResult<()> {
    let normalized = answer.to_ascii_lowercase();
    if needles
        .iter()
        .any(|needle| normalized.contains(&needle.to_ascii_lowercase()))
    {
        return Ok(());
    }

    Err(format!("LLM answer did not mention {label}; answer={answer}").into())
}

fn required_env(key: &str) -> TestResult<String> {
    env::var(key).map_err(|_| format!("missing env `{key}`").into())
}

fn graph_batch_roundtrip_bin() -> String {
    match env::var("GRAPH_BATCH_ROUNDTRIP_BIN") {
        Ok(path) if !path.trim().is_empty() => path,
        _ => env!("CARGO_BIN_EXE_graph_batch_roundtrip").to_string(),
    }
}

fn add_optional_flag(command: &mut Command, env_key: &str, flag: &str) {
    if let Ok(value) = env::var(env_key)
        && !value.trim().is_empty()
    {
        command.arg(flag).arg(value);
    }
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn excerpt(value: &str, max_chars: usize) -> String {
    let mut excerpt = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        excerpt.push_str("...");
    }
    excerpt
}
