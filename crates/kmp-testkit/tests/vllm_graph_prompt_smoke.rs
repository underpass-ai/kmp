use std::error::Error;

use kmp_testkit::{
    GraphBatchRepairJudgePolicy, GraphBatchRetryPolicy, LlmEvaluatorConfig, LlmProvider,
    build_graph_batch_request_body, request_graph_batch_with_policy,
    request_graph_batch_with_repair_judge,
};

const RUN_ENV: &str = "RUN_VLLM_SMOKE";
const USE_REPAIR_JUDGE_ENV: &str = "LLM_GRAPH_BATCH_USE_REPAIR_JUDGE";
const ROOT_NODE_ID: &str = "incident-2026-04-08-payments-latency";
const REQUEST_FIXTURE: &str =
    include_str!("../../../api/examples/inference-prompts/vllm-graph-materialization.request.json");

#[tokio::test]
async fn vllm_graph_prompt_smoke_returns_valid_batch() -> Result<(), Box<dyn Error + Send + Sync>> {
    if std::env::var(RUN_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping vLLM graph smoke: set {RUN_ENV}=1 plus LLM_ENDPOINT/LLM_MODEL/LLM_PROVIDER"
        );
        return Ok(());
    }

    let config = LlmEvaluatorConfig::from_env();
    assert_eq!(
        config.provider,
        LlmProvider::OpenAI,
        "vLLM smoke expects LLM_PROVIDER=openai"
    );

    let request_body = build_graph_batch_request_body(&config, REQUEST_FIXTURE)?;
    let primary_policy = GraphBatchRetryPolicy::from_env();
    let use_repair_judge = std::env::var(USE_REPAIR_JUDGE_ENV)
        .map(|value| value == "true" || value == "1")
        .unwrap_or(false);
    let outcome = if use_repair_judge {
        request_graph_batch_with_repair_judge(
            &config,
            request_body,
            "rehydration",
            "vllm-smoke",
            primary_policy,
            GraphBatchRepairJudgePolicy::from_env(),
        )
        .await?
    } else {
        request_graph_batch_with_policy(
            &config,
            request_body,
            "rehydration",
            "vllm-smoke",
            primary_policy,
        )
        .await?
    };
    let batch = outcome.batch;

    assert_eq!(batch.root_node_id, ROOT_NODE_ID);
    assert_eq!(
        batch.nodes.len(),
        3,
        "model should keep the smoke graph compact"
    );
    assert_eq!(
        batch.relations.len(),
        2,
        "model should emit exactly the requested two root relations"
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
        "model should emit the requested two node details"
    );
    assert!(batch.nodes.iter().any(|node| {
        node.node_id != ROOT_NODE_ID
            && (node.title.to_ascii_lowercase().contains("db") || node.summary.contains("50 to 5"))
    }));
    assert!(batch.nodes.iter().any(|node| {
        node.node_id != ROOT_NODE_ID
            && (node.title.to_ascii_lowercase().contains("reroute")
                || node.summary.contains("80% of traffic"))
    }));
    assert!(
        batch
            .node_details
            .iter()
            .all(|detail| detail.node_id != ROOT_NODE_ID)
    );
    assert!(
        outcome.prompt_tokens > 0,
        "smoke request should report prompt token usage"
    );
    assert!(
        outcome.completion_tokens > 0,
        "smoke request should report completion token usage"
    );
    assert!(
        outcome.primary_attempts <= primary_policy.max_attempts,
        "primary retry policy must stay bounded"
    );
    if use_repair_judge {
        let repair_policy = GraphBatchRepairJudgePolicy::from_env();
        assert!(
            outcome.repair_attempts <= repair_policy.max_attempts,
            "repair retry policy must stay bounded"
        );
    }

    Ok(())
}
