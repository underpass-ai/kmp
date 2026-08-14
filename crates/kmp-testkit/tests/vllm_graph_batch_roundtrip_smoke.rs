use std::env;
use std::error::Error;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_domain::RelationSemanticClass;
use kmp_testkit::{
    GraphBatch, GraphBatchRepairJudgePolicy, GraphBatchRetryPolicy,
    GraphBatchSemanticClassifierPolicy, LlmEvaluatorConfig, LlmProvider,
    build_graph_batch_request_body, call_llm, classify_graph_batch_semantic_classes_with_policy,
    namespace_graph_batch, request_graph_batch_with_policy, request_graph_batch_with_repair_judge,
};
use serde_json::Value;

const RUN_ENV: &str = "RUN_VLLM_GRAPH_BATCH_ROUNDTRIP_SMOKE";
const CONTEXT_CONSUMPTION_RUN_ENV: &str = "RUN_VLLM_GRAPH_BATCH_CONTEXT_CONSUMPTION_SMOKE";
const SOAK_RUN_ENV: &str = "RUN_VLLM_GRAPH_BATCH_ROUNDTRIP_SOAK";
const USE_REPAIR_JUDGE_ENV: &str = "LLM_GRAPH_BATCH_USE_REPAIR_JUDGE";
const REQUEST_FIXTURE: &str =
    include_str!("../../../api/examples/inference-prompts/vllm-graph-materialization.request.json");
const LARGE_INCIDENT_REQUEST_FIXTURE: &str = include_str!(
    "../../../api/examples/inference-prompts/vllm-graph-materialization.large-incident.request.json"
);
const KERNEL_CONTEXT_CONSUMPTION_PROMPT: &str =
    include_str!("../../../api/examples/inference-prompts/kernel-context-consumption.txt");

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[tokio::test]
async fn vllm_graph_batch_roundtrip_smoke_materializes_live_context() -> TestResult<()> {
    if env::var(RUN_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping vLLM GraphBatch roundtrip smoke: set {RUN_ENV}=1 plus LLM_* and PIR_GRAPH_BATCH_* endpoint variables"
        );
        return Ok(());
    }

    let config = LlmEvaluatorConfig::from_env();
    assert_eq!(
        config.provider,
        LlmProvider::OpenAI,
        "vLLM roundtrip smoke expects LLM_PROVIDER=openai"
    );

    let request_body = build_graph_batch_request_body(&config, REQUEST_FIXTURE)?;
    let primary_policy = GraphBatchRetryPolicy::from_env();
    let use_repair_judge = env::var(USE_REPAIR_JUDGE_ENV)
        .map(|value| value == "true" || value == "1")
        .unwrap_or(false);
    let run_id = env::var("VLLM_GRAPH_BATCH_ROUNDTRIP_RUN_ID")
        .unwrap_or_else(|_| format!("vllm-live-roundtrip-{}", unix_timestamp_secs()));

    let outcome = if use_repair_judge {
        request_graph_batch_with_repair_judge(
            &config,
            request_body,
            "rehydration",
            &run_id,
            primary_policy,
            GraphBatchRepairJudgePolicy::from_env(),
        )
        .await?
    } else {
        request_graph_batch_with_policy(
            &config,
            request_body,
            "rehydration",
            &run_id,
            primary_policy,
        )
        .await?
    };
    let mut batch = outcome.batch.clone();
    namespace_graph_batch(&mut batch, &run_id);
    assert_payments_latency_semantic_classes(&batch)?;
    let batch_payload = serde_json::to_string(&batch)?;

    let summary = run_roundtrip(&batch_payload, &run_id)?;

    assert_eq!(
        summary.get("root_node_id").and_then(Value::as_str),
        Some(batch.root_node_id.as_str())
    );
    assert_eq!(
        summary.get("run_id").and_then(Value::as_str),
        Some(run_id.as_str())
    );
    assert!(
        summary
            .get("published_messages")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            >= (batch.nodes.len() + batch.node_details.len()) as u64
    );
    assert!(
        summary
            .get("neighbor_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            >= batch.relations.len() as u64
    );
    assert!(
        summary
            .get("detail_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            >= batch.node_details.len() as u64
    );
    assert!(
        summary
            .get("rendered_chars")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            > 0
    );

    Ok(())
}

#[tokio::test]
async fn vllm_graph_batch_roundtrip_smoke_consumes_rehydrated_context() -> TestResult<()> {
    if env::var(CONTEXT_CONSUMPTION_RUN_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping vLLM context consumption smoke: set {CONTEXT_CONSUMPTION_RUN_ENV}=1 plus LLM_* and PIR_GRAPH_BATCH_* endpoint variables"
        );
        return Ok(());
    }

    let run_id = env::var("VLLM_GRAPH_BATCH_CONTEXT_CONSUMPTION_RUN_ID")
        .unwrap_or_else(|_| format!("vllm-consume-rehydration-{}", unix_timestamp_secs()));
    let use_repair_judge = use_repair_judge_from_env();
    if use_repair_judge {
        require_repair_judge_env()?;
    }

    let config = LlmEvaluatorConfig::from_env();
    let outcome = request_batch(REQUEST_FIXTURE, &run_id, use_repair_judge).await?;
    let mut batch = outcome.batch.clone();
    namespace_graph_batch(&mut batch, &run_id);
    assert_payments_latency_semantic_classes(&batch)?;
    let batch_payload = serde_json::to_string(&batch)?;
    let summary =
        run_roundtrip_with_options(&batch_payload, &run_id, true, Some("reason_preserving"))?;
    let rendered_context = summary
        .get("rendered_content")
        .and_then(Value::as_str)
        .ok_or("roundtrip summary should include rendered_content")?;
    assert!(
        !rendered_context.trim().is_empty(),
        "kernel must return rendered context for the generated graph"
    );

    let answer = answer_from_rehydrated_context(&config, rendered_context).await?;
    assert_answer_mentions_any(
        &answer,
        &["db", "database", "maxconnections", "max connections"],
        "confirmed DB connection finding",
    )?;
    assert_answer_mentions_any(
        &answer,
        &["reroute", "secondary", "80%"],
        "traffic reroute mitigation",
    )?;

    eprintln!(
        "context consumption smoke run_id={run_id} rendered_chars={} answer={}",
        rendered_context.chars().count(),
        answer.replace('\n', " ")
    );

    Ok(())
}

