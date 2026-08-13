use std::error::Error;

use kmp_testkit::{
    GraphBatchRetryPolicy, LlmEvaluatorConfig, LlmProvider, build_graph_batch_request_body,
    request_graph_batch_with_policy,
};
use serde_json::Value;

const RUN_ENV: &str = "RUN_VLLM_BLIND_SMOKE";
const ROOT_NODE_ID: &str = "incident-2026-04-08-payments-latency";
const REQUEST_FIXTURE: &str = include_str!(
    "../../../api/examples/inference-prompts/vllm-graph-materialization.blind.request.json"
);

#[test]
fn blind_request_fixture_reduces_prompt_leakage() -> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture: Value = serde_json::from_str(REQUEST_FIXTURE)?;
    let user_prompt = fixture["messages"][1]["content"]
        .as_str()
        .ok_or("blind request fixture must contain a user prompt")?;

    for banned in [
        "Confirmed finding:",
        "Mitigation decision:",
        "Use deterministic node ids:",
        "confirmed finding explains the latency incident",
    ] {
        assert!(
            !user_prompt.contains(banned),
            "blind fixture should not contain `{banned}`"
        );
    }

    for required in [
        "do not assume a cause is confirmed unless the notes say it directly",
        "At least one non-root node must capture supporting evidence or inspection.",
        "At least one non-root node must capture an action already taken during the incident.",
        "Do not require deterministic non-root node ids.",
    ] {
        assert!(
            user_prompt.contains(required),
            "blind fixture should contain `{required}`"
        );
    }

    Ok(())
}

#[tokio::test]
async fn vllm_blind_graph_prompt_smoke_returns_valid_bounded_batch()
-> Result<(), Box<dyn Error + Send + Sync>> {
    if std::env::var(RUN_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping blind vLLM graph smoke: set {RUN_ENV}=1 plus LLM_ENDPOINT/LLM_MODEL/LLM_PROVIDER"
        );
        return Ok(());
    }

    let config = LlmEvaluatorConfig::from_env();
    assert_eq!(
        config.provider,
        LlmProvider::OpenAI,
        "blind vLLM smoke expects LLM_PROVIDER=openai"
    );

    let request_body = build_graph_batch_request_body(&config, REQUEST_FIXTURE)?;
    let primary_policy = GraphBatchRetryPolicy::from_env();
    let outcome = request_graph_batch_with_policy(
        &config,
        request_body,
        "rehydration",
        "vllm-blind-smoke",
        primary_policy,
    )
    .await?;
    let batch = outcome.batch;

    assert_eq!(batch.root_node_id, ROOT_NODE_ID);
    assert_eq!(batch.nodes.len(), 4, "blind fixture keeps a bounded graph");
    assert_eq!(
        batch.relations.len(),
        3,
        "blind fixture should still emit a compact outward graph"
    );
    assert!(
        batch
            .relations
            .iter()
            .all(|relation| relation.source_node_id == ROOT_NODE_ID)
    );
    assert_eq!(
        batch.node_details.len(),
        2,
        "blind fixture should keep evidence compact"
    );
    assert!(
        batch
            .node_details
            .iter()
            .all(|detail| detail.node_id != ROOT_NODE_ID)
    );
    assert!(
        outcome.prompt_tokens > 0,
        "blind smoke request should report prompt token usage"
    );
    assert!(
        outcome.completion_tokens > 0,
        "blind smoke request should report completion token usage"
    );
    assert!(
        outcome.primary_attempts <= primary_policy.max_attempts,
        "primary retry policy must stay bounded"
    );

    Ok(())
}
