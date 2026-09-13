use super::bundle_views::proto_coordinate_from_domain;
use super::scalars::{ProtoMappingResult, invalid_argument};
use kmp_domain::{GraphReadRevision, MemoryNodesRequest, MemoryNodesResult};
use kmp_proto::v1beta1::{GraphNode, MemoryNodeHeader, ReadNodesRequest, ReadNodesResponse};

pub fn memory_nodes_request_from_proto(
    request: ReadNodesRequest,
) -> ProtoMappingResult<MemoryNodesRequest> {
    let expect_snapshot = if request.expect_snapshot.is_empty() {
        None
    } else {
        Some(
            GraphReadRevision::new(request.expect_snapshot)
                .map_err(|e| invalid_argument(e.to_string()))?,
        )
    };
    let request = MemoryNodesRequest {
        expect_snapshot,
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
        snapshot: result
            .snapshot
            .map_or_else(String::new, |revision| revision.as_str().to_owned()),
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