#[tokio::test]
async fn vllm_graph_batch_roundtrip_large_incident_soak_with_semantic_reranker() -> TestResult<()> {
    if env::var(SOAK_RUN_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping large vLLM GraphBatch soak: set {SOAK_RUN_ENV}=1 plus LLM_*, LLM_SEMANTIC_CLASSIFIER_* and PIR_GRAPH_BATCH_* endpoint variables"
        );
        return Ok(());
    }

    let iterations = env_usize("VLLM_GRAPH_BATCH_ROUNDTRIP_SOAK_ITERATIONS", 2);
    let root_run_id = env::var("VLLM_GRAPH_BATCH_ROUNDTRIP_RUN_ID")
        .unwrap_or_else(|_| format!("vllm-large-soak-{}", unix_timestamp_secs()));

    for iteration in 1..=iterations {
        let run_id = format!("{root_run_id}-{iteration}");
        let outcome = request_batch(LARGE_INCIDENT_REQUEST_FIXTURE, &run_id, false).await?;
        let classified = classify_batch_with_semantic_reranker(&outcome.batch, &run_id).await?;
        let mut batch = classified.batch;

        assert!(
            classified.attempts >= 1,
            "semantic reranker must run for iteration {iteration}"
        );

        assert_eq!(
            batch.root_node_id,
            "incident-2026-04-10-checkout-degradation"
        );
        assert_eq!(batch.nodes.len(), 16, "iteration {iteration}");
        assert_eq!(batch.relations.len(), 18, "iteration {iteration}");
        assert_eq!(batch.node_details.len(), 8, "iteration {iteration}");

        namespace_graph_batch(&mut batch, &run_id);
        let batch_payload = serde_json::to_string(&batch)?;
        let summary = run_roundtrip(&batch_payload, &run_id)?;

        assert_eq!(
            summary.get("root_node_id").and_then(Value::as_str),
            Some(batch.root_node_id.as_str())
        );
        assert_eq!(
            summary.get("run_id").and_then(Value::as_str),
            Some(run_id.as_str())
        );
        assert!(
            summary
                .get("published_messages")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                >= (batch.nodes.len() + batch.node_details.len()) as u64,
            "iteration {iteration}"
        );
        assert!(
            summary
                .get("neighbor_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                >= (batch.nodes.len().saturating_sub(1)) as u64,
            "iteration {iteration}"
        );
        assert!(
            summary
                .get("relationship_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                >= batch.relations.len() as u64,
            "iteration {iteration}"
        );
        assert!(
            summary
                .get("detail_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                >= batch.node_details.len() as u64,
            "iteration {iteration}"
        );
        assert!(
            summary
                .get("rendered_chars")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 0,
            "iteration {iteration}"
        );

        eprintln!(
            "large soak iteration={iteration}/{iterations} primary_attempts={} semantic_classifier_attempts={} semantic_classifier_changed_relations={}",
            outcome.primary_attempts, classified.attempts, classified.changed_relations
        );
    }

    Ok(())
}

async fn classify_batch_with_semantic_reranker(
    batch: &GraphBatch,
    run_id: &str,
) -> TestResult<kmp_testkit::GraphBatchSemanticClassifierOutcome> {
    let config = LlmEvaluatorConfig::from_env();
    classify_graph_batch_semantic_classes_with_policy(
        &config,
        batch,
        "rehydration",
        run_id,
        GraphBatchSemanticClassifierPolicy::from_env(),
    )
    .await
    .map_err(Into::into)
}

async fn request_batch(
    request_fixture: &str,
    run_id: &str,
    use_repair_judge: bool,
) -> TestResult<kmp_testkit::GraphBatchRequestOutcome> {
    let config = LlmEvaluatorConfig::from_env();
    assert_eq!(
        config.provider,
        LlmProvider::OpenAI,
        "vLLM roundtrip smoke expects LLM_PROVIDER=openai"
    );

    let request_body = build_graph_batch_request_body(&config, request_fixture)?;
    let primary_policy = GraphBatchRetryPolicy::from_env();
    Ok(if use_repair_judge {
        request_graph_batch_with_repair_judge(
            &config,
            request_body,
            "rehydration",
            run_id,
            primary_policy,
            GraphBatchRepairJudgePolicy::from_env(),
        )
        .await?
    } else {
        request_graph_batch_with_policy(
            &config,
            request_body,
            "rehydration",
            run_id,
            primary_policy,
        )
        .await?
    })
}

#[test]
fn namespace_graph_batch_rewrites_node_references() -> TestResult<()> {
    let mut batch = serde_json::from_str::<GraphBatch>(
        r#"{
          "root_node_id":"incident-1",
          "nodes":[
            {"node_id":"incident-1","node_kind":"incident","title":"Incident"},
            {"node_id":"finding-1","node_kind":"finding","title":"Finding"},
            {"node_id":"decision-1","node_kind":"decision","title":"Decision"}
          ],
          "relations":[
            {
              "source_node_id":"incident-1",
              "target_node_id":"finding-1",
              "relation_type":"caused_by",
              "semantic_class":"causal",
              "caused_by_node_id":"finding-1",
              "rationale":"Finding caused incident",
              "confidence":"high"
            },
            {
              "source_node_id":"incident-1",
              "target_node_id":"decision-1",
              "relation_type":"mitigated_by",
              "semantic_class":"motivational",
              "decision_id":"decision-1",
              "motivation":"Decision mitigates incident",
              "confidence":"high"
            }
          ],
          "node_details":[
            {"node_id":"finding-1","detail":"Finding detail"}
          ]
        }"#,
    )?;

    namespace_graph_batch(&mut batch, "run/1");

    assert_eq!(batch.root_node_id, "incident-1--run-1");
    assert_eq!(batch.nodes[0].node_id, "incident-1--run-1");
    assert_eq!(batch.relations[0].source_node_id, "incident-1--run-1");
    assert_eq!(batch.relations[0].target_node_id, "finding-1--run-1");
    assert_eq!(
        batch.relations[0].caused_by_node_id.as_deref(),
        Some("finding-1--run-1")
    );
    assert_eq!(
        batch.relations[1].decision_id.as_deref(),
        Some("decision-1--run-1")
    );
    assert_eq!(batch.node_details[0].node_id, "finding-1--run-1");

    Ok(())
}

fn run_roundtrip(batch_payload: &str, run_id: &str) -> TestResult<Value> {
    run_roundtrip_with_options(batch_payload, run_id, false, None)
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

    let mut command = Command::new(env!("CARGO_BIN_EXE_graph_batch_roundtrip"));
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
) -> TestResult<String> {
    let client = reqwest::Client::builder().build()?;
    let prompt = KERNEL_CONTEXT_CONSUMPTION_PROMPT
        .replace(
            "{question}",
            "Answer in one concise sentence: what confirmed finding caused the incident, and what mitigation decision was selected?",
        )
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

    Err(format!("vLLM answer did not mention {label}; answer={answer}").into())
}

fn assert_payments_latency_semantic_classes(batch: &GraphBatch) -> TestResult<()> {
    let finding_node_ids = batch
        .nodes
        .iter()
        .filter(|node| {
            let properties = node
                .properties
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(" ");
            let text =
                format!("{} {} {}", node.title, node.summary, properties).to_ascii_lowercase();
            text.contains("db")
                || text.contains("database")
                || text.contains("maxconnections")
                || text.contains("50 -> 5")
                || text.contains("50 to 5")
        })
        .map(|node| node.node_id.as_str())
        .collect::<Vec<_>>();

    if finding_node_ids.is_empty() {
        return Err("GraphBatch should include the DB maxConnections finding node".into());
    }

    let relation = batch
        .relations
        .iter()
        .find(|relation| finding_node_ids.contains(&relation.target_node_id.as_str()))
        .ok_or("GraphBatch should relate the incident to the DB maxConnections finding")?;

    if relation.semantic_class != RelationSemanticClass::Causal {
        return Err(format!(
            "DB maxConnections finding relation must be causal, got {:?}",
            relation.semantic_class
        )
        .into());
    }

    Ok(())
}

fn required_env(key: &str) -> TestResult<String> {
    env::var(key).map_err(|_| format!("missing env `{key}`").into())
}

fn require_repair_judge_env() -> TestResult<()> {
    required_env("LLM_JUDGE_ENDPOINT")?;
    required_env("LLM_JUDGE_MODEL")?;
    required_env("LLM_JUDGE_PROVIDER")?;
    Ok(())
}

fn use_repair_judge_from_env() -> bool {
    env::var(USE_REPAIR_JUDGE_ENV)
        .map(|value| value == "true" || value == "1")
        .unwrap_or(false)
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
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
