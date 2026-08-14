use kmp_ports::{NodeDetailProjection, PortError};
use serde_json::json;

pub(crate) fn serialize_node_detail(detail: &NodeDetailProjection) -> Result<String, PortError> {
    serde_json::to_string(&json!({
        "node_id": detail.node_id,
        "detail": detail.detail,
        "content_hash": detail.content_hash,
        "revision": detail.revision,
    }))
    .map_err(|error| {
        PortError::InvalidState(format!(
            "node detail could not be serialized for valkey: {error}"
        ))
    })
}

pub(crate) fn deserialize_node_detail(
    payload: &str,
) -> Result<Option<NodeDetailProjection>, PortError> {
    #[derive(serde::Deserialize)]
    struct RawNodeDetailProjection {
        node_id: String,
        detail: String,
        content_hash: String,
        revision: u64,
    }

    serde_json::from_str::<RawNodeDetailProjection>(payload)
        .map(|detail| {
            Some(NodeDetailProjection {
                node_id: detail.node_id,
                detail: detail.detail,
                content_hash: detail.content_hash,
                revision: detail.revision,
            })
        })
        .map_err(|error| {
            PortError::InvalidState(format!(
                "node detail could not be deserialized from valkey: {error}"
            ))
        })
}
