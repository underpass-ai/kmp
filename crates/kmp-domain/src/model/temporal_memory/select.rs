use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    DomainError, EntryLabels, TemporalAxis, TemporalCoordinate, TemporalCursor, TemporalDirection,
};

use super::TemporalTraversalRequest;
use super::axis_key::{TemporalAxisKey, TemporalKeyKind, primary_coordinate_key};
use super::position::{ResolvedTemporalCursor, TemporalPosition};

const DEFAULT_GOTO_ENTRIES: usize = 50;

pub(super) struct TemporalSelection<'a> {
    pub positions: Vec<&'a TemporalPosition>,
    pub total_unique_refs: usize,
    pub next_cursor: Option<String>,
}

pub(super) fn resolve_cursor(
    positions: &[&TemporalPosition],
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

pub(super) fn select_positions<'a>(
    positions: &'a [TemporalPosition],
    cursor: Option<&ResolvedTemporalCursor>,
    request: &TemporalTraversalRequest,
) -> TemporalSelection<'a> {
    if cursor.is_some_and(|cursor| cursor.axis_key.is_none()) {
        return TemporalSelection {
            positions: Vec::new(),
            total_unique_refs: 0,
            next_cursor: None,
        };
    }
    let cursor_axis_key = cursor.and_then(|cursor| cursor.axis_key.as_ref());
    let kind = cursor_axis_key.map_or(TemporalKeyKind::Time, TemporalAxisKey::axis);
    let first = positions.partition_point(|position| position.axis_key.axis() < kind);
    let end = positions.partition_point(|position| position.axis_key.axis() <= kind);
    let mut indexed = &positions[first..end];
    if request.axis() != TemporalAxis::Validity
        && let Some(interval) = request.interval()
    {
        let first = interval.start().map_or(0, |start| {
            let start = TemporalAxisKey::time(start);
            indexed.partition_point(|position| position.axis_key < start)
        });
        let end = interval.end().map_or(indexed.len(), |end| {
            let end = TemporalAxisKey::time(end);
            indexed.partition_point(|position| position.axis_key < end)
        });
        indexed = &indexed[first..end];
    }
    let mut comparable = indexed
        .iter()
        .filter(|position| {
            request.interval().is_none_or(|interval| {
                if request.axis() == TemporalAxis::Validity {
                    interval.overlaps(
                        position.coordinate.valid_from(),
                        position.coordinate.valid_until(),
                    )
                } else {
                    position.axis_key.in_interval(interval)
                }
            })
        })
        .collect::<Vec<_>>();

    let Some(cursor_axis_key) = cursor_axis_key else {
        let labels = labels_for_positions(comparable.iter().copied());
        comparable.retain(|position| entry_selected(position, request, &labels));
        // A direct interval starts at the selected range itself. Do not invent
        // a time just before its start or lose memories tied at that boundary.
        let side = if request.direction() == TemporalDirection::Rewind {
            PageSide::Before
        } else {
            PageSide::After
        };
        return select_limited(comparable, request.limit_entries().unwrap_or(5), side);
    };

    // Every time-based move on the validity clock rejects intervals that had
    // already ended at the cursor. `valid_until` is exclusive. Ref and
    // sequence cursors retain their historical navigation semantics so page
    // continuations can still walk recorded positions.
    let validity_time_cursor = request.axis() == TemporalAxis::Validity
        && matches!(request.cursor(), Some(TemporalCursor::Time(_)))
        && request.interval().is_none();
    if validity_time_cursor {
        comparable.retain(|position| validity_not_ended(&position.coordinate, cursor_axis_key));
    }

    // Goto is an as-of projection, not a history of interval starts. The
    // direction branches below still partition rewind, near and forward by
    // their validity start (or their end when the start is open).
    if validity_time_cursor && request.direction() == TemporalDirection::Goto {
        let mut active: Vec<_> = comparable
            .into_iter()
            .filter(|position| validity_contains(&position.coordinate, cursor_axis_key))
            .collect();
        let labels = labels_for_positions(active.iter().copied());
        active.retain(|position| entry_selected(position, request, &labels));
        return select_limited(
            active,
            request.limit_entries().unwrap_or(DEFAULT_GOTO_ENTRIES),
            PageSide::Before,
        );
    }

    // A Goto ref is still an as-of state. A later coordinate of an old
    // entry must not leak through whole-entry ref ordering.
    if request.direction() == TemporalDirection::Goto {
        comparable.retain(|position| position.axis_key <= *cursor_axis_key);
    }
    let mut partitions = partition_positions(
        comparable,
        cursor_axis_key,
        cursor.and_then(|cursor| cursor.ref_id.as_deref()),
        request,
    );
    let labels = labels_for_positions(
        partitions
            .before
            .iter()
            .filter(|_| request.direction() != TemporalDirection::Forward)
            .chain(partitions.exact.iter().filter(|_| {
                matches!(
                    request.direction(),
                    TemporalDirection::Goto | TemporalDirection::Near
                )
            }))
            .chain(partitions.after.iter().filter(|_| {
                matches!(
                    request.direction(),
                    TemporalDirection::Forward | TemporalDirection::Near
                )
            }))
            .copied(),
    );
    // Resolve the original ref's ordered position before focusing entries.
    // An unselected anchor still determines the sides; limits apply afterward.
    for side in [
        &mut partitions.before,
        &mut partitions.exact,
        &mut partitions.after,
    ] {
        side.retain(|position| entry_selected(position, request, &labels));
    }

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

fn validity_contains(coordinate: &TemporalCoordinate, cursor_axis_key: &TemporalAxisKey) -> bool {
    let started = coordinate
        .valid_from()
        .is_none_or(|start| TemporalAxisKey::time(start) <= *cursor_axis_key);
    started && validity_not_ended(coordinate, cursor_axis_key)
}

