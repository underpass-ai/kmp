use std::collections::BTreeMap;

use crate::model::KmpBundle;
use crate::value_objects::EntryLabels;

/// The labels every entry in a bundle stands in, keyed by entry ref: one
/// `contains_entry` edge is one coordinate, its `dimension` the key and its
/// `scope_id` the value. An entry with no edge has no labels and is absent
/// from the map; a selector reads that as the empty map.
pub fn labels_by_entry(bundle: &KmpBundle) -> BTreeMap<String, EntryLabels> {
    let mut coordinates = BTreeMap::<String, Vec<(String, String)>>::new();
    for relationship in bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
    {
        let explanation = relationship.explanation();
        let (Some(dimension), Some(scope_id)) = (explanation.dimension(), explanation.scope_id())
        else {
            continue;
        };
        coordinates
            .entry(relationship.target_node_id().to_string())
            .or_default()
            .push((dimension.to_string(), scope_id.to_string()));
    }
    coordinates
        .into_iter()
        .map(|(ref_id, coordinates)| {
            let labels = EntryLabels::from_coordinates(
                coordinates
                    .iter()
                    .map(|(dimension, scope_id)| (dimension.as_str(), scope_id.as_str())),
            );
            (ref_id, labels)
        })
        .collect()
}
