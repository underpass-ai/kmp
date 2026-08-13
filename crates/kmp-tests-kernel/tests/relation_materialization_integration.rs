#![cfg(feature = "container-tests")]

use std::sync::OnceLock;

use kmp_proto::v1beta1::GetContextRequest;
use kmp_tests_shared::debug::debug_log;
use kmp_tests_shared::fixtures::TestFixture;
use kmp_tests_shared::ports::ClosureSeed;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};
use tonic::transport::Channel;

const ROOT_NODE_ID: &str = "incident:pir-2026-04-14-cache-stampede";
const FINDING_NODE_ID: &str = "finding:pir-2026-04-14-cache-stampede:stampede";
const DECISION_NODE_ID: &str = "decision:pir-2026-04-14-cache-stampede:enable-jitter";

#[tokio::test]
async fn relation_materialized_events_extend_a_pir_like_spine_without_rematerializing_the_source_node()
 {
    let _guard = container_test_guard().lock().await;
    debug_log(
        "starting test relation_materialized_events_extend_a_pir_like_spine_without_rematerializing_the_source_node",
    );

    let seed_messages = projection_messages();

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
        .with_readiness_check(ROOT_NODE_ID, FINDING_NODE_ID)
        .build()
        .await
        .expect("fixture should start");

    let context = wait_for_relation_shape(fixture.query_client(), ROOT_NODE_ID).await;
    let bundle = context.bundle.expect("bundle should exist");
    let role_bundle = bundle.bundles.first().expect("role bundle should exist");

    assert_eq!(bundle.root_node_id, ROOT_NODE_ID);
    assert_eq!(role_bundle.neighbor_nodes.len(), 2);
    assert_eq!(role_bundle.relationships.len(), 3);
    assert!(
        role_bundle
            .neighbor_nodes
            .iter()
            .any(|node| node.node_id == FINDING_NODE_ID)
    );
    assert!(
        role_bundle
            .neighbor_nodes
            .iter()
            .any(|node| node.node_id == DECISION_NODE_ID)
    );
    assert!(
        role_bundle.relationships.iter().any(|relationship| {
            relationship.source_node_id == DECISION_NODE_ID
                && relationship.target_node_id == FINDING_NODE_ID
                && relationship.relationship_type == "ADDRESSES"
        }),
        "rendered bundle should include the relation-only spine edge",
    );
    assert!(
        context
            .rendered
            .as_ref()
            .expect("rendered context should exist")
            .content
            .contains("Enable cache jitter"),
    );

    fixture.shutdown().await.expect("fixture should shut down");
    debug_log(
        "finished test relation_materialized_events_extend_a_pir_like_spine_without_rematerializing_the_source_node",
    );
}

fn projection_messages() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "rehydration.graph.node.materialized".to_string(),
            serde_json::to_vec(&root_node_payload()).expect("root payload should serialize"),
        ),
        (
            "rehydration.graph.node.materialized".to_string(),
            serde_json::to_vec(&finding_node_payload()).expect("finding payload should serialize"),
        ),
        (
            "rehydration.node.detail.materialized".to_string(),
            serde_json::to_vec(&finding_detail_payload()).expect("finding detail should serialize"),
        ),
        (
            "rehydration.graph.node.materialized".to_string(),
            serde_json::to_vec(&decision_node_payload())
                .expect("decision payload should serialize"),
        ),
        (
            "rehydration.node.detail.materialized".to_string(),
            serde_json::to_vec(&decision_detail_payload())
                .expect("decision detail should serialize"),
        ),
        (
            "rehydration.graph.relation.materialized".to_string(),
            serde_json::to_vec(&relation_payload()).expect("relation payload should serialize"),
        ),
    ]
}

fn root_node_payload() -> Value {
    json!({
        "event_id": "evt-root",
        "correlation_id": "corr-root",
        "causation_id": "cmd-root",
        "occurred_at": "2026-04-14T19:00:00Z",
        "aggregate_id": ROOT_NODE_ID,
        "aggregate_type": "node",
        "schema_version": "v1beta1",
        "data": {
            "node_id": ROOT_NODE_ID,
            "node_kind": "incident",
            "title": "Payments cache stampede",
            "summary": "Cache stampede increased checkout latency after invalidation behavior changed.",
            "status": "ACTIVE",
            "labels": ["incident", "payments"],
            "properties": {"service": "payments-api"},
            "related_nodes": [
                {
                    "node_id": FINDING_NODE_ID,
                    "relation_type": "HAS_FINDING",
                    "explanation": {
                        "semantic_class": "evidential",
                        "sequence": 1
                    }
                },
                {
                    "node_id": DECISION_NODE_ID,
                    "relation_type": "MITIGATED_BY",
                    "explanation": {
                        "semantic_class": "procedural",
                        "sequence": 2
                    }
                }
            ]
        }
    })
}

