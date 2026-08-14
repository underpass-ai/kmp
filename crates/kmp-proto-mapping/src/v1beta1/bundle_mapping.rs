use kmp_application::{QueryTimingBreakdown, RehydrateSessionResult, RenderedContext};
use kmp_domain::KmpBundle;
use kmp_proto::v1beta1::{
    GraphRoleBundle, QueryTimingBreakdown as ProtoTimingBreakdown, RehydrateSessionResponse,
    RehydrationBundle as ProtoBundle, RehydrationStats,
};

use super::rendered_mapping::proto_rendered_context_v1beta1;

use crate::v1beta1::support::timestamp_from;
use crate::v1beta1::{
    proto_bundle_node_detail_v1beta1, proto_bundle_node_v1beta1, proto_bundle_relationship_v1beta1,
    proto_bundle_version_v1beta1,
};

pub fn proto_rehydrate_session_response_v1beta1(
    result: &RehydrateSessionResult,
) -> RehydrateSessionResponse {
    RehydrateSessionResponse {
        bundle: Some(ProtoBundle {
            root_node_id: result.root_node_id.clone(),
            bundles: result
                .bundles
                .iter()
                .zip(
                    result
                        .rendered_contexts
                        .iter()
                        .map(Some)
                        .chain(std::iter::repeat(None)),
                )
                .map(|(bundle, rendered)| {
                    proto_graph_role_bundle_with_rendered_v1beta1(bundle, rendered)
                })
                .collect(),
            stats: Some(proto_session_stats_v1beta1(result)),
            version: Some(proto_bundle_version_v1beta1(&result.version)),
        }),
        snapshot_persisted: result.snapshot_persisted,
        snapshot_id: result.snapshot_id.clone().unwrap_or_default(),
        generated_at: Some(timestamp_from(result.generated_at)),
        timing: result.timing.as_ref().map(proto_timing_breakdown_v1beta1),
    }
}

pub fn proto_timing_breakdown_v1beta1(timing: &QueryTimingBreakdown) -> ProtoTimingBreakdown {
    ProtoTimingBreakdown {
        graph_load_seconds: timing.graph_load.as_secs_f64(),
        detail_load_seconds: timing.detail_load.as_secs_f64(),
        bundle_assembly_seconds: timing.bundle_assembly.as_secs_f64(),
        role_count: timing.role_count as u32,
        batch_size: timing.batch_size as u32,
    }
}

pub fn proto_bundle_from_single_role_v1beta1(bundle: &KmpBundle) -> ProtoBundle {
    ProtoBundle {
        root_node_id: bundle.root_node_id().as_str().to_string(),
        bundles: vec![proto_graph_role_bundle_v1beta1(bundle)],
        stats: Some(proto_bundle_stats_v1beta1(bundle, 0, 1)),
        version: Some(proto_bundle_version_v1beta1(bundle.metadata())),
    }
}

fn proto_graph_role_bundle_v1beta1(bundle: &KmpBundle) -> GraphRoleBundle {
    proto_graph_role_bundle_with_rendered_v1beta1(bundle, None)
}

fn proto_graph_role_bundle_with_rendered_v1beta1(
    bundle: &KmpBundle,
    rendered: Option<&RenderedContext>,
) -> GraphRoleBundle {
    GraphRoleBundle {
        role: bundle.role().as_str().to_string(),
        root_node: Some(proto_bundle_node_v1beta1(bundle.root_node())),
        neighbor_nodes: bundle
            .neighbor_nodes()
            .iter()
            .map(proto_bundle_node_v1beta1)
            .collect(),
        relationships: bundle
            .relationships()
            .iter()
            .map(proto_bundle_relationship_v1beta1)
            .collect(),
        node_details: bundle
            .node_details()
            .iter()
            .map(proto_bundle_node_detail_v1beta1)
            .collect(),
        rendered: rendered.map(|r| proto_rendered_context_v1beta1(r, &[])),
    }
}

fn proto_session_stats_v1beta1(result: &RehydrateSessionResult) -> RehydrationStats {
    let nodes = result
        .bundles
        .iter()
        .map(|bundle| bundle.stats().selected_nodes())
        .sum();
    let relationships = result
        .bundles
        .iter()
        .map(|bundle| bundle.stats().selected_relationships())
        .sum();
    let detailed_nodes = result
        .bundles
        .iter()
        .map(|bundle| bundle.stats().detailed_nodes())
        .sum();

    RehydrationStats {
        roles: result.bundles.len() as u32,
        nodes,
        relationships,
        detailed_nodes,
        timeline_events: result.timeline_events,
    }
}

fn proto_bundle_stats_v1beta1(
    bundle: &KmpBundle,
    timeline_events: u32,
    roles: u32,
) -> RehydrationStats {
    RehydrationStats {
        roles,
        nodes: bundle.stats().selected_nodes(),
        relationships: bundle.stats().selected_relationships(),
        detailed_nodes: bundle.stats().detailed_nodes(),
        timeline_events,
    }
}
