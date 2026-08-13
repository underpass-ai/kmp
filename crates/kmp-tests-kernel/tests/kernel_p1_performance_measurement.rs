#![cfg(feature = "container-tests")]

use std::error::Error;
use std::time::Instant;

use kmp_tests_shared::fixtures::TestFixture;
use kmp_tests_shared::ports::ClosureSeed;
use kmp_tests_shared::seed::kernel_data::DEVELOPER_ROLE;
use kmp_tests_shared::seed::kernel_e2e_data::{
    ROOT_NODE_ID, TASK_ID, publish_kernel_e2e_projection_events,
};

/// Measures the P1 optimization impact by comparing:
///   - 3x single-role RehydrateSession (old path: repeated graph reads)
///   - 1x multi-role RehydrateSession (new path: shared graph read)
///
/// Both paths exercise real Neo4j graph reads and real Valkey MGET/GET detail loads.
/// The timing breakdown in the proto response shows the per-phase durations.
#[tokio::test]
async fn p1_performance_measurement_multi_role_vs_repeated_single_role()
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
        let roles = vec![
            DEVELOPER_ROLE.to_string(),
            "reviewer".to_string(),
            "ops".to_string(),
        ];

        // -- Warmup: discard first call to avoid cold-start skew --
        let _ = query_client
            .rehydrate_session(kmp_proto::v1beta1::RehydrateSessionRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                roles: vec![DEVELOPER_ROLE.to_string()],
                persist_snapshot: false,
                timeline_window: 0,
                snapshot_ttl: None,
            })
            .await?;

        // -- Baseline: 3x single-role calls (simulates pre-P1 behavior) --
        let baseline_start = Instant::now();
        let mut baseline_responses = Vec::new();
        for role in &roles {
            let resp = query_client
                .rehydrate_session(kmp_proto::v1beta1::RehydrateSessionRequest {
                    root_node_id: ROOT_NODE_ID.to_string(),
                    roles: vec![role.clone()],
                    persist_snapshot: false,
                    timeline_window: 0,
                    snapshot_ttl: None,
                })
                .await?
                .into_inner();
            baseline_responses.push(resp);
        }
        let baseline_total_ms = baseline_start.elapsed().as_secs_f64() * 1000.0;

        // -- Optimized: 1x multi-role call (P1 shared-read path) --
        let optimized_start = Instant::now();
        let optimized = query_client
            .rehydrate_session(kmp_proto::v1beta1::RehydrateSessionRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                roles: roles.clone(),
                persist_snapshot: false,
                timeline_window: 0,
                snapshot_ttl: None,
            })
            .await?
            .into_inner();
        let optimized_total_ms = optimized_start.elapsed().as_secs_f64() * 1000.0;

        // -- Extract timing breakdown from optimized response --
        let timing = optimized
            .timing
            .as_ref()
            .expect("optimized response should contain timing breakdown");

        // -- Sum baseline per-call timings --
        let baseline_graph_sum_ms: f64 = baseline_responses
            .iter()
            .filter_map(|r| r.timing.as_ref())
            .map(|t| t.graph_load_seconds * 1000.0)
            .sum();
        let baseline_detail_sum_ms: f64 = baseline_responses
            .iter()
            .filter_map(|r| r.timing.as_ref())
            .map(|t| t.detail_load_seconds * 1000.0)
            .sum();

        // -- Report --
        let optimized_graph_ms = timing.graph_load_seconds * 1000.0;
        let optimized_detail_ms = timing.detail_load_seconds * 1000.0;
        let optimized_assembly_ms = timing.bundle_assembly_seconds * 1000.0;

        eprintln!();
        eprintln!("P1 PERFORMANCE MEASUREMENT (3 roles)");
        eprintln!("=========================================================");
        eprintln!("BASELINE (3x single-role calls):");
        eprintln!("  Wall clock:         {:>8.2} ms", baseline_total_ms);
        eprintln!("  Graph load sum:     {:>8.2} ms  (3 reads)", baseline_graph_sum_ms);
        eprintln!("  Detail load sum:    {:>8.2} ms  (3 batches)", baseline_detail_sum_ms);
        eprintln!("OPTIMIZED (1x multi-role call):");
        eprintln!("  Wall clock:         {:>8.2} ms", optimized_total_ms);
        eprintln!("  Graph load:         {:>8.2} ms  (1 read)", optimized_graph_ms);
        eprintln!("  Detail load:        {:>8.2} ms  (1 batch, MGET)", optimized_detail_ms);
        eprintln!("  Bundle assembly:    {:>8.2} ms  ({} roles)", optimized_assembly_ms, timing.role_count);
        eprintln!("  Batch size:         {:>8} nodes", timing.batch_size);
        eprintln!("IMPROVEMENT:");
        eprintln!("  Wall clock saved:   {:>8.2} ms ({:.0}%)",
            baseline_total_ms - optimized_total_ms,
            if baseline_total_ms > 0.0 { (1.0 - optimized_total_ms / baseline_total_ms) * 100.0 } else { 0.0 });
        eprintln!("  Graph reads saved:  {:>8.2} ms (2 reads eliminated)",
            baseline_graph_sum_ms - optimized_graph_ms);
        eprintln!("  Detail loads saved: {:>8.2} ms (2 batches eliminated)",
            baseline_detail_sum_ms - optimized_detail_ms);
        eprintln!("=========================================================");
        eprintln!();

        // -- Assertions: correctness --
        let opt_bundle = optimized.bundle.expect("bundle");
        assert_eq!(opt_bundle.bundles.len(), 3, "should produce 3 role bundles");
        let first_neighbors = opt_bundle.bundles[0].neighbor_nodes.len();
        for (i, role_bundle) in opt_bundle.bundles.iter().enumerate() {
            assert_eq!(
                role_bundle.neighbor_nodes.len(),
                first_neighbors,
                "role {i}: neighbor count must be identical across roles (shared read)"
            );
        }

        // -- Assertion: timing is populated --
        assert!(timing.graph_load_seconds > 0.0, "graph load should be measurable");
        assert!(timing.role_count == 3, "should report 3 roles");
        assert!(timing.batch_size > 0, "batch size should be > 0");

        // -- Assertion: multi-role should be faster than 3x single --
        assert!(
            optimized_total_ms < baseline_total_ms * 1.1,
            "multi-role ({optimized_total_ms:.2}ms) should not be slower than 3x single ({baseline_total_ms:.2}ms)"
        );

        Ok::<(), Box<dyn Error + Send + Sync>>(())
    }
    .await;

    fixture.shutdown().await?;
    result
}
