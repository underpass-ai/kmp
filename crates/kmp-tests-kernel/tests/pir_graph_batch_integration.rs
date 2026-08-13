#![cfg(feature = "container-tests")]

use std::sync::OnceLock;

use kmp_proto::v1beta1::{GetContextRequest, GetNodeDetailRequest};
use kmp_testkit::{graph_batch_to_projection_events, parse_graph_batch};
use kmp_tests_shared::debug::debug_log;
use kmp_tests_shared::fixtures::TestFixture;
use kmp_tests_shared::ports::ClosureSeed;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};
use tonic::transport::Channel;

const INCIDENT_ROOT_NODE_ID: &str = "incident:pir-2026-04-09-payments-latency";
const FIRST_FINDING_NODE_ID: &str = "finding:pir-2026-04-09-payments-latency:db-pool-typo";
const SECOND_WAVE_FINDING_NODE_ID: &str = "finding:pir-2026-04-09-payments-latency:retry-storm";
const SECOND_WAVE_TASK_NODE_ID: &str = "task:pir-2026-04-09-payments-latency:apply-retry-cap";

const INCIDENT_BATCH_JSON: &str =
    include_str!("../../../api/examples/kernel/v1beta1/async/incident-graph-batch.json");
const INCIDENT_BATCH_INCREMENTAL_TWO_JSON: &str = include_str!(
    "../../../api/examples/kernel/v1beta1/async/incident-graph-batch.incremental-2.json"
);
const INCIDENT_BATCH_INCREMENTAL_THREE_JSON: &str = include_str!(
    "../../../api/examples/kernel/v1beta1/async/incident-graph-batch.incremental-3.json"
);

#[tokio::test]
async fn pir_graph_batch_republish_with_same_run_id_is_idempotent() {
    let _guard = container_test_guard().lock().await;
    debug_log("starting test pir_graph_batch_republish_with_same_run_id_is_idempotent");

    let batch = parse_graph_batch(INCIDENT_BATCH_JSON).expect("incident batch should parse");
    let messages = graph_batch_to_projection_events(&batch, "rehydration", "pir-wave-1")
        .expect("incident batch should translate");
    let seed_messages = messages.clone();

    let fixture = TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(move |ctx| {
            let client = ctx.nats_client().clone();
            let messages = seed_messages.clone();
            Box::pin(async move {
                publish_messages(&client, &messages).await?;
                Ok(())
            })
        }))
        .with_readiness_check(INCIDENT_ROOT_NODE_ID, FIRST_FINDING_NODE_ID)
        .build()
        .await
        .expect("fixture should start");

    let mut query_client = fixture.query_client();
    let initial_context = get_context(&mut query_client, INCIDENT_ROOT_NODE_ID)
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

    assert_eq!(initial_bundle.root_node_id, INCIDENT_ROOT_NODE_ID);
    assert_eq!(initial_role_bundle.neighbor_nodes.len(), 2);
    assert_eq!(initial_role_bundle.relationships.len(), 2);
    assert_eq!(initial_role_bundle.node_details.len(), 2);

    let nats_client = async_nats::connect(fixture.nats_url())
        .await
        .expect("nats client should connect");
    publish_messages(&nats_client, &messages)
        .await
        .expect("republish should succeed");

    sleep(Duration::from_millis(600)).await;

    let repeated_context = wait_for_graph_shape(
        query_client.clone(),
        INCIDENT_ROOT_NODE_ID,
        2,
        2,
        2,
        &[
            FIRST_FINDING_NODE_ID,
            "decision:pir-2026-04-09-payments-latency:reroute-secondary",
        ],
    )
    .await;
    let repeated_bundle = repeated_context
        .bundle
        .expect("repeated bundle should exist");
    let repeated_role_bundle = repeated_bundle
        .bundles
        .first()
        .expect("repeated role bundle should exist");

    assert_eq!(repeated_role_bundle.neighbor_nodes.len(), 2);
    assert_eq!(repeated_role_bundle.relationships.len(), 2);
    assert_eq!(repeated_role_bundle.node_details.len(), 2);

    let detail = query_client
        .get_node_detail(GetNodeDetailRequest {
            node_id: FIRST_FINDING_NODE_ID.to_string(),
        })
        .await
        .expect("detail should stay queryable")
        .into_inner();

    assert_eq!(
        detail
            .detail
            .as_ref()
            .map(|response| (response.revision, response.detail.as_str())),
        Some((
            1,
            "The rollout 2026.04.09.3 changed DB maxConnections from 50 to 5. Error rate hit 18% and p95 rose to 2.3 seconds within four minutes."
        ))
    );

    fixture.shutdown().await.expect("fixture should shut down");
    debug_log("finished test pir_graph_batch_republish_with_same_run_id_is_idempotent");
}

