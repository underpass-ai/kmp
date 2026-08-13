use std::error::Error;

use kmp_testkit::{
    GraphBatchRepairJudgePolicy, GraphBatchRetryPolicy, LlmEvaluatorConfig,
    build_graph_batch_request_body, request_graph_batch_with_repair_judge,
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const RUN_ENV: &str = "RUN_VLLM_SMOKE";
const ROOT_NODE_ID: &str = "incident-2026-04-08-payments-latency";
const REQUEST_FIXTURE: &str =
    include_str!("../../../api/examples/inference-prompts/vllm-graph-materialization.request.json");

#[tokio::test]
async fn vllm_graph_repair_judge_smoke_salvages_invalid_primary_response()
-> Result<(), Box<dyn Error + Send + Sync>> {
    if std::env::var(RUN_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping vLLM repair-judge smoke: set {RUN_ENV}=1 plus LLM_JUDGE_ENDPOINT/LLM_JUDGE_MODEL/LLM_JUDGE_PROVIDER"
        );
        return Ok(());
    }

    let invalid_primary_response = openai_chat_response(
        r#"{
          "root_node_id":"incident-2026-04-08-payments-latency",
          "nodes":[],
          "relations":[],
          "node_details":[]
        }"#,
    );
    let primary_endpoint = spawn_single_response_server(invalid_primary_response).await;

    let mut config = LlmEvaluatorConfig::from_env();
    config.endpoint = primary_endpoint;

    let request_body = build_graph_batch_request_body(&config, REQUEST_FIXTURE)?;
    let outcome = request_graph_batch_with_repair_judge(
        &config,
        request_body,
        "rehydration",
        "vllm-repair-judge-smoke",
        GraphBatchRetryPolicy {
            max_attempts: 1,
            ..GraphBatchRetryPolicy::from_env()
        },
        GraphBatchRepairJudgePolicy::from_env(),
    )
    .await?;

    assert!(outcome.repaired_by_judge);
    assert_eq!(outcome.primary_attempts, 1);
    assert!(outcome.repair_attempts >= 1);
    assert_eq!(outcome.batch.root_node_id, ROOT_NODE_ID);
    assert_eq!(outcome.batch.nodes.len(), 3);
    assert_eq!(outcome.batch.relations.len(), 2);
    assert_eq!(outcome.batch.node_details.len(), 2);

    Ok(())
}

fn openai_chat_response(content: &str) -> String {
    json!({
        "choices": [{"message": {"content": content}}],
        "usage": {"prompt_tokens": 12, "completion_tokens": 18}
    })
    .to_string()
}

async fn spawn_single_response_server(response_body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("listener should have address");

    tokio::spawn(async move {
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
    });

    format!("http://{address}/v1/chat/completions")
}
