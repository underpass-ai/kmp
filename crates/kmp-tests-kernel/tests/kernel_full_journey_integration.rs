#![cfg(feature = "container-tests")]

use std::error::Error;

use kmp_proto::v1beta1::{
    CommandMetadata, ContextChange, ContextChangeOperation, GetContextRequest,
    GetNodeDetailRequest, UpdateContextRequest,
};
use kmp_tests_shared::fixtures::{TestFixture, wait_for_neighbor_count};
use kmp_tests_shared::ports::ClosureSeed;
use kmp_tests_shared::seed::kernel_data::DEVELOPER_ROLE;
use kmp_tests_shared::seed::kernel_e2e_data::{
    CHIEF_ENGINEER_TITLE, DECISION_DETAIL, DECISION_ID, EXPECTED_DETAIL_COUNT,
    EXPECTED_NEIGHBOR_COUNT, EXPECTED_RELATIONSHIP_COUNT, EXPECTED_SELECTED_NODE_COUNT,
    EXPECTED_SELECTED_RELATIONSHIP_COUNT, EXPLORER_CHECKLIST_ID, EXPLORER_CHECKLIST_TITLE,
    EXPLORER_LEAF_DETAIL, EXPLORER_LEAF_ID, EXPLORER_LEAF_TITLE, EXPLORER_WORKSTREAM_DETAIL,
    EXPLORER_WORKSTREAM_ID, EXPLORER_WORKSTREAM_TITLE, JUMP_DECISION_ID, POWER_TASK_ID,
    PROPULSION_SUBSYSTEM_TITLE, RELATION_DECISION_REQUIRES, RELATION_DEPENDS_ON, RELATION_IMPACTS,
    ROOT_DETAIL, ROOT_NODE_ID, ROOT_TITLE, TASK_DETAIL, TASK_ID,
    publish_kernel_e2e_projection_events,
};
use prost_types::Duration;