#[tokio::test]
async fn pir_graph_batch_incremental_waves_expand_the_same_incident() {
    let _guard = container_test_guard().lock().await;
    debug_log("starting test pir_graph_batch_incremental_waves_expand_the_same_incident");

    let batch_one = parse_graph_batch(INCIDENT_BATCH_JSON).expect("incident batch should parse");
    let batch_two = parse_graph_batch(INCIDENT_BATCH_INCREMENTAL_TWO_JSON)
        .expect("second wave batch should parse");

    let batch_one_messages =
        graph_batch_to_projection_events(&batch_one, "rehydration", "pir-wave-1")
            .expect("first wave should translate");
    let batch_two_messages =
        graph_batch_to_projection_events(&batch_two, "rehydration", "pir-wave-2")
            .expect("second wave should translate");

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
        .with_readiness_check(INCIDENT_ROOT_NODE_ID, FIRST_FINDING_NODE_ID)
        .build()
        .await
        .expect("fixture should start");

    let mut query_client = fixture.query_client();
    let initial_context = get_context(&mut query_client, INCIDENT_ROOT_NODE_ID)
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

    assert_eq!(initial_role_bundle.neighbor_nodes.len(), 2);
    assert_eq!(initial_role_bundle.relationships.len(), 2);
    assert_eq!(initial_role_bundle.node_details.len(), 2);

    let nats_client = async_nats::connect(fixture.nats_url())
        .await
        .expect("nats client should connect");
    publish_messages(&nats_client, &batch_two_messages)
        .await
        .expect("second wave should publish");

    let final_context = wait_for_graph_shape(
        query_client.clone(),
        INCIDENT_ROOT_NODE_ID,
        4,
        4,
        4,
        &[
            FIRST_FINDING_NODE_ID,
            "decision:pir-2026-04-09-payments-latency:reroute-secondary",
            SECOND_WAVE_FINDING_NODE_ID,
            SECOND_WAVE_TASK_NODE_ID,
        ],
    )
    .await;
    let final_bundle = final_context.bundle.expect("final bundle should exist");
    let final_role_bundle = final_bundle
        .bundles
        .first()
        .expect("final role bundle should exist");
    let root_node = final_role_bundle
        .root_node
        .as_ref()
        .expect("root node should exist");

    assert_eq!(final_bundle.root_node_id, INCIDENT_ROOT_NODE_ID);
    assert_eq!(final_role_bundle.neighbor_nodes.len(), 4);
    assert_eq!(final_role_bundle.relationships.len(), 4);
    assert_eq!(final_role_bundle.node_details.len(), 4);
    assert_eq!(root_node.status, "MITIGATING");
    assert!(
        final_context
            .rendered
            .as_ref()
            .expect("rendered context should exist")
            .content
            .contains("Retry storm amplified load")
    );
    assert!(
        final_context
            .rendered
            .as_ref()
            .expect("rendered context should exist")
            .content
            .contains("Apply retry cap")
    );

    let detail = query_client
        .get_node_detail(GetNodeDetailRequest {
            node_id: SECOND_WAVE_TASK_NODE_ID.to_string(),
        })
        .await
        .expect("second wave task detail should be queryable")
        .into_inner();

    assert_eq!(
        detail.node.as_ref().map(|node| node.node_id.as_str()),
        Some(SECOND_WAVE_TASK_NODE_ID)
    );
    assert_eq!(
        detail.node.as_ref().map(|node| node.status.as_str()),
        Some("PLANNED")
    );
    assert_eq!(
        detail.detail.as_ref().map(|detail| detail.detail.as_str()),
        Some(
            "Set retry cap to 2 with full jitter and monitor DB wait time, request concurrency, and p95 until rollback and reroute complete."
        )
    );

    fixture.shutdown().await.expect("fixture should shut down");
    debug_log("finished test pir_graph_batch_incremental_waves_expand_the_same_incident");
}

