use std::env;
use std::error::Error;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    GraphBatch, GraphBatchSemanticClassifierOutcome, GraphBatchSemanticClassifierPolicy,
    LlmEvaluatorConfig, call_llm, classify_graph_batch_semantic_classes_with_policy,
    namespace_graph_batch, parse_graph_batch,
};
use serde_json::{Value, json};

const RUN_ENV: &str = "RUN_PIR_GRAPH_BATCH_SMOKE";
const CONTEXT_CONSUMPTION_RUN_ENV: &str = "RUN_PIR_GRAPH_BATCH_CONTEXT_CONSUMPTION_SMOKE";
const DEFAULT_BATCH_FIXTURE: &str =
    include_str!("../../../api/examples/kernel/v1beta1/async/incident-graph-batch.json");
const INCREMENTAL_BATCH_FIXTURE: &str = include_str!(
    "../../../api/examples/kernel/v1beta1/async/incident-graph-batch.incremental-2.json"
);
const CORRECTIVE_BATCH_FIXTURE: &str = include_str!(
    "../../../api/examples/kernel/v1beta1/async/incident-graph-batch.incremental-3.json"
);
const KERNEL_CONTEXT_CONSUMPTION_PROMPT: &str =
    include_str!("../../../api/examples/inference-prompts/kernel-context-consumption.txt");
const SECOND_WAVE_TASK_NODE_ID: &str = "task:pir-2026-04-09-payments-latency:apply-retry-cap";

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

struct ExecutedWave {
    run_id: String,
    batch: GraphBatch,
    summary: Value,
    rendered_context: Option<String>,
}

struct LiveWaveRequest<'a> {
    label: &'a str,
    payload: &'a str,
    node_namespace: &'a str,
    run_id: &'a str,
    detail_node_base_id: Option<&'a str>,
    include_rendered_content: bool,
    rehydration_mode: Option<&'a str>,
}

