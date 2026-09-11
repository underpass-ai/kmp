use std::collections::BTreeMap;

use crate::{
    DomainError, EntryLabels, KmpBundle, TemporalAxis, TemporalCoordinate, TemporalDirection,
    labels_by_entry,
};

use super::TemporalTraversalResult;
use super::axis_key::{TemporalAxisKey, TemporalKeyKind};

impl TemporalTraversalResult {
    /// Keep label assertions inside the proof's clock boundary before applying
    /// whole-entry predicates. Antecedents may predate an enumeration interval;
    /// its exclusive end, or Goto's stricter inclusive cursor, bounds proof.
    pub fn proof_labels(
        &self,
        bundle: &KmpBundle,
    ) -> Result<BTreeMap<String, EntryLabels>, DomainError> {
        let mut boundary = self
            .interval()
            .and_then(|interval| interval.end())
            .map(|end| (TemporalAxisKey::time(end), false));
        if self.direction() == TemporalDirection::Goto
            && let Some(cursor) = self.resolved_cursor()
            && let Some(at) = TemporalAxisKey::from_coordinate("", cursor, self.axis())
                .into_iter()
                .find(|key| key.axis() == TemporalKeyKind::Time)
            && boundary.as_ref().is_none_or(|(end, _)| at < *end)
        {
            boundary = Some((at, true));
        }
        let Some((end, inclusive)) = boundary else {
            return Ok(labels_by_entry(bundle));
        };
        let mut memberships = BTreeMap::<String, Vec<(String, String)>>::new();
        for edge in bundle
            .relationships()
            .iter()
            .filter(|edge| edge.relationship_type() == "contains_entry")
        {
            let Some(coordinate) =
                TemporalCoordinate::from_relation_explanation(edge.explanation())?
            else {
                continue;
            };
            if admits(&coordinate, self.axis(), &end, inclusive) {
                memberships
                    .entry(edge.target_node_id().to_string())
                    .or_default()
                    .push((
                        coordinate.dimension().to_string(),
                        coordinate.scope_id().to_string(),
                    ));
            }
        }
        Ok(memberships
            .into_iter()
            .map(|(id, coords)| {
                (
                    id,
                    EntryLabels::from_coordinates(
                        coords
                            .iter()
                            .map(|(key, value)| (key.as_str(), value.as_str())),
                    ),
                )
            })
            .collect())
    }
}

fn admits(
    coordinate: &TemporalCoordinate,
    axis: TemporalAxis,
    end: &TemporalAxisKey,
    inclusive: bool,
) -> bool {
    if axis == TemporalAxis::Validity {
        if coordinate.valid_from().is_none() && coordinate.valid_until().is_none() {
            return false;
        }
        let started = coordinate.valid_from().is_none_or(|start| {
            let start = TemporalAxisKey::time(start);
            start < *end || (inclusive && start == *end)
        });
        return started
            && (!inclusive
                || coordinate
                    .valid_until()
                    .is_none_or(|until| TemporalAxisKey::time(until) > *end));
    }
    TemporalAxisKey::from_coordinate("", coordinate, axis)
        .into_iter()
        .filter(|key| key.axis() == TemporalKeyKind::Time)
        .any(|key| key < *end || (inclusive && key == *end))
}