#[tokio::test]
async fn pir_graph_batch_corrective_wave_updates_existing_nodes_without_graph_growth() {
    let _guard = container_test_guard().lock().await;
    debug_log(
        "starting test pir_graph_batch_corrective_wave_updates_existing_nodes_without_graph_growth",
    );

    let batch_one = parse_graph_batch(INCIDENT_BATCH_JSON).expect("incident batch should parse");
    let batch_two = parse_graph_batch(INCIDENT_BATCH_INCREMENTAL_TWO_JSON)
        .expect("second wave batch should parse");
    let batch_three = parse_graph_batch(INCIDENT_BATCH_INCREMENTAL_THREE_JSON)
        .expect("third wave batch should parse");

    let batch_one_messages =
        graph_batch_to_projection_events(&batch_one, "rehydration", "pir-wave-1")
            .expect("first wave should translate");
    let batch_two_messages =
        graph_batch_to_projection_events(&batch_two, "rehydration", "pir-wave-2")
            .expect("second wave should translate");
    let batch_three_messages =
        graph_batch_to_projection_events(&batch_three, "rehydration", "pir-wave-3")
            .expect("third wave should translate");

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
        .with_readiness_check(INCIDENT_ROOT_NODE_ID, FIRST_FINDING_NODE_ID)
        .build()
        .await
        .expect("fixture should start");

    let query_client = fixture.query_client();
    let nats_client = async_nats::connect(fixture.nats_url())
        .await
        .expect("nats client should connect");

    publish_messages(&nats_client, &batch_two_messages)
        .await
        .expect("second wave should publish");
    wait_for_graph_shape(
        query_client.clone(),
        INCIDENT_ROOT_NODE_ID,
        4,
        4,
        4,
        &[
            FIRST_FINDING_NODE_ID,
            "decision:pir-2026-04-09-payments-latency:reroute-secondary",
            SECOND_WAVE_FINDING_NODE_ID,
            SECOND_WAVE_TASK_NODE_ID,
        ],
    )
    .await;

    publish_messages(&nats_client, &batch_three_messages)
        .await
        .expect("third wave should publish");

    let final_context =
        wait_for_corrective_context(query_client.clone(), INCIDENT_ROOT_NODE_ID).await;
    let final_bundle = final_context.bundle.expect("final bundle should exist");
    let final_role_bundle = final_bundle
        .bundles
        .first()
        .expect("final role bundle should exist");
    let root_node = final_role_bundle
        .root_node
        .as_ref()
        .expect("root node should exist");

    assert_eq!(final_bundle.root_node_id, INCIDENT_ROOT_NODE_ID);
    assert_eq!(final_role_bundle.neighbor_nodes.len(), 4);
    assert_eq!(final_role_bundle.relationships.len(), 4);
    assert_eq!(final_role_bundle.node_details.len(), 4);
    assert_eq!(root_node.status, "STABILIZING");
    assert!(
        root_node.summary.contains(
            "returned below 1.1 seconds after rollback, reroute, and the retry-cap rollout"
        )
    );

    let retry_cap_task = final_role_bundle
        .neighbor_nodes
        .iter()
        .find(|node| node.node_id == SECOND_WAVE_TASK_NODE_ID)
        .expect("retry-cap task node should exist");
    assert_eq!(retry_cap_task.status, "COMPLETED");
    assert!(
        retry_cap_task
            .summary
            .contains("Retry cap of 2 with full jitter was rolled out")
    );

    let rendered = final_context
        .rendered
        .as_ref()
        .expect("rendered context should exist")
        .content
        .clone();
    assert!(rendered.contains("returned below 1.1 seconds"));
    assert!(rendered.contains("retry-cap change was completed"));
    assert!(!rendered.contains(
        "Set retry cap to 2 with full jitter and monitor DB wait time, request concurrency, and p95 until rollback and reroute complete."
    ));

    let task_detail =
        wait_for_node_detail_revision(query_client.clone(), SECOND_WAVE_TASK_NODE_ID, 2).await;

    assert_eq!(
        task_detail.detail.as_ref().map(|detail| detail.revision),
        Some(2)
    );
    assert_eq!(
        task_detail
            .detail
            .as_ref()
            .map(|detail| detail.detail.as_str()),
        Some(
            "The retry-cap change was completed with limit 2 and full jitter. Request concurrency and DB wait time fell back toward normal within minutes."
        )
    );

    let finding_detail =
        wait_for_node_detail_revision(query_client.clone(), SECOND_WAVE_FINDING_NODE_ID, 2).await;

    assert_eq!(
        finding_detail.detail.as_ref().map(|detail| detail.revision),
        Some(2)
    );
    assert_eq!(
        finding_detail
            .detail
            .as_ref()
            .map(|detail| detail.detail.as_str()),
        Some(
            "The retry storm remained the secondary amplifier of the incident. After the retry-cap rollout, request concurrency fell back toward baseline and queue growth stopped."
        )
    );

    fixture.shutdown().await.expect("fixture should shut down");
    debug_log(
        "finished test pir_graph_batch_corrective_wave_updates_existing_nodes_without_graph_growth",
    );
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
            role: "incident-commander".to_string(),
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

    panic!("incident graph did not reach the expected shape before timeout");
}

