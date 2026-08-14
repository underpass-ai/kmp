#![cfg(feature = "container-tests")]

use std::sync::OnceLock;

use kmp_proto::v1beta1::{GetContextRequest, GetNodeDetailRequest};
use kmp_testkit::{llm_graph_to_projection_events, parse_llm_graph_batch};
use kmp_tests_shared::debug::debug_log;
use kmp_tests_shared::fixtures::TestFixture;
use kmp_tests_shared::ports::ClosureSeed;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};
use tonic::transport::Channel;

const ROOT_NODE_ID: &str = "incident-2026-04-08-payments-latency";
const FOCUS_NODE_ID: &str = "finding-db-pool-typo";
const DECISION_NODE_ID: &str = "decision-reroute-secondary";

const GRAPH_MATERIALIZATION_PROMPT: &str =
    include_str!("../../../api/examples/inference-prompts/graph-materialization.txt");
const LLM_BATCH_JSON: &str =
    include_str!("../../../api/examples/kernel/v1beta1/async/vllm-graph-batch.json");
const INCREMENTAL_ROOT_NODE_ID: &str = "incident-2026-04-09-checkout-latency";
const INCREMENTAL_INITIAL_FOCUS_NODE_ID: &str = "finding-db-pool-typo";
const INCREMENTAL_SECOND_WAVE_NODE_ID: &str = "task-apply-retry-cap";
const INCREMENTAL_BATCH_ONE_JSON: &str =
    include_str!("../../../api/examples/kernel/v1beta1/async/vllm-graph-batch.incremental-1.json");
const INCREMENTAL_BATCH_TWO_JSON: &str =
    include_str!("../../../api/examples/kernel/v1beta1/async/vllm-graph-batch.incremental-2.json");

#[tokio::test]
async fn llm_graph_batch_materializes_through_projection_runtime() {
    let _guard = container_test_guard().lock().await;
    debug_log("starting test llm_graph_batch_materializes_through_projection_runtime");
    assert!(
        GRAPH_MATERIALIZATION_PROMPT.contains("Return JSON only"),
        "prompt fixture should describe a strict JSON contract"
    );

    let batch = parse_llm_graph_batch(LLM_BATCH_JSON).expect("example LLM batch should parse");
    let root_id = batch.root_node_id.clone();
    let messages = llm_graph_to_projection_events(&batch, "rehydration", "llm-e2e")
        .expect("example LLM batch should translate to projection messages");

    let fixture = TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(move |ctx| {
            let client = ctx.nats_client().clone();
            let messages = messages.clone();
            Box::pin(async move {
                for (subject, payload) in messages {
                    client.publish(subject, payload.into()).await?;
                }
                client.flush().await?;
                Ok(())
            })
        }))
        .with_readiness_check(root_id.clone(), FOCUS_NODE_ID)
        .build()
        .await
        .expect("test fixture should start");

    let mut query_client = fixture.query_client();
    let context = query_client
        .get_context(GetContextRequest {
            root_node_id: ROOT_NODE_ID.to_string(),
            role: "developer".to_string(),
            token_budget: 4096,
            requested_scopes: vec!["graph".to_string(), "details".to_string()],
            depth: 2,
            max_tier: 0,
            rehydration_mode: 0,
        })
        .await
        .expect("get_context should succeed")
        .into_inner();

    let bundle = context.bundle.expect("bundle should exist");
    let role_bundle = bundle
        .bundles
        .first()
        .expect("single role bundle should exist");

    assert_eq!(bundle.root_node_id, ROOT_NODE_ID);
    assert!(
        role_bundle
            .neighbor_nodes
            .iter()
            .any(|node| node.node_id == FOCUS_NODE_ID && node.title == "Connection pool typo")
    );
    assert!(
        role_bundle
            .neighbor_nodes
            .iter()
            .any(|node| node.node_id == DECISION_NODE_ID)
    );
    assert!(
        context
            .rendered
            .as_ref()
            .expect("rendered context should exist")
            .content
            .contains("Connection pool typo")
    );

    let detail = query_client
        .get_node_detail(GetNodeDetailRequest {
            node_id: FOCUS_NODE_ID.to_string(),
        })
        .await
        .expect("get_node_detail should succeed")
        .into_inner();

    assert_eq!(
        detail.node.as_ref().map(|node| node.node_id.as_str()),
        Some(FOCUS_NODE_ID)
    );
    assert_eq!(
        detail.detail.as_ref().map(|node| node.detail.as_str()),
        Some(
            "The Helm values reduced maxConnections from 50 to 5. Error rate reached 23% and P95 rose to 2.4s within three minutes."
        )
    );

    fixture.shutdown().await.expect("fixture should shut down");
    debug_log("finished test llm_graph_batch_materializes_through_projection_runtime");
}