fn labels_for_positions<'a>(
    positions: impl Iterator<Item = &'a TemporalPosition>,
) -> BTreeMap<String, EntryLabels> {
    let mut coordinates = BTreeMap::<String, Vec<(&str, &str)>>::new();
    for position in positions {
        coordinates
            .entry(position.ref_id.clone())
            .or_default()
            .push((
                position.coordinate.dimension(),
                position.coordinate.scope_id(),
            ));
    }
    coordinates
        .into_iter()
        .map(|(id, coords)| (id, EntryLabels::from_coordinates(coords)))
        .collect()
}

fn entry_selected(
    position: &TemporalPosition,
    request: &TemporalTraversalRequest,
    labels: &BTreeMap<String, EntryLabels>,
) -> bool {
    request.dimensions().includes_coordinate(
        position.coordinate.dimension(),
        position.coordinate.scope_id(),
    ) && request.dimensions().admits(
        labels
            .get(&position.ref_id)
            .unwrap_or(&EntryLabels::default()),
    ) && request
        .entry_selection()
        .is_none_or(|selection| selection.admits(&position.ref_id))
}

fn validity_not_ended(coordinate: &TemporalCoordinate, cursor_axis_key: &TemporalAxisKey) -> bool {
    coordinate
        .valid_until()
        .is_none_or(|end| TemporalAxisKey::time(end) > *cursor_axis_key)
}

struct TemporalPartitions<'a> {
    before: Vec<&'a TemporalPosition>,
    exact: Vec<&'a TemporalPosition>,
    after: Vec<&'a TemporalPosition>,
}

fn partition_positions<'a>(
    comparable: Vec<&'a TemporalPosition>,
    cursor_axis_key: &TemporalAxisKey,
    cursor_ref: Option<&str>,
    request: &TemporalTraversalRequest,
) -> TemporalPartitions<'a> {
    let Some(cursor_ref) = cursor_ref else {
        return TemporalPartitions {
            before: comparable
                .iter()
                .filter(|position| &position.axis_key < cursor_axis_key)
                .copied()
                .collect(),
            exact: comparable
                .iter()
                .filter(|position| &position.axis_key == cursor_axis_key)
                .copied()
                .collect(),
            after: comparable
                .into_iter()
                .filter(|position| &position.axis_key > cursor_axis_key)
                .collect(),
        };
    };

    let ordered_refs = ordered_unique_ref_ids(
        comparable
            .iter()
            .filter(|position| {
                request.dimensions().includes_coordinate(
                    position.coordinate.dimension(),
                    position.coordinate.scope_id(),
                )
            })
            .copied()
            .collect(),
    );
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

fn positions_for_refs<'a>(
    positions: &[&'a TemporalPosition],
    refs: &[String],
) -> Vec<&'a TemporalPosition> {
    let refs = refs.iter().map(String::as_str).collect::<BTreeSet<_>>();
    positions
        .iter()
        .filter(|position| refs.contains(position.ref_id.as_str()))
        .copied()
        .collect()
}

#[derive(Clone, Copy)]
enum PageSide {
    Before,
    After,
}

fn select_limited<'a>(
    candidates: Vec<&'a TemporalPosition>,
    limit: usize,
    page_side: PageSide,
) -> TemporalSelection<'a> {
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
    mut positions: Vec<&TemporalPosition>,
    limit: usize,
    page_side: PageSide,
) -> (Vec<&TemporalPosition>, Vec<String>) {
    let ordered_refs = unique_refs_in_order(positions.iter().copied());
    let keep_from = ordered_refs.len().saturating_sub(limit);
    let selected_refs = match page_side {
        PageSide::Before => ordered_refs.into_iter().skip(keep_from).collect::<Vec<_>>(),
        PageSide::After => ordered_refs.into_iter().take(limit).collect::<Vec<_>>(),
    };
    let selected = selected_refs.iter().cloned().collect::<BTreeSet<_>>();
    positions.retain(|position| selected.contains(&position.ref_id));
    (positions, selected_refs)
}

pub(super) fn ordered_unique_ref_ids(
    mut selected_positions: Vec<&TemporalPosition>,
) -> Vec<String> {
    selected_positions.sort();
    unique_refs_in_order(selected_positions.into_iter())
}

fn unique_refs_in_order<'a>(positions: impl Iterator<Item = &'a TemporalPosition>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    positions
        .filter_map(|position| {
            if seen.insert(position.ref_id.as_str()) {
                Some(position.ref_id.clone())
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn coordinates_by_ref(
    positions: &[&TemporalPosition],
) -> BTreeMap<String, Vec<TemporalCoordinate>> {
    let mut coordinates = BTreeMap::<String, Vec<TemporalCoordinate>>::new();
    for position in positions {
        let entry = coordinates.entry(position.ref_id.clone()).or_default();
        if !entry.contains(&position.coordinate) {
            entry.push(position.coordinate.clone());
        }
    }

    for coordinates in coordinates.values_mut() {
        coordinates.sort_by(compare_temporal_coordinates);
    }

    coordinates
}

fn unique_ref_count<'a: 'b, 'b>(
    positions: impl IntoIterator<Item = &'b &'a TemporalPosition>,
) -> usize {
    positions
        .into_iter()
        .map(|position| position.ref_id.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

/// Stable record-coordinate order shared by history and dependency projections.
pub fn compare_temporal_coordinates(
    left: &TemporalCoordinate,
    right: &TemporalCoordinate,
) -> Ordering {
    primary_coordinate_key(left)
        .cmp(&primary_coordinate_key(right))
        .then_with(|| left.dimension().cmp(right.dimension()))
        .then_with(|| left.scope_id().cmp(right.scope_id()))
}
