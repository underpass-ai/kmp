use std::collections::BTreeMap;

use crate::{BundleNode, DomainError, KmpBundle, TemporalAxis, TemporalCoordinate};

use super::axis_key::TemporalAxisKey;
use super::position::TemporalPosition;

pub(super) fn temporal_positions(
    bundle: &KmpBundle,
    axis: TemporalAxis,
) -> Result<Vec<TemporalPosition>, DomainError> {
    let mut positions = Vec::new();

    for relationship in bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
    {
        let Some(coordinate) =
            TemporalCoordinate::from_relation_explanation(relationship.explanation())?
        else {
            continue;
        };
        let ref_id = relationship.target_node_id().to_string();
        for axis_key in TemporalAxisKey::from_coordinate(&ref_id, &coordinate, axis) {
            positions.push(TemporalPosition {
                ref_id: ref_id.clone(),
                coordinate: coordinate.clone(),
                axis_key,
            });
        }
    }

    Ok(positions)
}

pub(super) fn bundle_nodes_by_id(bundle: &KmpBundle) -> BTreeMap<&str, &BundleNode> {
    std::iter::once(bundle.root_node())
        .chain(bundle.neighbor_nodes().iter())
        .map(|node| (node.node_id(), node))
        .collect()
}
