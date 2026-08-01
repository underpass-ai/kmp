use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::{DomainError, TemporalCoordinate, TemporalCursor, TemporalDirection};

use super::TemporalTraversalRequest;
use super::axis_key::{TemporalAxisKey, primary_coordinate_key};
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
) -> Result<ResolvedTemporalCursor, DomainError> {
    match cursor {
        TemporalCursor::Ref(ref_id) => positions
            .iter()
            .filter(|position| position.ref_id == *ref_id)
            .min()
            .map(|position| ResolvedTemporalCursor {
                axis_key: position.axis_key.clone(),
                coordinate: position.coordinate.clone(),
            })
            .ok_or_else(|| {
                DomainError::InvalidState(format!("temporal cursor ref not found: {ref_id}"))
            }),
        TemporalCursor::Time(value) => Ok(ResolvedTemporalCursor {
            axis_key: TemporalAxisKey::time(value),
            coordinate: TemporalCoordinate::cursor_time(value.clone())?,
        }),
        TemporalCursor::Sequence(value) => Ok(ResolvedTemporalCursor {
            axis_key: TemporalAxisKey::sequence(*value),
            coordinate: TemporalCoordinate::cursor_sequence(*value)?,
        }),
    }
}

pub(super) fn select_positions(
    positions: &[TemporalPosition],
    cursor: &ResolvedTemporalCursor,
    request: &TemporalTraversalRequest,
) -> TemporalSelection {
    let comparable = positions
        .iter()
        .filter(|position| position.axis_key.axis() == cursor.axis_key.axis())
        .cloned()
        .collect::<Vec<_>>();

    match request.direction() {
        TemporalDirection::Goto => {
            let candidates = comparable
                .into_iter()
                .filter(|position| position.axis_key <= cursor.axis_key)
                .collect::<Vec<_>>();
            select_limited(
                candidates,
                request.limit_entries().unwrap_or(DEFAULT_GOTO_ENTRIES),
                PageSide::Before,
            )
        }
        TemporalDirection::Rewind => {
            let candidates = comparable
                .into_iter()
                .filter(|position| position.axis_key < cursor.axis_key)
                .collect::<Vec<_>>();
            select_limited(
                candidates,
                request.limit_entries().unwrap_or(5),
                PageSide::Before,
            )
        }
        TemporalDirection::Forward => {
            let candidates = comparable
                .into_iter()
                .filter(|position| position.axis_key > cursor.axis_key)
                .collect::<Vec<_>>();
            select_limited(
                candidates,
                request.limit_entries().unwrap_or(5),
                PageSide::After,
            )
        }
        TemporalDirection::Near => {
            let before_candidates = comparable
                .iter()
                .filter(|position| position.axis_key < cursor.axis_key)
                .cloned()
                .collect::<Vec<_>>();
            let before = take_last(before_candidates.clone(), request.window().before_entries());
            let exact = comparable
                .iter()
                .filter(|position| position.axis_key == cursor.axis_key)
                .cloned()
                .collect::<Vec<_>>();
            let after_candidates = comparable
                .into_iter()
                .filter(|position| position.axis_key > cursor.axis_key)
                .collect::<Vec<_>>();
            let after = after_candidates
                .iter()
                .take(request.window().after_entries())
                .cloned()
                .collect::<Vec<_>>();
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
    let positions = match page_side {
        PageSide::Before => take_last(candidates, limit),
        PageSide::After => candidates.into_iter().take(limit).collect(),
    };
    let returned_refs = ordered_unique_ref_ids(positions.clone());
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

fn take_last(mut positions: Vec<TemporalPosition>, limit: usize) -> Vec<TemporalPosition> {
    positions.sort();
    let keep_from = positions.len().saturating_sub(limit);
    positions.into_iter().skip(keep_from).collect()
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
