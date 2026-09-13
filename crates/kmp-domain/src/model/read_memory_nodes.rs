use std::collections::BTreeSet;

use super::trace_read_budget::TraceReadBudget;
use crate::{
    MemoryNodeHeader, MemoryNodesRequest, MemoryNodesResult, PortError, RelationDirection,
    TemporalCoordinate, TraceSnapshotReader,
};

/// Resolve a bounded reference set using one borrowed store snapshot. No source
/// discovery, body reads, temporal cut or ranking is implied by a framing read.
pub fn read_memory_nodes<R: TraceSnapshotReader>(
    reader: &R,
    request: &MemoryNodesRequest,
) -> Result<MemoryNodesResult, PortError> {
    request
        .validate()
        .map_err(|e| PortError::InvalidState(e.to_string()))?;
    let mut budget = TraceReadBudget::new(reader, request.limits());
    let mut result = MemoryNodesResult::default();
    let mut seen = BTreeSet::new();
    for reference in &request.refs {
        if !seen.insert(reference) {
            continue;
        }
        if budget.stop.is_some() {
            result.omitted.push(reference.clone());
            continue;
        }
        let Some(node) = budget.node(reference)? else {
            if budget.stop.is_some() {
                result.omitted.push(reference.clone());
            } else {
                result.missing.push(reference.clone());
            }
            continue;
        };
        if node.properties.get("memory_about") != Some(&request.about) {
            result.missing.push(reference.clone());
            continue;
        }
        let mut coordinates = Vec::new();
        let mut after = None;
        let complete = loop {
            let Some(page) = budget.page(
                reference,
                RelationDirection::Incoming,
                after,
                Some("contains_entry"),
            )?
            else {
                break false;
            };
            for edge in page.edges {
                if edge.target_node_id != *reference || edge.relation_type != "contains_entry" {
                    return Err(PortError::InvalidState(
                        "node coordinate page returned the wrong endpoint or type".into(),
                    ));
                }
                if let Some(coordinate) =
                    TemporalCoordinate::from_relation_explanation(&edge.explanation)
                        .map_err(|e| PortError::InvalidState(e.to_string()))?
                {
                    coordinates.push(coordinate);
                }
            }
            if page.exhausted {
                break true;
            }
            after = page.next;
        };
        result.nodes.push(MemoryNodeHeader {
            node,
            coordinates,
            coordinates_complete: complete,
        });
    }
    result.stop = budget.stop;
    result.scanned_edges = budget.scanned;
    Ok(result)
}
