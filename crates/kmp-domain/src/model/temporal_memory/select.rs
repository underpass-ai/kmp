use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::{DomainError, TemporalAxis, TemporalCoordinate, TemporalCursor, TemporalDirection};

use super::TemporalTraversalRequest;
use super::axis_key::{TemporalAxisKey, TemporalKeyKind, primary_coordinate_key};
use super::position::{ResolvedTemporalCursor, TemporalPosition};

const DEFAULT_GOTO_ENTRIES: usize = 50;

pub(super) struct TemporalSelection {
    pub positions: Vec<TemporalPosition>,
    pub total_unique_refs: usize,
    pub next_cursor: Option<String>,
}

pub(super) fn resolve_cursor(
    positions: &[TemporalPosition],
    cursor: &TemporalCursor,
    requested_axis: TemporalAxis,
) -> Result<ResolvedTemporalCursor, DomainError> {
    match cursor {
        TemporalCursor::Ref(ref_id) => {
            let first = positions
                .iter()
                .filter(|position| position.ref_id == *ref_id)
                .min()
                .ok_or_else(|| {
                    DomainError::InvalidState(format!("temporal cursor ref not found: {ref_id}"))
                })?;
            let selected = if requested_axis == TemporalAxis::Default {
                Some(first)
            } else {
                positions
                    .iter()
                    .filter(|position| position.ref_id == *ref_id)
                    .filter(|position| position.axis_key.axis() == TemporalKeyKind::Time)
                    .min()
            };

            Ok(ResolvedTemporalCursor {
                axis_key: selected.map(|position| position.axis_key.clone()),
                ref_id: Some(ref_id.clone()),
                coordinate: selected.unwrap_or(first).coordinate.clone(),
            })
        }
        TemporalCursor::Time(value) => Ok(ResolvedTemporalCursor {
            axis_key: Some(TemporalAxisKey::time(value)),
            ref_id: None,
            coordinate: TemporalCoordinate::cursor_time(value.clone(), requested_axis)?,
        }),
        TemporalCursor::Sequence(value) => Ok(ResolvedTemporalCursor {
            axis_key: Some(TemporalAxisKey::sequence(*value)),
            ref_id: None,
            coordinate: TemporalCoordinate::cursor_sequence(*value)?,
        }),
    }
}

pub(super) fn select_positions(
    positions: &[TemporalPosition],
    cursor: &ResolvedTemporalCursor,
    request: &TemporalTraversalRequest,
) -> TemporalSelection {
    let Some(cursor_axis_key) = cursor.axis_key.as_ref() else {
        return TemporalSelection {
            positions: Vec::new(),
            total_unique_refs: 0,
            next_cursor: None,
        };
    };
    let comparable = positions
        .iter()
        .filter(|position| position.axis_key.axis() == cursor_axis_key.axis())
        .cloned()
        .collect::<Vec<_>>();
    let partitions = partition_positions(comparable, cursor_axis_key, cursor.ref_id.as_deref());

    match request.direction() {
        TemporalDirection::Goto => {
            let candidates = partitions
                .before
                .into_iter()
                .chain(partitions.exact)
                .collect();
            select_limited(
                candidates,
                request.limit_entries().unwrap_or(DEFAULT_GOTO_ENTRIES),
                PageSide::Before,
            )
        }
        TemporalDirection::Rewind => select_limited(
            partitions.before,
            request.limit_entries().unwrap_or(5),
            PageSide::Before,
        ),
        TemporalDirection::Forward => select_limited(
            partitions.after,
            request.limit_entries().unwrap_or(5),
            PageSide::After,
        ),
        TemporalDirection::Near => {
            let before_candidates = partitions.before;
            let before = take_ref_page(
                before_candidates.clone(),
                request.window().before_entries(),
                PageSide::Before,
            )
            .0;
            let exact = partitions.exact;
            let after_candidates = partitions.after;
            let after = take_ref_page(
                after_candidates.clone(),
                request.window().after_entries(),
                PageSide::After,
            )
            .0;
            let before_more =
                unique_ref_count(before.iter()) < unique_ref_count(before_candidates.iter());
            let after_more =
                unique_ref_count(after.iter()) < unique_ref_count(after_candidates.iter());
            let total_unique_refs = unique_ref_count(
                before_candidates
                    .iter()
                    .chain(exact.iter())
                    .chain(after_candidates.iter()),
            );
            let positions = before
                .into_iter()
                .chain(exact)
                .chain(after)
                .collect::<Vec<_>>();
            let returned_refs = ordered_unique_ref_ids(positions.clone());
            let next_cursor = if returned_refs.len() < total_unique_refs {
                if after_more {
                    returned_refs.last().cloned()
                } else if before_more {
                    returned_refs.first().cloned()
                } else {
                    None
                }
            } else {
                None
            };

            TemporalSelection {
                positions,
                total_unique_refs,
                next_cursor,
            }
        }
    }
}

