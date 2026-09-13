use serde_json::{Map, Value, json};

use kmp_proto::v1beta1::ReadNodesResponse;

use super::rendering::temporal_coordinate_json;

/// One framing batch as the app reads it: scoped headers and the coordinates
/// found for each, never a canonical body. Refs this about does not hold are
/// `missing`; refs the work budget left unread are `omitted`; headers whose
/// coordinate walk was cut short are named in `incomplete_coordinates`, so a
/// browser can refuse to frame them. Coordinates spell their clocks the way
/// every other MCP coordinate does.
pub(crate) fn memory_nodes_from_response(response: ReadNodesResponse) -> Value {
    let mut nodes = Vec::new();
    let mut coordinates = Map::new();
    let mut incomplete = Vec::new();
    for header in response.nodes {
        let Some(node) = header.node else {
            continue;
        };
        if !header.coordinates_complete {
            incomplete.push(node.node_id.clone());
        }
        coordinates.insert(
            node.node_id.clone(),
            Value::Array(
                header
                    .coordinates
                    .iter()
                    .map(temporal_coordinate_json)
                    .collect(),
            ),
        );
        nodes.push(json!({
            "id": node.node_id,
            "kind": node.node_kind,
            "title": node.title,
            "summary": node.summary,
            "status": node.status,
            "labels": node.labels,
            "properties": node.properties,
        }));
    }
    json!({
        "snapshot": response.snapshot,
        "nodes": nodes,
        "coordinates": coordinates,
        "missing": response.missing,
        "omitted": response.omitted,
        "incomplete_coordinates": incomplete,
        "stop_reason": response.stop_reason,
        "scanned_edges": response.scanned_edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_proto::v1beta1::{GraphNode, MemoryNodeHeader, TemporalCoordinate};
    use prost_types::Timestamp;

    #[test]
    fn a_batch_names_partial_headers_and_spells_clocks_like_every_other_coordinate() {
        let result = memory_nodes_from_response(ReadNodesResponse {
            snapshot: "sqlite-observer:nonce:7".into(),
            nodes: vec![MemoryNodeHeader {
                node: Some(GraphNode {
                    node_id: "a".into(),
                    node_kind: "decision".into(),
                    ..Default::default()
                }),
                coordinates: vec![TemporalCoordinate {
                    dimension: "task".into(),
                    scope_id: "task:1".into(),
                    observed_at: Some(Timestamp {
                        seconds: 0,
                        nanos: 123_456_789,
                    }),
                    ..Default::default()
                }],
                coordinates_complete: false,
            }],
            missing: vec!["gone".into()],
            omitted: vec!["b".into()],
            stop_reason: "edge_budget".into(),
            scanned_edges: 1,
        });
        assert_eq!(result["nodes"][0]["id"], "a");
        assert_eq!(result["nodes"][0]["kind"], "decision");
        assert_eq!(result["incomplete_coordinates"], json!(["a"]));
        assert_eq!(result["missing"], json!(["gone"]));
        assert_eq!(result["omitted"], json!(["b"]));
        assert_eq!(result["stop_reason"], "edge_budget");
        assert_eq!(result["snapshot"], "sqlite-observer:nonce:7");
        let coordinate = &result["coordinates"]["a"][0];
        assert_eq!(coordinate["observed_at"], "1970-01-01T00:00:00.123456789Z");
        assert!(
            coordinate.get("occurred_at").is_none(),
            "absent clocks stay absent"
        );
        assert!(
            coordinate.get("method").is_none(),
            "absent origins stay absent"
        );
    }
}