fn finding_node_payload() -> Value {
    json!({
        "event_id": "evt-finding",
        "correlation_id": "corr-root",
        "causation_id": "cmd-root",
        "occurred_at": "2026-04-14T19:00:05Z",
        "aggregate_id": FINDING_NODE_ID,
        "aggregate_type": "node",
        "schema_version": "v1beta1",
        "data": {
            "node_id": FINDING_NODE_ID,
            "node_kind": "finding",
            "title": "Cache stampede on invalidation",
            "summary": "Synchronized invalidation drove miss-rate spikes and elevated checkout p95.",
            "status": "ACTIVE",
            "labels": ["finding"],
            "properties": {"family": "cache-stampede"},
            "related_nodes": []
        }
    })
}

fn finding_detail_payload() -> Value {
    json!({
        "event_id": "evt-finding-detail",
        "correlation_id": "corr-root",
        "causation_id": "cmd-root",
        "occurred_at": "2026-04-14T19:00:06Z",
        "aggregate_id": FINDING_NODE_ID,
        "aggregate_type": "node",
        "schema_version": "v1beta1",
        "data": {
            "node_id": FINDING_NODE_ID,
            "detail": "Redis miss rate jumped from 4% to 38% immediately after the invalidation rollout. Checkout p95 rose above 2.1s.",
            "content_hash": "hash-finding-1",
            "revision": 1
        }
    })
}

fn decision_node_payload() -> Value {
    json!({
        "event_id": "evt-decision",
        "correlation_id": "corr-root",
        "causation_id": "cmd-root",
        "occurred_at": "2026-04-14T19:00:10Z",
        "aggregate_id": DECISION_NODE_ID,
        "aggregate_type": "node",
        "schema_version": "v1beta1",
        "data": {
            "node_id": DECISION_NODE_ID,
            "node_kind": "decision",
            "title": "Enable cache jitter",
            "summary": "Introduce jittered invalidation to desynchronize misses while keeping rollback reversible.",
            "status": "PROPOSED",
            "labels": ["decision", "fix-planning"],
            "properties": {"stage": "fix_planning"},
            "related_nodes": []
        }
    })
}

fn decision_detail_payload() -> Value {
    json!({
        "event_id": "evt-decision-detail",
        "correlation_id": "corr-root",
        "causation_id": "cmd-root",
        "occurred_at": "2026-04-14T19:00:11Z",
        "aggregate_id": DECISION_NODE_ID,
        "aggregate_type": "node",
        "schema_version": "v1beta1",
        "data": {
            "node_id": DECISION_NODE_ID,
            "detail": "Apply a bounded jitter window to cache invalidation, then verify miss rate and checkout p95 before widening rollout.",
            "content_hash": "hash-decision-1",
            "revision": 1
        }
    })
}

fn relation_payload() -> Value {
    json!({
        "event_id": "evt-relation",
        "correlation_id": "corr-root",
        "causation_id": "cmd-root",
        "occurred_at": "2026-04-14T19:00:12Z",
        "aggregate_id": format!("relation:{DECISION_NODE_ID}|ADDRESSES|{FINDING_NODE_ID}"),
        "aggregate_type": "node_relation",
        "schema_version": "v1beta1",
        "data": {
            "source_node_id": DECISION_NODE_ID,
            "target_node_id": FINDING_NODE_ID,
            "relation_type": "ADDRESSES",
            "explanation": {
                "semantic_class": "causal",
                "rationale": "the mitigation decision targets the cache stampede finding directly",
                "decision_id": DECISION_NODE_ID,
                "sequence": 3
            }
        }
    })
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

async fn wait_for_relation_shape(
    mut query_client: kmp_proto::v1beta1::context_query_service_client::ContextQueryServiceClient<
        Channel,
    >,
    root_node_id: &str,
) -> kmp_proto::v1beta1::GetContextResponse {
    for _ in 0..40 {
        if let Ok(context) = get_context(&mut query_client, root_node_id).await
            && let Some(bundle) = context.bundle.as_ref()
            && let Some(role_bundle) = bundle.bundles.first()
            && role_bundle.neighbor_nodes.len() == 2
            && role_bundle.relationships.len() == 3
            && role_bundle.relationships.iter().any(|relationship| {
                relationship.source_node_id == DECISION_NODE_ID
                    && relationship.target_node_id == FINDING_NODE_ID
                    && relationship.relationship_type == "ADDRESSES"
            })
        {
            return context;
        }

        sleep(Duration::from_millis(250)).await;
    }

    panic!("timed out waiting for relation-only spine edge to appear");
}
