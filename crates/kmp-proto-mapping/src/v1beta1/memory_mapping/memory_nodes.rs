use super::bundle_views::proto_coordinate_from_domain;
use super::scalars::{ProtoMappingResult, invalid_argument};
use kmp_domain::{MemoryNodesRequest, MemoryNodesResult};
use kmp_proto::v1beta1::{GraphNode, MemoryNodeHeader, ReadNodesRequest, ReadNodesResponse};
use serde_json::{Value, json};

pub fn memory_nodes_request_from_proto(
    request: ReadNodesRequest,
) -> ProtoMappingResult<MemoryNodesRequest> {
    let request = MemoryNodesRequest {
        expect_snapshot: (!request.expect_snapshot.is_empty()).then_some(request.expect_snapshot),
        about: request.about,
        refs: request.refs,
        max_edges: if request.max_edges == 0 {
            2048
        } else {
            request.max_edges
        },
    };
    request
        .validate()
        .map_err(|e| invalid_argument(e.to_string()))?;
    Ok(request)
}

pub fn memory_nodes_response_from_result(result: MemoryNodesResult) -> ReadNodesResponse {
    ReadNodesResponse {
        snapshot: result.snapshot.unwrap_or_default(),
        nodes: result
            .nodes
            .into_iter()
            .map(|header| MemoryNodeHeader {
                node: Some(GraphNode {
                    node_id: header.node.node_id,
                    node_kind: header.node.node_kind,
                    title: header.node.title,
                    summary: header.node.summary,
                    status: header.node.status,
                    labels: header.node.labels,
                    properties: header.node.properties.into_iter().collect(),
                    provenance: None,
                }),
                coordinates: header
                    .coordinates
                    .iter()
                    .map(proto_coordinate_from_domain)
                    .collect(),
                coordinates_complete: header.coordinates_complete,
            })
            .collect(),
        missing: result.missing,
        omitted: result.omitted,
        stop_reason: result.stop.map_or("complete", |s| s.as_str()).into(),
        scanned_edges: result.scanned_edges,
    }
}

/// Shared HTTP/MCP-App representation; metadata only, never complete proof.
pub fn memory_nodes_json(response: ReadNodesResponse) -> Value {
    let mut nodes = Vec::new();
    let mut coordinates = serde_json::Map::new();
    let mut incomplete = Vec::new();
    for header in response.nodes {
        let Some(node) = header.node else {
            continue;
        };
        if !header.coordinates_complete {
            incomplete.push(node.node_id.clone());
        }
        let values: Vec<_> = header
            .coordinates
            .iter()
            .map(|c| {
                json!({
                    "dimension":c.dimension, "scope_id":c.scope_id,
                    "occurred_at":c.occurred_at.map(|v|v.to_string()),
                    "observed_at":c.observed_at.map(|v|v.to_string()),
                    "ingested_at":c.ingested_at.map(|v|v.to_string()),
                    "valid_from":c.valid_from.map(|v|v.to_string()),
                    "valid_until":c.valid_until.map(|v|v.to_string()),
                    "sequence":c.sequence, "rank":c.rank,
                    "method":c.method, "why":c.why, "motivation":c.motivation,
                })
            })
            .collect();
        coordinates.insert(node.node_id.clone(), json!(values));
        nodes.push(json!({"id":node.node_id,"kind":node.node_kind,"title":node.title,"summary":node.summary,"status":node.status,"labels":node.labels,"properties":node.properties}));
    }
    json!({"snapshot":response.snapshot,"nodes":nodes,"missing":response.missing,"coordinates":coordinates,
        "omitted":response.omitted,"incomplete_coordinates":incomplete,
        "stop_reason":response.stop_reason,"scanned_edges":response.scanned_edges})
}