#[test]
fn pir_graph_batch_roundtrip_smoke_succeeds_against_live_kernel() -> TestResult<()> {
    if env::var(RUN_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping PIR GraphBatch roundtrip smoke: set {RUN_ENV}=1 plus PIR_GRAPH_BATCH_NATS_URL and PIR_GRAPH_BATCH_GRPC_ENDPOINT"
        );
        return Ok(());
    }

    let run_id = env::var("PIR_GRAPH_BATCH_RUN_ID")
        .unwrap_or_else(|_| format!("pir-live-smoke-{}", unix_timestamp_secs()));
    let payload = load_fixture("PIR_GRAPH_BATCH_INPUT", DEFAULT_BATCH_FIXTURE)?;
    let mut batch = parse_graph_batch(&payload)?;
    namespace_graph_batch(&mut batch, &run_id);
    let summary = run_roundtrip(&serde_json::to_vec(&batch)?, &run_id, None, false, None)?;
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
            >= 4
    );
    assert!(
        summary
            .get("neighbor_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            >= 2
    );
    assert!(
        summary
            .get("detail_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            >= 2
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
async fn pir_graph_batch_incremental_context_consumption_smoke_succeeds_against_live_kernel()
-> TestResult<()> {
    if env::var(CONTEXT_CONSUMPTION_RUN_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping PIR GraphBatch context consumption smoke: set {CONTEXT_CONSUMPTION_RUN_ENV}=1 plus LLM_* and PIR_GRAPH_BATCH_* variables"
        );
        return Ok(());
    }

    let node_namespace = env::var("PIR_GRAPH_BATCH_NODE_NAMESPACE")
        .unwrap_or_else(|_| format!("pir-live-context-{}", unix_timestamp_secs()));
    let first_run_id = format!("{node_namespace}-wave-1");
    let second_run_id = format!("{node_namespace}-wave-2");
    let third_run_id = format!("{node_namespace}-wave-3");

    let wave_one_payload = load_fixture("PIR_GRAPH_BATCH_INPUT", DEFAULT_BATCH_FIXTURE)?;
    let wave_two_payload = load_fixture(
        "PIR_GRAPH_BATCH_INCREMENTAL_INPUT",
        INCREMENTAL_BATCH_FIXTURE,
    )?;
    let wave_three_payload = load_fixture(
        "PIR_GRAPH_BATCH_INCREMENTAL_THREE_INPUT",
        CORRECTIVE_BATCH_FIXTURE,
    )?;
    let config = LlmEvaluatorConfig::from_env();

    let wave_one = execute_live_wave(
        &config,
        LiveWaveRequest {
            label: "wave-1",
            payload: &wave_one_payload,
            node_namespace: &node_namespace,
            run_id: &first_run_id,
            detail_node_base_id: None,
            include_rendered_content: false,
            rehydration_mode: None,
        },
    )
    .await?;
    let summary_one = &wave_one.summary;
    assert_eq!(
        summary_one.get("root_node_id").and_then(Value::as_str),
        Some(wave_one.batch.root_node_id.as_str())
    );
    assert_eq!(
        summary_one.get("run_id").and_then(Value::as_str),
        Some(wave_one.run_id.as_str())
    );
    assert_summary_counts(summary_one, 2, 2, 2);

    let wave_two = execute_live_wave(
        &config,
        LiveWaveRequest {
            label: "wave-2",
            payload: &wave_two_payload,
            node_namespace: &node_namespace,
            run_id: &second_run_id,
            detail_node_base_id: None,
            include_rendered_content: true,
            rehydration_mode: Some("reason_preserving"),
        },
    )
    .await?;
    let summary_two = &wave_two.summary;

    assert_eq!(
        summary_two.get("root_node_id").and_then(Value::as_str),
        Some(wave_two.batch.root_node_id.as_str())
    );
    assert_eq!(
        summary_two.get("root_node_id").and_then(Value::as_str),
        summary_one.get("root_node_id").and_then(Value::as_str)
    );
    assert_eq!(
        summary_two.get("run_id").and_then(Value::as_str),
        Some(wave_two.run_id.as_str())
    );
    assert_summary_counts(summary_two, 4, 4, 4);

    let rendered_context_two = wave_two
        .rendered_context
        .as_deref()
        .ok_or("wave 2 roundtrip summary should include rendered_content")?;
    assert!(
        rendered_context_two.contains("Retry storm amplified load"),
        "rendered content should include the second-wave finding"
    );
    assert!(
        rendered_context_two.contains("Apply retry cap"),
        "rendered content should include the second-wave task"
    );
    let wave_three = execute_live_wave(
        &config,
        LiveWaveRequest {
            label: "wave-3",
            payload: &wave_three_payload,
            node_namespace: &node_namespace,
            run_id: &third_run_id,
            detail_node_base_id: Some(SECOND_WAVE_TASK_NODE_ID),
            include_rendered_content: true,
            rehydration_mode: Some("reason_preserving"),
        },
    )
    .await?;
    let summary_three = &wave_three.summary;

    assert_eq!(
        summary_three.get("root_node_id").and_then(Value::as_str),
        Some(wave_three.batch.root_node_id.as_str())
    );
    assert_eq!(
        summary_three.get("root_node_id").and_then(Value::as_str),
        summary_one.get("root_node_id").and_then(Value::as_str)
    );
    assert_eq!(
        summary_three.get("run_id").and_then(Value::as_str),
        Some(wave_three.run_id.as_str())
    );
    assert_summary_counts(summary_three, 4, 4, 4);
    assert_eq!(
        summary_three.get("detail_revision").and_then(Value::as_u64),
        Some(2)
    );

    let rendered_context_three = wave_three
        .rendered_context
        .as_deref()
        .ok_or("wave 3 roundtrip summary should include rendered_content")?;
    assert!(
        rendered_context_three.contains("returned below 1.1 seconds"),
        "rendered content should include the corrected incident summary"
    );
    assert!(
        rendered_context_three.contains("retry-cap change was completed"),
        "rendered content should include the corrected task detail revision"
    );

    let answer = answer_from_rehydrated_context(
        &config,
        rendered_context_three,
        "Answer in one concise sentence: after the retry-cap rollout, what happened to incident latency, and what was the final state of the retry-cap task?",
    )
    .await?;

    assert_answer_mentions_any(
        &answer,
        &["below 1.1", "recovered", "returned", "stabilizing"],
        "corrected incident state",
    )?;
    assert_answer_mentions_any(
        &answer,
        &["completed", "rolled out", "full jitter"],
        "corrected retry-cap task state",
    )?;

    eprintln!(
        "pir context consumption result {}",
        serde_json::to_string_pretty(&json!({
            "node_namespace": node_namespace,
            "answer": answer.replace('\n', " "),
            "rendered_chars": rendered_context_three.chars().count(),
            "wave_1": {
                "run_id": wave_one.run_id,
                "neighbor_count": summary_one.get("neighbor_count").and_then(Value::as_u64),
                "relationship_count": summary_one.get("relationship_count").and_then(Value::as_u64),
                "detail_count": summary_one.get("detail_count").and_then(Value::as_u64),
            },
            "wave_2": {
                "run_id": wave_two.run_id,
                "neighbor_count": summary_two.get("neighbor_count").and_then(Value::as_u64),
                "relationship_count": summary_two.get("relationship_count").and_then(Value::as_u64),
                "detail_count": summary_two.get("detail_count").and_then(Value::as_u64),
            },
            "wave_3": {
                "run_id": wave_three.run_id,
                "neighbor_count": summary_three.get("neighbor_count").and_then(Value::as_u64),
                "relationship_count": summary_three.get("relationship_count").and_then(Value::as_u64),
                "detail_count": summary_three.get("detail_count").and_then(Value::as_u64),
                "detail_revision": summary_three.get("detail_revision").and_then(Value::as_u64),
            }
        }))?
    );
    eprintln!(
        "pir context consumption smoke node_namespace={node_namespace} rendered_chars={} answer={}",
        rendered_context_three.chars().count(),
        answer.replace('\n', " ")
    );

    Ok(())
}

async fn execute_live_wave(
    config: &LlmEvaluatorConfig,
    request: LiveWaveRequest<'_>,
) -> TestResult<ExecutedWave> {
    let mut batch = parse_graph_batch(request.payload)?;
    namespace_graph_batch(&mut batch, request.node_namespace);
    let detail_node_id = request
        .detail_node_base_id
        .map(|base_id| resolve_namespaced_node_id(&batch, base_id))
        .transpose()?;
    let classified = classify_batch_with_semantic_reranker(config, &batch, request.run_id).await?;
    let batch = classified.batch.clone();
    let summary = run_roundtrip(
        &serde_json::to_vec(&batch)?,
        request.run_id,
        detail_node_id.as_deref(),
        request.include_rendered_content,
        request.rehydration_mode,
    )?;
    let rendered_context = summary
        .get("rendered_content")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    emit_wave_evidence(
        request.label,
        request.run_id,
        &batch,
        &classified,
        &summary,
        rendered_context.as_deref(),
    );
    Ok(ExecutedWave {
        run_id: request.run_id.to_string(),
        batch,
        summary,
        rendered_context,
    })
}

async fn classify_batch_with_semantic_reranker(
    config: &LlmEvaluatorConfig,
    batch: &GraphBatch,
    run_id: &str,
) -> TestResult<GraphBatchSemanticClassifierOutcome> {
    let outcome = classify_graph_batch_semantic_classes_with_policy(
        config,
        batch,
        "rehydration",
        run_id,
        GraphBatchSemanticClassifierPolicy::from_env(),
    )
    .await?;
    assert!(
        outcome.attempts >= 1,
        "semantic reranker must run for run_id={run_id}"
    );
    Ok(outcome)
}

fn emit_wave_evidence(
    label: &str,
    run_id: &str,
    batch: &GraphBatch,
    classifier: &GraphBatchSemanticClassifierOutcome,
    summary: &Value,
    rendered_context: Option<&str>,
) {
    let rendered_excerpt = rendered_context.map(|context| excerpt(context, 280));
    let evidence = json!({
        "wave": label,
        "run_id": run_id,
        "root_node_id": batch.root_node_id,
        "published_graph": {
            "node_ids": batch.nodes.iter().map(|node| node.node_id.clone()).collect::<Vec<_>>(),
            "relation_triplets": batch.relations.iter().map(|relation| {
                json!({
                    "source_node_id": relation.source_node_id,
                    "relation_type": relation.relation_type,
                    "target_node_id": relation.target_node_id,
                    "semantic_class": relation.semantic_class,
                })
            }).collect::<Vec<_>>(),
            "detail_node_ids": batch.node_details.iter().map(|detail| detail.node_id.clone()).collect::<Vec<_>>(),
        },
        "semantic_reranker": {
            "attempts": classifier.attempts,
            "changed_relations": classifier.changed_relations,
            "prompt_tokens": classifier.prompt_tokens,
            "completion_tokens": classifier.completion_tokens,
        },
        "roundtrip_summary": summary,
        "rendered_excerpt": rendered_excerpt,
    });
    if let Ok(pretty) = serde_json::to_string_pretty(&evidence) {
        eprintln!("pir wave evidence {pretty}");
    }
}

fn load_fixture(env_key: &str, default_fixture: &str) -> TestResult<String> {
    match env::var(env_key) {
        Ok(path) if !path.trim().is_empty() => Ok(fs::read_to_string(path)?),
        _ => Ok(default_fixture.to_string()),
    }
}

fn run_roundtrip(
    batch_payload: &[u8],
    run_id: &str,
    detail_node_id: Option<&str>,
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

    if let Some(detail_node_id) = detail_node_id {
        command.arg("--detail-node-id").arg(detail_node_id);
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
        .write_all(batch_payload)?;
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

fn assert_summary_counts(
    summary: &Value,
    expected_neighbors: u64,
    expected_relationships: u64,
    expected_details: u64,
) {
    assert_eq!(
        summary.get("neighbor_count").and_then(Value::as_u64),
        Some(expected_neighbors)
    );
    assert_eq!(
        summary.get("relationship_count").and_then(Value::as_u64),
        Some(expected_relationships)
    );
    assert_eq!(
        summary.get("detail_count").and_then(Value::as_u64),
        Some(expected_details)
    );
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

fn resolve_namespaced_node_id(batch: &GraphBatch, base_id: &str) -> TestResult<String> {
    batch
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .chain(
            batch
                .node_details
                .iter()
                .map(|detail| detail.node_id.as_str()),
        )
        .find(|node_id| *node_id == base_id || node_id.starts_with(&format!("{base_id}--")))
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("missing namespaced node id for `{base_id}`").into())
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
