use super::super::engine::{ReadTx, Table};
use super::super::serdes::decode;
use kmp_domain::{NodeProjection, PortError};
use serde::Deserialize;
use std::collections::BTreeMap;

/// Boundary DTO for graph admission. Large source prose and arbitrary payload
/// metadata stay in SQLite until the application admits their node identities.
#[derive(Deserialize)]
struct NodeAdmissionHeader {
    node_id: String,
    node_kind: String,
    status: String,
    labels: Vec<String>,
    #[serde(rename = "properties.placeholder")]
    placeholder: Option<String>,
}

pub(super) fn read(tx: &dyn ReadTx, id: &str) -> Result<Option<NodeProjection>, PortError> {
    let Some(raw) = tx.project_str_json(
        Table::Nodes,
        id,
        &[
            "node_id",
            "node_kind",
            "status",
            "labels",
            "properties.placeholder",
        ],
    )?
    else {
        return Ok(None);
    };
    let header: NodeAdmissionHeader = decode("node admission header", &raw)?;
    let mut text = if matches!(header.node_kind.as_str(), "memory_evidence" | "evidence") {
        BTreeMap::new()
    } else {
        let raw = tx
            .project_str_json(Table::Nodes, id, &["title", "summary"])?
            .ok_or_else(|| {
                PortError::InvalidState("node vanished inside read transaction".into())
            })?;
        decode::<BTreeMap<String, String>>("node summary header", &raw)?
    };
    Ok(Some(NodeProjection {
        node_id: header.node_id,
        node_kind: header.node_kind,
        status: header.status,
        labels: header.labels,
        title: text.remove("title").unwrap_or_default(),
        summary: text.remove("summary").unwrap_or_default(),
        properties: header
            .placeholder
            .map(|value| BTreeMap::from([("placeholder".into(), value)]))
            .unwrap_or_default(),
        provenance: None,
    }))
}