async fn wait_for_corrective_context(
    mut query_client: kmp_proto::v1beta1::context_query_service_client::ContextQueryServiceClient<
        Channel,
    >,
    root_node_id: &str,
) -> kmp_proto::v1beta1::GetContextResponse {
    for _ in 0..40 {
        if let Ok(context) = get_context(&mut query_client, root_node_id).await
            && let Some(bundle) = context.bundle.as_ref()
            && let Some(role_bundle) = bundle.bundles.first()
        {
            let root_is_corrected = role_bundle.root_node.as_ref().is_some_and(|node| {
                node.status == "STABILIZING" && node.summary.contains("returned below 1.1 seconds")
            });
            let task_is_corrected = role_bundle
                .neighbor_nodes
                .iter()
                .any(|node| node.node_id == SECOND_WAVE_TASK_NODE_ID && node.status == "COMPLETED");

            if role_bundle.neighbor_nodes.len() == 4
                && role_bundle.relationships.len() == 4
                && role_bundle.node_details.len() == 4
                && root_is_corrected
                && task_is_corrected
            {
                return context;
            }
        }

        sleep(Duration::from_millis(200)).await;
    }

    panic!("incident graph did not reach the corrective state before timeout");
}

async fn wait_for_node_detail_revision(
    mut query_client: kmp_proto::v1beta1::context_query_service_client::ContextQueryServiceClient<
        Channel,
    >,
    node_id: &str,
    expected_revision: u64,
) -> kmp_proto::v1beta1::GetNodeDetailResponse {
    for _ in 0..40 {
        if let Ok(detail) = query_client
            .get_node_detail(GetNodeDetailRequest {
                node_id: node_id.to_string(),
            })
            .await
            .map(|response| response.into_inner())
            && detail
                .detail
                .as_ref()
                .is_some_and(|value| value.revision == expected_revision)
        {
            return detail;
        }

        sleep(Duration::from_millis(200)).await;
    }

    panic!("node detail `{node_id}` did not reach revision {expected_revision} before timeout");
}