struct TemporalPartitions {
    before: Vec<TemporalPosition>,
    exact: Vec<TemporalPosition>,
    after: Vec<TemporalPosition>,
}

fn partition_positions(
    comparable: Vec<TemporalPosition>,
    cursor_axis_key: &TemporalAxisKey,
    cursor_ref: Option<&str>,
) -> TemporalPartitions {
    let Some(cursor_ref) = cursor_ref else {
        return TemporalPartitions {
            before: comparable
                .iter()
                .filter(|position| &position.axis_key < cursor_axis_key)
                .cloned()
                .collect(),
            exact: comparable
                .iter()
                .filter(|position| &position.axis_key == cursor_axis_key)
                .cloned()
                .collect(),
            after: comparable
                .into_iter()
                .filter(|position| &position.axis_key > cursor_axis_key)
                .collect(),
        };
    };

    let ordered_refs = ordered_unique_ref_ids(comparable.clone());
    let Some(anchor) = ordered_refs.iter().position(|ref_id| ref_id == cursor_ref) else {
        return TemporalPartitions {
            before: Vec::new(),
            exact: Vec::new(),
            after: Vec::new(),
        };
    };
    TemporalPartitions {
        before: positions_for_refs(&comparable, &ordered_refs[..anchor]),
        exact: positions_for_refs(&comparable, &ordered_refs[anchor..=anchor]),
        after: positions_for_refs(&comparable, &ordered_refs[anchor + 1..]),
    }
}

fn positions_for_refs(positions: &[TemporalPosition], refs: &[String]) -> Vec<TemporalPosition> {
    let refs = refs.iter().map(String::as_str).collect::<BTreeSet<_>>();
    positions
        .iter()
        .filter(|position| refs.contains(position.ref_id.as_str()))
        .cloned()
        .collect()
}

#[derive(Clone, Copy)]
enum PageSide {
    Before,
    After,
}

fn select_limited(
    candidates: Vec<TemporalPosition>,
    limit: usize,
    page_side: PageSide,
) -> TemporalSelection {
    let total_unique_refs = unique_ref_count(candidates.iter());
    let (positions, returned_refs) = take_ref_page(candidates, limit, page_side);
    let next_cursor = if returned_refs.len() < total_unique_refs {
        match page_side {
            PageSide::Before => returned_refs.first().cloned(),
            PageSide::After => returned_refs.last().cloned(),
        }
    } else {
        None
    };
    TemporalSelection {
        positions,
        total_unique_refs,
        next_cursor,
    }
}

fn take_ref_page(
    mut positions: Vec<TemporalPosition>,
    limit: usize,
    page_side: PageSide,
) -> (Vec<TemporalPosition>, Vec<String>) {
    positions.sort();
    let ordered_refs = ordered_unique_ref_ids(positions.clone());
    let keep_from = ordered_refs.len().saturating_sub(limit);
    let selected_refs = match page_side {
        PageSide::Before => ordered_refs.into_iter().skip(keep_from).collect::<Vec<_>>(),
        PageSide::After => ordered_refs.into_iter().take(limit).collect::<Vec<_>>(),
    };
    let selected = selected_refs.iter().cloned().collect::<BTreeSet<_>>();
    positions.retain(|position| selected.contains(&position.ref_id));
    (positions, selected_refs)
}

pub(super) fn ordered_unique_ref_ids(mut selected_positions: Vec<TemporalPosition>) -> Vec<String> {
    selected_positions.sort();
    let mut seen = BTreeSet::new();
    selected_positions
        .into_iter()
        .filter_map(|position| {
            if seen.insert(position.ref_id.clone()) {
                Some(position.ref_id)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn coordinates_by_ref(
    positions: &[TemporalPosition],
) -> BTreeMap<String, Vec<TemporalCoordinate>> {
    let mut coordinates = BTreeMap::<String, Vec<TemporalCoordinate>>::new();
    for position in positions {
        let entry = coordinates.entry(position.ref_id.clone()).or_default();
        if !entry.contains(&position.coordinate) {
            entry.push(position.coordinate.clone());
        }
    }

    for coordinates in coordinates.values_mut() {
        coordinates.sort_by(compare_coordinates);
    }

    coordinates
}

fn unique_ref_count<'a>(positions: impl IntoIterator<Item = &'a TemporalPosition>) -> usize {
    positions
        .into_iter()
        .map(|position| position.ref_id.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

fn compare_coordinates(left: &TemporalCoordinate, right: &TemporalCoordinate) -> Ordering {
    primary_coordinate_key(left)
        .cmp(&primary_coordinate_key(right))
        .then_with(|| left.dimension().cmp(right.dimension()))
        .then_with(|| left.scope_id().cmp(right.scope_id()))
}
