use std::collections::BTreeMap;

use crate::{BundleNode, DomainError, KmpBundle, TemporalAxis, TemporalCoordinate};

use super::axis_key::{TemporalAxisKey, entry_time_key};
use super::position::TemporalPosition;

pub(super) fn temporal_positions(
    bundle: &KmpBundle,
    axis: TemporalAxis,
) -> Result<Vec<TemporalPosition>, DomainError> {
    let coordinates = entry_coordinates(bundle)?;
    let entry_times = entry_time_keys(&coordinates, axis);
    let mut positions = Vec::new();
    for (ref_id, coordinate) in coordinates {
        let entry_time = entry_times.get(&ref_id);
        for axis_key in
            TemporalAxisKey::from_entry_coordinate(&ref_id, &coordinate, axis, entry_time)
        {
            positions.push(TemporalPosition {
                ref_id: ref_id.clone(),
                coordinate: coordinate.clone(),
                axis_key,
            });
        }
    }

    Ok(positions)
}

/// Every `contains_entry` coordinate of the bundle, with the entry it places.
pub(super) fn entry_coordinates(
    bundle: &KmpBundle,
) -> Result<Vec<(String, TemporalCoordinate)>, DomainError> {
    let mut coordinates = Vec::new();
    for relationship in bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
    {
        if let Some(coordinate) =
            TemporalCoordinate::from_relation_explanation(relationship.explanation())?
        {
            coordinates.push((relationship.target_node_id().to_string(), coordinate));
        }
    }
    Ok(coordinates)
}

/// Each entry's own position on the clock, for the coordinates that carry
/// no time there and stand where their entry stands.
pub(super) fn entry_time_keys(
    coordinates: &[(String, TemporalCoordinate)],
    axis: TemporalAxis,
) -> BTreeMap<String, TemporalAxisKey> {
    let mut by_entry = BTreeMap::<&str, Vec<&TemporalCoordinate>>::new();
    for (ref_id, coordinate) in coordinates {
        by_entry.entry(ref_id).or_default().push(coordinate);
    }
    by_entry
        .into_iter()
        .filter_map(|(ref_id, coordinates)| {
            Some((ref_id.to_string(), entry_time_key(coordinates, axis)?))
        })
        .collect()
}

pub(super) fn bundle_nodes_by_id(bundle: &KmpBundle) -> BTreeMap<&str, &BundleNode> {
    std::iter::once(bundle.root_node())
        .chain(bundle.neighbor_nodes().iter())
        .map(|node| (node.node_id(), node))
        .collect()
}