#[tokio::test]
async fn kernel_full_journey_covers_projection_query_and_command()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture = TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(|ctx| {
            let client = ctx.nats_client().clone();
            Box::pin(async move { publish_kernel_e2e_projection_events(&client).await })
        }))
        .with_readiness_check(ROOT_NODE_ID, TASK_ID)
        .build()
        .await?;

    let result = async {
        let mut query_client = fixture.query_client();
        let mut command_client = fixture.command_client();

        let shallow_query_context = query_client
            .get_context(GetContextRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                role: DEVELOPER_ROLE.to_string(),
                token_budget: 65536,
                requested_scopes: vec!["graph".to_string(), "decisions".to_string()],
                depth: 1,
                max_tier: 0,
                rehydration_mode: 2,
            })
            .await?
            .into_inner();
        let shallow_bundle = shallow_query_context
            .bundle
            .expect("shallow query bundle should exist");
        let shallow_role_bundle = &shallow_bundle.bundles[0];
        assert!(
            shallow_role_bundle
                .neighbor_nodes
                .iter()
                .any(|node| node.node_id == EXPLORER_WORKSTREAM_ID)
        );
        assert!(
            !shallow_role_bundle
                .neighbor_nodes
                .iter()
                .any(|node| node.node_id == EXPLORER_CHECKLIST_ID)
        );
        assert!(
            !shallow_role_bundle
                .neighbor_nodes
                .iter()
                .any(|node| node.node_id == EXPLORER_LEAF_ID)
        );
        assert!(
            !shallow_query_context
                .rendered
                .as_ref()
                .expect("shallow rendered context should exist")
                .content
                .contains(EXPLORER_LEAF_DETAIL)
        );

        // The fixture's readiness probe waits for *any* neighbour to appear,
        // which is the right gate for "the projection is alive" and the wrong
        // one for "it has caught up". Asserting an exact count straight after
        // it raced the projection: 15 of 17, everything else correct, and a
        // re-run with no change passing. Wait for what this test is about to
        // assert; the assertion itself stays exact.
        let query_request = GetContextRequest {
            root_node_id: ROOT_NODE_ID.to_string(),
            role: DEVELOPER_ROLE.to_string(),
            token_budget: 65536,
            requested_scopes: vec!["graph".to_string(), "decisions".to_string()],
            depth: 3,
            max_tier: 0,
            rehydration_mode: 2,
        };
        wait_for_neighbor_count(
            fixture.query_client(),
            query_request.clone(),
            EXPECTED_NEIGHBOR_COUNT,
            40,
        )
        .await?;

        let query_context = query_client.get_context(query_request).await?.into_inner();
        let query_bundle = query_context.bundle.expect("query bundle should exist");
        assert_eq!(query_bundle.root_node_id, ROOT_NODE_ID);
        assert_eq!(query_bundle.bundles.len(), 1);
        let query_role_bundle = &query_bundle.bundles[0];
        assert_eq!(query_role_bundle.role, DEVELOPER_ROLE);
        assert_eq!(
            query_role_bundle.neighbor_nodes.len(),
            EXPECTED_NEIGHBOR_COUNT
        );
        assert_eq!(
            query_role_bundle.relationships.len(),
            EXPECTED_RELATIONSHIP_COUNT
        );
        assert_eq!(query_role_bundle.node_details.len(), EXPECTED_DETAIL_COUNT);
        assert!(
            query_role_bundle
                .neighbor_nodes
                .iter()
                .any(|node| node.node_id == TASK_ID)
        );
        assert!(
            query_role_bundle
                .node_details
                .iter()
                .any(|detail| detail.node_id == TASK_ID)
        );
        assert!(
            query_role_bundle
                .neighbor_nodes
                .iter()
                .any(|node| node.node_id == EXPLORER_WORKSTREAM_ID)
        );
        assert!(
            query_role_bundle
                .neighbor_nodes
                .iter()
                .any(|node| node.node_id == EXPLORER_CHECKLIST_ID)
        );
        assert!(
            query_role_bundle
                .neighbor_nodes
                .iter()
                .any(|node| node.node_id == EXPLORER_LEAF_ID)
        );
        assert!(
            query_role_bundle
                .node_details
                .iter()
                .any(|detail| detail.node_id == EXPLORER_WORKSTREAM_ID)
        );
        assert!(
            query_role_bundle
                .node_details
                .iter()
                .any(|detail| detail.node_id == EXPLORER_LEAF_ID)
        );
        assert!(query_role_bundle.relationships.iter().any(|edge| {
            edge.source_node_id == DECISION_ID
                && edge.target_node_id == TASK_ID
                && edge.relationship_type == RELATION_IMPACTS
        }));
        assert!(query_role_bundle.relationships.iter().any(|edge| {
            edge.source_node_id == POWER_TASK_ID
                && edge.target_node_id == TASK_ID
                && edge.relationship_type == RELATION_DEPENDS_ON
        }));
        assert!(query_role_bundle.relationships.iter().any(|edge| {
            edge.source_node_id == JUMP_DECISION_ID
                && edge.target_node_id == DECISION_ID
                && edge.relationship_type == RELATION_DECISION_REQUIRES
        }));
        let rendered_query_context = query_context
            .rendered
            .expect("rendered context should exist");
        assert!(rendered_query_context.content.contains(ROOT_TITLE));
        assert!(rendered_query_context.content.contains(ROOT_DETAIL));
        assert!(rendered_query_context.content.contains(TASK_DETAIL));
        assert!(rendered_query_context.content.contains(DECISION_DETAIL));
        assert!(
            rendered_query_context
                .content
                .contains(PROPULSION_SUBSYSTEM_TITLE)
        );
        assert!(
            rendered_query_context
                .content
                .contains(CHIEF_ENGINEER_TITLE)
        );
        assert!(
            rendered_query_context
                .content
                .contains(EXPLORER_WORKSTREAM_TITLE)
        );
        assert!(
            rendered_query_context
                .content
                .contains(EXPLORER_WORKSTREAM_DETAIL)
        );
        assert!(
            rendered_query_context
                .content
                .contains(EXPLORER_CHECKLIST_TITLE)
        );
        assert!(rendered_query_context.content.contains(EXPLORER_LEAF_TITLE));
        assert!(
            rendered_query_context
                .content
                .contains(EXPLORER_LEAF_DETAIL)
        );

        let node_detail = query_client
            .get_node_detail(GetNodeDetailRequest {
                node_id: EXPLORER_LEAF_ID.to_string(),
            })
            .await?
            .into_inner();
        assert_eq!(
            node_detail.node.as_ref().map(|node| node.node_id.as_str()),
            Some(EXPLORER_LEAF_ID)
        );
        assert_eq!(
            node_detail.node.as_ref().map(|node| node.title.as_str()),
            Some(EXPLORER_LEAF_TITLE)
        );
        assert_eq!(
            node_detail
                .detail
                .as_ref()
                .map(|detail| detail.detail.as_str()),
            Some(EXPLORER_LEAF_DETAIL)
        );

        let zoomed_context = query_client
            .get_context(GetContextRequest {
                root_node_id: EXPLORER_WORKSTREAM_ID.to_string(),
                role: DEVELOPER_ROLE.to_string(),
                token_budget: 65536,
                requested_scopes: vec!["graph".to_string(), "details".to_string()],
                depth: 2,
                max_tier: 0,
                rehydration_mode: 2,
            })
            .await?
            .into_inner();
        let zoomed_bundle = zoomed_context.bundle.expect("zoomed bundle should exist");
        assert_eq!(zoomed_bundle.root_node_id, EXPLORER_WORKSTREAM_ID);
        let zoomed_role_bundle = &zoomed_bundle.bundles[0];
        assert_eq!(zoomed_role_bundle.neighbor_nodes.len(), 2);
        assert_eq!(zoomed_role_bundle.relationships.len(), 2);
        assert_eq!(zoomed_role_bundle.node_details.len(), 2);
        assert!(
            zoomed_role_bundle
                .neighbor_nodes
                .iter()
                .any(|node| node.node_id == EXPLORER_CHECKLIST_ID)
        );
        assert!(
            zoomed_role_bundle
                .neighbor_nodes
                .iter()
                .any(|node| node.node_id == EXPLORER_LEAF_ID)
        );
        let zoomed_rendered = zoomed_context
            .rendered
            .expect("zoomed rendered context should exist");
        assert!(zoomed_rendered.content.contains(EXPLORER_WORKSTREAM_TITLE));
        assert!(zoomed_rendered.content.contains(EXPLORER_WORKSTREAM_DETAIL));
        assert!(zoomed_rendered.content.contains(EXPLORER_CHECKLIST_TITLE));
        assert!(zoomed_rendered.content.contains(EXPLORER_LEAF_TITLE));
        assert!(zoomed_rendered.content.contains(EXPLORER_LEAF_DETAIL));
        assert!(!zoomed_rendered.content.contains(ROOT_TITLE));
        assert!(!zoomed_rendered.content.contains(ROOT_DETAIL));

        let query_rehydrate = query_client
            .rehydrate_session(kmp_proto::v1beta1::RehydrateSessionRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                roles: vec![DEVELOPER_ROLE.to_string()],
                persist_snapshot: true,
                timeline_window: 11,
                snapshot_ttl: Some(Duration {
                    seconds: 600,
                    nanos: 0,
                }),
            })
            .await?
            .into_inner();
        assert!(query_rehydrate.snapshot_persisted);
        assert_eq!(
            query_rehydrate.snapshot_id,
            format!("snapshot:{ROOT_NODE_ID}:{DEVELOPER_ROLE}")
        );
        let query_rehydrate_bundle = query_rehydrate
            .bundle
            .expect("rehydrated bundle should exist");
        assert_eq!(query_rehydrate_bundle.root_node_id, ROOT_NODE_ID);
        let query_rehydrate_stats = query_rehydrate_bundle
            .stats
            .as_ref()
            .expect("rehydrated stats should exist");
        assert_eq!(query_rehydrate_stats.nodes, EXPECTED_SELECTED_NODE_COUNT);
        assert_eq!(
            query_rehydrate_stats.relationships,
            EXPECTED_SELECTED_RELATIONSHIP_COUNT
        );
        assert_eq!(
            query_rehydrate_stats.detailed_nodes,
            EXPECTED_DETAIL_COUNT as u32
        );
        assert_eq!(query_rehydrate_stats.timeline_events, 11);
        assert_eq!(query_rehydrate_bundle.bundles.len(), 1);
        assert_eq!(
            query_rehydrate_bundle.bundles[0].neighbor_nodes.len(),
            EXPECTED_NEIGHBOR_COUNT
        );
        assert_eq!(
            query_rehydrate_bundle.bundles[0].relationships.len(),
            EXPECTED_RELATIONSHIP_COUNT
        );
        assert_eq!(
            query_rehydrate_bundle.bundles[0].node_details.len(),
            EXPECTED_DETAIL_COUNT
        );

        let leaf_rehydrate = query_client
            .rehydrate_session(kmp_proto::v1beta1::RehydrateSessionRequest {
                root_node_id: EXPLORER_LEAF_ID.to_string(),
                roles: vec![DEVELOPER_ROLE.to_string()],
                persist_snapshot: false,
                timeline_window: 0,
                snapshot_ttl: None,
            })
            .await?
            .into_inner();
        assert!(!leaf_rehydrate.snapshot_persisted);
        let leaf_bundle = leaf_rehydrate
            .bundle
            .expect("leaf rehydration bundle should exist");
        assert_eq!(leaf_bundle.root_node_id, EXPLORER_LEAF_ID);
        let leaf_stats = leaf_bundle.stats.as_ref().expect("leaf stats should exist");
        assert_eq!(leaf_stats.nodes, 1);
        assert_eq!(leaf_stats.relationships, 0);
        assert_eq!(leaf_stats.detailed_nodes, 1);
        let leaf_role_bundle = &leaf_bundle.bundles[0];
        assert!(leaf_role_bundle.neighbor_nodes.is_empty());
        assert!(leaf_role_bundle.relationships.is_empty());
        assert_eq!(leaf_role_bundle.node_details.len(), 1);
        assert_eq!(leaf_role_bundle.node_details[0].node_id, EXPLORER_LEAF_ID);
        assert_eq!(
            leaf_role_bundle.node_details[0].detail,
            EXPLORER_LEAF_DETAIL
        );

        // -- Multi-role rehydration: shared graph read, per-role bundles --
        let multi_role_rehydrate = query_client
            .rehydrate_session(kmp_proto::v1beta1::RehydrateSessionRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                roles: vec![
                    DEVELOPER_ROLE.to_string(),
                    "reviewer".to_string(),
                    "ops".to_string(),
                ],
                persist_snapshot: false,
                timeline_window: 0,
                snapshot_ttl: None,
            })
            .await?
            .into_inner();
        let multi_bundle = multi_role_rehydrate
            .bundle
            .expect("multi-role rehydrated bundle should exist");
        assert_eq!(multi_bundle.root_node_id, ROOT_NODE_ID);
        assert_eq!(
            multi_bundle.bundles.len(),
            3,
            "should produce one bundle per role"
        );
        // Each role bundle must have the same graph shape (shared read).
        for (i, role_bundle) in multi_bundle.bundles.iter().enumerate() {
            assert_eq!(
                role_bundle.neighbor_nodes.len(),
                EXPECTED_NEIGHBOR_COUNT,
                "role bundle {i} neighbor count mismatch"
            );
            assert_eq!(
                role_bundle.relationships.len(),
                EXPECTED_RELATIONSHIP_COUNT,
                "role bundle {i} relationship count mismatch"
            );
            assert_eq!(
                role_bundle.node_details.len(),
                EXPECTED_DETAIL_COUNT,
                "role bundle {i} detail count mismatch"
            );
        }
        // Roles are assigned correctly.
        assert_eq!(multi_bundle.bundles[0].role, DEVELOPER_ROLE);
        assert_eq!(multi_bundle.bundles[1].role, "reviewer");
        assert_eq!(multi_bundle.bundles[2].role, "ops");
        // Graph data is identical across roles (shared read, not re-fetched).
        let dev_node_ids: Vec<&str> = multi_bundle.bundles[0]
            .neighbor_nodes
            .iter()
            .map(|n| n.node_id.as_str())
            .collect();
        let reviewer_node_ids: Vec<&str> = multi_bundle.bundles[1]
            .neighbor_nodes
            .iter()
            .map(|n| n.node_id.as_str())
            .collect();
        assert_eq!(
            dev_node_ids, reviewer_node_ids,
            "neighbor nodes must be identical across roles"
        );

        let command_update = command_client
            .update_context(UpdateContextRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                role: DEVELOPER_ROLE.to_string(),
                work_item_id: TASK_ID.to_string(),
                changes: vec![ContextChange {
                    operation: ContextChangeOperation::Update as i32,
                    entity_kind: "node_detail".to_string(),
                    entity_id: TASK_ID.to_string(),
                    payload_json: "{\"status\":\"READY\"}".to_string(),
                    reason: "full-journey".to_string(),
                    scopes: vec!["graph".to_string(), "details".to_string()],
                }],
                metadata: Some(CommandMetadata {
                    idempotency_key: "idem-kernel-full-journey".to_string(),
                    correlation_id: "corr-kernel-full-journey".to_string(),
                    causation_id: "cause-kernel-full-journey".to_string(),
                    requested_by: "kernel-e2e".to_string(),
                    requested_at: None,
                }),
                precondition: None,
            })
            .await?
            .into_inner();
        assert_eq!(
            command_update
                .accepted_version
                .as_ref()
                .map(|version| version.revision),
            Some(1)
        );
        assert!(command_update.warnings.is_empty());

        Ok::<(), Box<dyn Error + Send + Sync>>(())
    }
    .await;

    fixture.shutdown().await?;
    result
}
