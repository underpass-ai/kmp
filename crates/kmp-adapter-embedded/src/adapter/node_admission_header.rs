use super::super::engine::{Key, ReadTx, Table};
use super::super::serdes::decode;
use kmp_domain::{NodeProjection, PortError};
use serde::Deserialize;
use std::collections::BTreeMap;

/// Boundary DTO for graph admission. Large source prose and arbitrary payload
/// metadata stay out of the projection: serde skips every field it does not
/// name without allocating it, so the node's stored JSON is read once, in
/// Rust, and only the header is kept.
#[derive(Deserialize)]
struct NodeAdmissionHeader {
    node_id: String,
    node_kind: String,
    status: String,
    labels: Vec<String>,
    properties: Option<AdmittedProperties>,
    title: Option<String>,
    summary: Option<String>,
}

/// The one payload property a node is admitted with.
#[derive(Deserialize)]
struct AdmittedProperties {
    placeholder: Option<String>,
}

pub(super) fn read(tx: &dyn ReadTx, id: &str) -> Result<Option<NodeProjection>, PortError> {
    let Some(raw) = tx.get(Table::Nodes, Key::Str(id))? else {
        return Ok(None);
    };
    let header: NodeAdmissionHeader = decode("node admission header", &raw)?;
    let (title, summary) = if matches!(header.node_kind.as_str(), "memory_evidence" | "evidence") {
        (String::new(), String::new())
    } else {
        match (header.title, header.summary) {
            (Some(title), Some(summary)) => (title, summary),
            _ => {
                return Err(PortError::InvalidState(format!(
                    "node `{id}` has no title or summary to admit"
                )));
            }
        }
    };
    Ok(Some(NodeProjection {
        node_id: header.node_id,
        node_kind: header.node_kind,
        status: header.status,
        labels: header.labels,
        title,
        summary,
        properties: header
            .properties
            .and_then(|properties| properties.placeholder)
            .map(|value| BTreeMap::from([("placeholder".into(), value)]))
            .unwrap_or_default(),
        provenance: None,
    }))
}

#[cfg(test)]
#[path = "node_admission_header_tests.rs"]
mod tests;
