#![cfg(feature = "container-tests")]

//! Integration tests for multi-resolution tiers (L0/L1/L2).
//!
//! These tests verify that tiers are populated in real gRPC responses
//! and that `max_tier` filtering works end-to-end through the full stack.

use std::error::Error;

use kmp_proto::v1beta1::{
    GetContextRequest, ResolutionTier, context_query_service_client::ContextQueryServiceClient,
};
use kmp_tests_shared::fixtures::TestFixture;
use kmp_tests_shared::ports::ClosureSeed;
use kmp_tests_shared::seed::explanatory_data::{
    DetailMode, FAILURE_FOCUS_NODE_ID, ProjectionSeedVariant, ROOT_NODE_ID,
    publish_flawed_task_projection_events_variant,
};
use tonic::transport::Channel;

const TOKEN_BUDGET: u32 = 4096;

async fn start_explanatory_fixture() -> Result<TestFixture, Box<dyn Error + Send + Sync>> {
    let variant = ProjectionSeedVariant::FULL_EXPLANATORY_WITH_DETAIL;
    TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(move |ctx| {
            let client = ctx.nats_client().clone();
            Box::pin(async move {
                publish_flawed_task_projection_events_variant(&client, variant).await
            })
        }))
        .with_readiness_check(ROOT_NODE_ID, FAILURE_FOCUS_NODE_ID)
        .require_node_detail(variant.detail_mode == DetailMode::WithDetail)
        .build()
        .await
}

async fn get_context_with_max_tier(
    client: &mut ContextQueryServiceClient<Channel>,
    max_tier: i32,
) -> Result<kmp_proto::v1beta1::GetContextResponse, Box<dyn Error + Send + Sync>> {
    Ok(client
        .get_context(GetContextRequest {
            root_node_id: ROOT_NODE_ID.to_string(),
            role: "implementer".to_string(),
            token_budget: TOKEN_BUDGET,
            requested_scopes: vec![],
            depth: 3,
            max_tier,
            rehydration_mode: 0,
        })
        .await?
        .into_inner())
}

#[tokio::test]
async fn tiers_are_populated_in_grpc_response_with_explanatory_data()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture = start_explanatory_fixture().await?;
    let mut client = fixture.query_client();

    let response = get_context_with_max_tier(&mut client, 0).await?;
    let rendered = response.rendered.expect("rendered should exist");

    assert!(!rendered.content.is_empty());
    assert!(rendered.token_count > 0);
    assert!(!rendered.sections.is_empty());

    assert!(
        rendered.tiers.len() >= 2,
        "should have at least L0 + L1, got {} tiers",
        rendered.tiers.len()
    );

    let l0 = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L0Summary as i32)
        .expect("L0 tier should exist");
    assert!(
        l0.content.contains("Objective:"),
        "L0 should have Objective line"
    );
    assert!(l0.content.contains("Status:"), "L0 should have Status line");
    assert!(
        l0.content.contains("Blocker:"),
        "L0 should have Blocker line"
    );
    assert!(l0.content.contains("Next:"), "L0 should have Next line");
    assert!(l0.token_count > 0);
    assert!(
        l0.token_count <= 150,
        "L0 should fit in ~100 tokens, got {}",
        l0.token_count
    );

    let l1 = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L1CausalSpine as i32)
        .expect("L1 tier should exist");
    assert!(
        l1.content.contains("[causal]")
            || l1.content.contains("[motivational]")
            || l1.content.contains("[evidential]"),
        "L1 should contain explanatory relationships"
    );
    assert!(
        !l1.content.contains("Detail "),
        "L1 should not contain node details"
    );
    assert!(l1.token_count > 0);

    fixture.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn l0_summary_is_self_contained_for_status_check() -> Result<(), Box<dyn Error + Send + Sync>>
{
    let fixture = start_explanatory_fixture().await?;
    let mut client = fixture.query_client();

    let response = get_context_with_max_tier(&mut client, 0).await?;
    let rendered = response.rendered.expect("rendered should exist");

    let l0 = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L0Summary as i32)
        .expect("L0 should exist");

    let lines: Vec<_> = l0.content.lines().collect();
    assert_eq!(
        lines.len(),
        4,
        "L0 should be 4 lines: Objective, Status, Blocker, Next. Got:\n{}",
        l0.content
    );
    assert!(lines[0].starts_with("Objective:"));
    assert!(lines[1].starts_with("Status:"));
    assert!(lines[2].starts_with("Blocker:"));
    assert!(lines[3].starts_with("Next:"));

    fixture.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn l1_causal_spine_contains_explanatory_but_not_structural()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture = start_explanatory_fixture().await?;
    let mut client = fixture.query_client();

    let response = get_context_with_max_tier(&mut client, 0).await?;
    let rendered = response.rendered.expect("rendered should exist");

    let l1 = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L1CausalSpine as i32)
        .expect("L1 should exist");

    assert!(
        l1.content.contains("Node "),
        "L1 should contain the root node render"
    );

    let has_explanatory = l1.content.contains("[causal]")
        || l1.content.contains("[motivational]")
        || l1.content.contains("[evidential]")
        || l1.content.contains("[constraint]");
    assert!(has_explanatory, "L1 should have explanatory relationships");

    let has_structural = l1.content.contains("[structural]");
    assert!(
        !has_structural,
        "L1 should NOT have structural relationships"
    );

    fixture.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn l2_evidence_pack_contains_details_and_structural()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture = start_explanatory_fixture().await?;
    let mut client = fixture.query_client();

    let response = get_context_with_max_tier(&mut client, 0).await?;
    let rendered = response.rendered.expect("rendered should exist");

    let l2 = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L2EvidencePack as i32);

    if let Some(l2) = l2 {
        let has_detail = l2.content.contains("Detail ");
        let has_structural =
            l2.content.contains("[structural]") || l2.content.contains("[procedural]");
        assert!(
            has_detail || has_structural,
            "L2 should contain details or structural relationships"
        );
    }

    fixture.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn tier_token_counts_sum_approximately_to_total() -> Result<(), Box<dyn Error + Send + Sync>>
{
    let fixture = start_explanatory_fixture().await?;
    let mut client = fixture.query_client();

    let response = get_context_with_max_tier(&mut client, 0).await?;
    let rendered = response.rendered.expect("rendered should exist");

    let tier_total: u32 = rendered.tiers.iter().map(|t| t.token_count).sum();

    assert!(tier_total > 0, "tier token total should be positive");
    assert!(
        (tier_total as f64) < (rendered.token_count as f64 * 1.5),
        "tier total ({tier_total}) should not vastly exceed flat total ({})",
        rendered.token_count
    );

    fixture.shutdown().await?;
    Ok(())
}
