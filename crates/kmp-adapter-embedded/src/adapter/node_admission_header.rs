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
    title: Option<String>,
    summary: Option<String>,
}

/// The fields one statement projects: the header, and the title and summary
/// every node but evidence is admitted with. Reading them in the same
/// statement halves the statements a catalogue issues per node; evidence
/// keeps neither, exactly as when they were read apart.
const FIELDS: [&str; 7] = [
    "node_id",
    "node_kind",
    "status",
    "labels",
    "properties.placeholder",
    "title",
    "summary",
];

pub(super) fn read(tx: &dyn ReadTx, id: &str) -> Result<Option<NodeProjection>, PortError> {
    let Some(raw) = tx.project_str_json(Table::Nodes, id, &FIELDS)? else {
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
            .placeholder
            .map(|value| BTreeMap::from([("placeholder".into(), value)]))
            .unwrap_or_default(),
        provenance: None,
    }))
}