#[tokio::test]
async fn llm_graph_incremental_batches_materialize_medium_graph() {
    let _guard = container_test_guard().lock().await;
    debug_log("starting test llm_graph_incremental_batches_materialize_medium_graph");

    let batch_one = parse_llm_graph_batch(INCREMENTAL_BATCH_ONE_JSON)
        .expect("incremental batch one should parse");
    let batch_two = parse_llm_graph_batch(INCREMENTAL_BATCH_TWO_JSON)
        .expect("incremental batch two should parse");

    let batch_one_messages = llm_graph_to_projection_events(&batch_one, "rehydration", "llm-inc-1")
        .expect("incremental batch one should translate");
    let batch_two_messages = llm_graph_to_projection_events(&batch_two, "rehydration", "llm-inc-2")
        .expect("incremental batch two should translate");

    let fixture = TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(move |ctx| {
            let client = ctx.nats_client().clone();
            let messages = batch_one_messages.clone();
            Box::pin(async move {
                publish_messages(&client, &messages).await?;
                Ok(())
            })
        }))
        .with_readiness_check(INCREMENTAL_ROOT_NODE_ID, INCREMENTAL_INITIAL_FOCUS_NODE_ID)
        .build()
        .await
        .expect("incremental fixture should start");

    let mut query_client = fixture.query_client();
    let initial_context = get_context(&mut query_client, INCREMENTAL_ROOT_NODE_ID)
        .await
        .expect("initial get_context should succeed");
    let initial_bundle = initial_context
        .bundle
        .as_ref()
        .expect("initial bundle should exist");
    let initial_role_bundle = initial_bundle
        .bundles
        .first()
        .expect("initial role bundle should exist");

    assert_eq!(initial_bundle.root_node_id, INCREMENTAL_ROOT_NODE_ID);
    assert_eq!(initial_role_bundle.neighbor_nodes.len(), 5);
    assert_eq!(initial_role_bundle.relationships.len(), 5);
    assert_eq!(initial_role_bundle.node_details.len(), 3);
    assert!(
        initial_role_bundle
            .neighbor_nodes
            .iter()
            .any(|node| node.node_id == "task-rollback-pool-config")
    );

    let nats_client = async_nats::connect(fixture.nats_url())
        .await
        .expect("nats client should connect");
    publish_messages(&nats_client, &batch_two_messages)
        .await
        .expect("second wave should publish");

    let final_context = wait_for_graph_shape(
        query_client.clone(),
        INCREMENTAL_ROOT_NODE_ID,
        9,
        10,
        5,
        &[
            "decision-reroute-secondary",
            "task-rollback-pool-config",
            "finding-retry-storm",
            "decision-throttle-retries",
            INCREMENTAL_SECOND_WAVE_NODE_ID,
            "artifact-recovery-checklist",
        ],
    )
    .await;
    let final_bundle = final_context.bundle.expect("final bundle should exist");
    let final_role_bundle = final_bundle
        .bundles
        .first()
        .expect("final role bundle should exist");

    assert_eq!(final_bundle.root_node_id, INCREMENTAL_ROOT_NODE_ID);
    assert_eq!(final_role_bundle.neighbor_nodes.len(), 9);
    assert_eq!(final_role_bundle.relationships.len(), 10);
    assert_eq!(final_role_bundle.node_details.len(), 5);
    assert!(
        final_context
            .rendered
            .as_ref()
            .expect("final rendered context should exist")
            .content
            .contains("Retry storm amplified saturation")
    );
    assert!(
        final_context
            .rendered
            .as_ref()
            .expect("final rendered context should exist")
            .content
            .contains("Apply retry cap")
    );

    let detail = query_client
        .get_node_detail(GetNodeDetailRequest {
            node_id: INCREMENTAL_SECOND_WAVE_NODE_ID.to_string(),
        })
        .await
        .expect("second wave node detail should be queryable")
        .into_inner();

    assert_eq!(
        detail.node.as_ref().map(|node| node.node_id.as_str()),
        Some(INCREMENTAL_SECOND_WAVE_NODE_ID)
    );
    assert_eq!(
        detail.detail.as_ref().map(|node| node.detail.as_str()),
        Some(
            "Apply a retry cap of 2 with full jitter, then verify that DB wait time, request concurrency, and checkout p95 begin to normalize."
        )
    );

    fixture.shutdown().await.expect("fixture should shut down");
    debug_log("finished test llm_graph_incremental_batches_materialize_medium_graph");
}

async fn publish_messages(
    client: &async_nats::Client,
    messages: &[(String, Vec<u8>)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for (subject, payload) in messages {
        client
            .publish(subject.clone(), payload.clone().into())
            .await?;
    }
    client.flush().await?;
    Ok(())
}

fn container_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

async fn get_context(
    query_client: &mut kmp_proto::v1beta1::context_query_service_client::ContextQueryServiceClient<
        Channel,
    >,
    root_node_id: &str,
) -> Result<kmp_proto::v1beta1::GetContextResponse, tonic::Status> {
    query_client
        .get_context(GetContextRequest {
            root_node_id: root_node_id.to_string(),
            role: "developer".to_string(),
            token_budget: 4096,
            requested_scopes: vec!["graph".to_string(), "details".to_string()],
            depth: 3,
            max_tier: 0,
            rehydration_mode: 0,
        })
        .await
        .map(|response| response.into_inner())
}

async fn wait_for_graph_shape(
    mut query_client: kmp_proto::v1beta1::context_query_service_client::ContextQueryServiceClient<
        Channel,
    >,
    root_node_id: &str,
    expected_neighbors: usize,
    expected_relationships: usize,
    expected_details: usize,
    expected_node_ids: &[&str],
) -> kmp_proto::v1beta1::GetContextResponse {
    for _ in 0..40 {
        if let Ok(context) = get_context(&mut query_client, root_node_id).await
            && let Some(bundle) = context.bundle.as_ref()
            && let Some(role_bundle) = bundle.bundles.first()
        {
            let has_expected_nodes = expected_node_ids.iter().all(|expected| {
                role_bundle
                    .neighbor_nodes
                    .iter()
                    .any(|node| node.node_id == *expected)
            });

            if role_bundle.neighbor_nodes.len() == expected_neighbors
                && role_bundle.relationships.len() == expected_relationships
                && role_bundle.node_details.len() == expected_details
                && has_expected_nodes
            {
                return context;
            }
        }

        sleep(Duration::from_millis(200)).await;
    }

    panic!("incremental graph did not reach the expected shape before timeout");
}
